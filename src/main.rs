use blockpack::config::Config;
use blockpack::tx::Tx;
use blockpack::block::Block;
use blockpack::pool::Pool;

fn main() {
    let config = Config::default();
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!("Block gas limit: {}", config.block_gas_limit);
    println!("Target block time: {}ms", config.target_block_time_ms);
    println!("TX arrival rate: {}/s", config.tx_arrival_rate);
    println!();
    
    // Demo: simulate some transactions arriving
    let mut pool = Pool::new(config.max_pool_size);
    
    let incoming_txs = vec![
        Tx::new(1, 21_000, 50),    // Simple transfer, high fee
        Tx::new(2, 100_000, 30),   // Contract call, medium fee
        Tx::new(3, 250_000, 10),   // Complex tx, low fee
        Tx::new(4, 21_000, 100),   // High priority transfer
        Tx::new(5, 500_000, 5),    // Very large, low fee
        Tx::new(6, 45_000, 45),    // Medium size, good fee
    ];
    
    println!("Incoming transactions:");
    for tx in incoming_txs {
        println!(
            "  TX {:2} - gas: {:>7}, fee: {:>3} gwei, density: {:>6.2}",
            tx.id,
            tx.gas_limit,
            tx.priority_fee_gwei,
            tx.value_density()
        );
        pool.insert(tx);
    }
    
    println!();
    println!("Pool stats:");
    let stats = pool.stats();
    println!("  Count: {}", stats.count);
    println!("  Total gas: {}", stats.total_gas);
    println!("  Total value: {}", stats.total_value);
    println!("  Density range: {:.2} - {:.2}", stats.min_density, stats.max_density);
    println!("  Avg density: {:.2}", stats.avg_density);
    
    println!();
    println!("Transactions sorted by density (best first):");
    for tx in pool.sorted_by_density() {
        println!(
            "  TX {:2} - density: {:>6.2}, gas: {:>7}, value: {:>10}",
            tx.id,
            tx.value_density(),
            tx.gas_limit,
            tx.value()
        );
    }
    
    // Build a block using greedy approach (preview of step 2.1)
    println!();
    println!("Building block (greedy by density)...");
    let mut block = Block::new(1, config.block_gas_limit);
    
    for tx in pool.sorted_by_density() {
        if block.can_fit(tx) {
            block.add_tx(tx.clone());
        }
    }
    
    println!();
    println!("Block {} result:", block.number);
    println!("  Transactions: {}", block.tx_count());
    println!("  Gas used: {} / {}", block.gas_used, block.gas_limit);
    println!("  Fullness: {:.4}%", block.fullness());
    println!("  Total value: {}", block.total_value);
    
    println!();
    println!("Included transactions:");
    for tx in &block.transactions {
        println!(
            "  TX {:2} - gas: {:>7}, fee: {:>3} gwei, value: {:>10}",
            tx.id,
            tx.gas_limit,
            tx.priority_fee_gwei,
            tx.value()
        );
    }
    
    println!();
    println!("TODO: Wire up stream -> pool -> builder pipeline");
}