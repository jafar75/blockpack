use std::thread;
use std::time::Duration;
use crossbeam_channel::unbounded;

use blockpack::{
    Tx, BuilderCommand, BuilderEvent, BuilderLoopConfig,
    PollingBuilder, run_builder_loop,
};

fn main() {
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!();
    
    // Demo 1: PollingBuilder (simple synchronous usage)
    demo_polling_builder();
    
    println!();
    println!("{:=<70}", "");
    println!();
    
    // Demo 2: Full async builder loop with channels
    demo_builder_loop();
}

fn demo_polling_builder() {
    println!("Demo 1: PollingBuilder (synchronous)");
    println!("{:-<70}", "");
    
    let config = BuilderLoopConfig {
        block_gas_limit: 200_000,
        block_time: Duration::from_millis(500),
        start_block: 1,
    };
    
    let mut builder = PollingBuilder::new(config);
    
    // Simulate incoming transactions
    let txs = vec![
        Tx::new(1, 50_000, 10),
        Tx::new(2, 60_000, 20),
        Tx::new(3, 40_000, 30),
        Tx::new(4, 50_000, 40),
        Tx::new(5, 45_000, 25),
    ];
    
    println!("Processing {} transactions...", txs.len());
    println!();
    
    for tx in txs {
        let result = builder.process_tx(tx);
        let candidate = builder.candidate().unwrap();
        println!(
            "  TX {} -> {:?}, block: {} gas, {} value",
            candidate.transactions.last().map(|t| t.id).unwrap_or(0),
            result,
            candidate.gas_used,
            candidate.total_value
        );
    }
    
    // Seal the block
    println!();
    let block = builder.force_seal().unwrap();
    println!("Sealed block #{}:", block.number);
    println!("  Transactions: {}", block.tx_count());
    println!("  Gas: {} / {}", block.gas_used, block.gas_limit);
    println!("  Value: {}", block.total_value);
    
    let stats = builder.stats();
    println!();
    println!("Stats: {} received, {} included, {:.0}% inclusion rate",
        stats.total_txs_received,
        stats.total_txs_included,
        stats.inclusion_rate() * 100.0
    );
}

fn demo_builder_loop() {
    println!("Demo 2: Builder Loop (async with channels)");
    println!("{:-<70}", "");
    
    let config = BuilderLoopConfig {
        block_gas_limit: 150_000,
        block_time: Duration::from_millis(200), // Fast blocks for demo
        start_block: 100,
    };
    
    // Create channels
    let (tx_sender, tx_receiver) = unbounded();
    let (cmd_sender, cmd_receiver) = unbounded();
    let (event_sender, event_receiver) = unbounded();
    
    // Spawn builder loop
    let loop_handle = thread::spawn(move || {
        run_builder_loop(config, tx_receiver, cmd_receiver, Some(event_sender))
    });
    
    // Spawn event listener
    let event_handle = thread::spawn(move || {
        let mut blocks_seen = 0;
        while let Ok(event) = event_receiver.recv() {
            match event {
                BuilderEvent::TxProcessed { tx_id, result, block_value, .. } => {
                    println!("  [event] TX {} processed: {:?}, value now {}",
                        tx_id, result, block_value);
                }
                BuilderEvent::BlockSealed { block, stats, build_duration } => {
                    blocks_seen += 1;
                    println!();
                    println!("  [event] Block #{} sealed in {:?}", block.number, build_duration);
                    println!("          {} txs, {} gas, {} value",
                        block.tx_count(), block.gas_used, block.total_value);
                    println!("          acceptance rate: {:.0}%", stats.acceptance_rate() * 100.0);
                }
                BuilderEvent::Shutdown => {
                    println!();
                    println!("  [event] Builder shut down, {} blocks produced", blocks_seen);
                    break;
                }
            }
        }
    });
    
    // Send transactions in bursts
    println!("Sending transactions...");
    println!();
    
    let tx_batches = vec![
        vec![
            Tx::new(1, 30_000, 10),
            Tx::new(2, 40_000, 15),
            Tx::new(3, 35_000, 20),
        ],
        vec![
            Tx::new(4, 50_000, 25),
            Tx::new(5, 45_000, 30),
        ],
        vec![
            Tx::new(6, 60_000, 35),
            Tx::new(7, 30_000, 40),
            Tx::new(8, 40_000, 50),
        ],
    ];
    
    for (i, batch) in tx_batches.into_iter().enumerate() {
        println!("Sending batch {}...", i + 1);
        for tx in batch {
            tx_sender.send(tx).unwrap();
        }
        // Wait a bit between batches
        thread::sleep(Duration::from_millis(150));
    }
    
    // Let it run a bit more
    thread::sleep(Duration::from_millis(300));
    
    // Shutdown
    println!();
    println!("Sending shutdown command...");
    cmd_sender.send(BuilderCommand::Shutdown).unwrap();
    
    // Wait for completion
    let stats = loop_handle.join().unwrap();
    event_handle.join().unwrap();
    
    println!();
    println!("Final loop stats:");
    println!("  Blocks built: {}", stats.blocks_built);
    println!("  Total txs received: {}", stats.total_txs_received);
    println!("  Total txs included: {}", stats.total_txs_included);
    println!("  Total value: {}", stats.total_value_extracted);
    println!("  Avg block value: {:.0}", stats.avg_block_value());
    println!("  Avg block fullness: {:.1}%", stats.avg_block_fullness(150_000));
    println!("  Run duration: {:?}", stats.run_duration);
    
    println!();
    println!("TODO: Add stream simulation (step 3.1)");
}