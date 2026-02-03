use std::thread;
use std::time::Duration;
use crossbeam_channel::unbounded;

use blockpack::{
    BuilderCommand, BuilderEvent, BuilderLoopConfig,
    StreamConfig, spawn_stream, run_builder_loop,
};

fn main() {
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!();
    
    // Full end-to-end demo: stream -> builder -> blocks
    demo_full_pipeline();
}

fn demo_full_pipeline() {
    println!("Full Pipeline Demo: Stream -> Builder -> Blocks");
    println!("{:-<70}", "");
    println!();
    
    // Configuration
    let stream_config = StreamConfig {
        tx_rate: 100.0,              // 100 tx/s
        tx_count: Some(200),         // Generate 200 txs total
        min_gas: 21_000,
        max_gas: 300_000,
        min_fee: 5,
        max_fee: 150,
        log_normal_fees: true,
        fee_mu: 2.5,                 // ~12 gwei median
        fee_sigma: 0.7,
        ..Default::default()
    };
    
    let builder_config = BuilderLoopConfig {
        block_gas_limit: 1_000_000,  // 1M gas per block (small for demo)
        block_time: Duration::from_millis(500), // Fast blocks
        start_block: 1,
    };
    
    println!("Stream config:");
    println!("  TX rate: {}/s", stream_config.tx_rate);
    println!("  TX count: {:?}", stream_config.tx_count);
    println!("  Gas range: {} - {}", stream_config.min_gas, stream_config.max_gas);
    println!("  Fee range: {} - {} gwei", stream_config.min_fee, stream_config.max_fee);
    println!();
    println!("Builder config:");
    println!("  Block gas limit: {}", builder_config.block_gas_limit);
    println!("  Block time: {:?}", builder_config.block_time);
    println!();
    
    // Create channels
    let (tx_sender, tx_receiver) = unbounded();
    let (cmd_sender, cmd_receiver) = unbounded();
    let (event_sender, event_receiver) = unbounded();
    
    // Spawn stream generator
    println!("Starting stream generator...");
    let stream_handle = spawn_stream(stream_config, tx_sender);
    
    // Spawn builder loop
    println!("Starting builder loop...");
    let builder_handle = thread::spawn({
        let config = builder_config.clone();
        move || run_builder_loop(config, tx_receiver, cmd_receiver, Some(event_sender))
    });
    
    // Collect and display events
    println!("Processing...");
    println!();
    
    let mut blocks_received = 0;
    let mut last_block_txs = 0;
    let mut last_block_value = 0;
    
    loop {
        match event_receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(BuilderEvent::TxProcessed { .. }) => {
                // Don't print every tx to avoid spam
            }
            Ok(BuilderEvent::BlockSealed { block, stats, build_duration }) => {
                blocks_received += 1;
                last_block_txs = block.tx_count();
                last_block_value = block.total_value;
                
                println!(
                    "Block #{:3} | {:3} txs | {:7} gas ({:5.1}%) | {:10} value | built in {:6.2?} | accept {:.0}%",
                    block.number,
                    block.tx_count(),
                    block.gas_used,
                    block.fullness(),
                    block.total_value,
                    build_duration,
                    stats.acceptance_rate() * 100.0,
                );
            }
            Ok(BuilderEvent::Shutdown) => {
                println!();
                println!("Builder shutdown received.");
                break;
            }
            Err(_) => {
                // Timeout - check if stream is done
                if stream_handle.is_finished() {
                    // Give builder a moment to process remaining txs
                    thread::sleep(Duration::from_millis(200));
                    cmd_sender.send(BuilderCommand::Shutdown).ok();
                }
            }
        }
    }
    
    // Get final stats
    let stream_stats = stream_handle.join().unwrap();
    let builder_stats = builder_handle.join().unwrap();
    
    println!();
    println!("{:=<70}", "");
    println!("FINAL STATISTICS");
    println!("{:=<70}", "");
    println!();
    
    println!("Stream Statistics:");
    println!("  Transactions generated: {}", stream_stats.txs_generated);
    println!("  Total gas generated: {}", stream_stats.total_gas);
    println!("  Total value generated: {}", stream_stats.total_value);
    println!("  Fee range: {} - {} gwei", stream_stats.min_fee, stream_stats.max_fee);
    println!("  Average fee: {:.2} gwei", stream_stats.avg_fee);
    println!("  Actual rate: {:.1} tx/s", stream_stats.actual_rate);
    println!("  Duration: {:?}", stream_stats.duration);
    println!();
    
    println!("Builder Statistics:");
    println!("  Blocks built: {}", builder_stats.blocks_built);
    println!("  Transactions received: {}", builder_stats.total_txs_received);
    println!("  Transactions included: {}", builder_stats.total_txs_included);
    println!("  Inclusion rate: {:.1}%", builder_stats.inclusion_rate() * 100.0);
    println!("  Total value extracted: {}", builder_stats.total_value_extracted);
    println!("  Total gas used: {}", builder_stats.total_gas_used);
    println!("  Average block value: {:.0}", builder_stats.avg_block_value());
    println!("  Average block fullness: {:.1}%", builder_stats.avg_block_fullness(builder_config.block_gas_limit));
    println!("  Run duration: {:?}", builder_stats.run_duration);
    println!();
    
    // Efficiency metrics
    let value_capture = if stream_stats.total_value > 0 {
        builder_stats.total_value_extracted as f64 / stream_stats.total_value as f64 * 100.0
    } else {
        0.0
    };
    
    println!("Efficiency:");
    println!("  Value captured: {:.1}% of total generated", value_capture);
    println!("  Blocks per second: {:.2}", 
        builder_stats.blocks_built as f64 / builder_stats.run_duration.as_secs_f64());
    
    println!();
    println!("Done!");
}