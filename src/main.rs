use blockpack::config::Config;
use blockpack::tx::Tx;
use blockpack::pool::Pool;
use blockpack::builder::Builder;

fn main() {
    let config = Config::default();
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!();
    
    // Create pool and add transactions
    let mut pool = Pool::new(config.max_pool_size);
    
    let incoming_txs = vec![
        Tx::new(1, 21_000, 50),
        Tx::new(2, 100_000, 30),
        Tx::new(3, 250_000, 10),
        Tx::new(4, 21_000, 100),
        Tx::new(5, 500_000, 5),
        Tx::new(6, 45_000, 45),
        Tx::new(7, 150_000, 35),
        Tx::new(8, 80_000, 60),
        Tx::new(9, 30_000, 25),
        Tx::new(10, 200_000, 15),
    ];
    
    println!("Incoming transactions:");
    println!("{:-<65}", "");
    println!("{:>4} {:>10} {:>8} {:>12} {:>10}", "ID", "Gas", "Fee", "Value", "Density");
    println!("{:-<65}", "");
    
    for tx in incoming_txs {
        println!(
            "{:>4} {:>10} {:>8} {:>12} {:>10.2}",
            tx.id,
            tx.gas_limit,
            tx.priority_fee_gwei,
            tx.value(),
            tx.value_density()
        );
        pool.insert(tx);
    }
    println!("{:-<65}", "");
    
    let stats = pool.stats();
    println!(
        "Pool: {} txs, {} total gas, density range {:.0}-{:.0}",
        stats.count, stats.total_gas, stats.min_density, stats.max_density
    );
    
    // Build with limited gas to show algorithm behavior
    let limited_gas = 500_000;
    println!();
    println!("Building block with {} gas limit...", limited_gas);
    println!();
    
    // Compare greedy vs greedy with backfill
    let mut builder1 = Builder::new(limited_gas);
    let result1 = builder1.build_greedy(&pool);
    
    let mut builder2 = Builder::new(limited_gas);
    let result2 = builder2.build_greedy_with_backfill(&pool);
    
    println!("=== Greedy by Density ===");
    print_build_result(&result1);
    
    println!();
    println!("=== Greedy with Backfill ===");
    print_build_result(&result2);
    
    // Show improvement
    if result2.block.total_value > result1.block.total_value {
        let improvement = result2.block.total_value - result1.block.total_value;
        let pct = (improvement as f64 / result1.block.total_value as f64) * 100.0;
        println!();
        println!(
            "Backfill improved value by {} ({:.2}%)",
            improvement, pct
        );
    }
    
    // Now build with full gas limit
    println!();
    println!("{:=<65}", "");
    println!("Building block with full {} gas limit...", config.block_gas_limit);
    
    let mut builder = Builder::new(config.block_gas_limit);
    let result = builder.build_greedy(&pool);
    
    println!();
    print_build_result(&result);
    
    println!();
    println!("TODO: Add incremental improvement");
}

fn print_build_result(result: &blockpack::builder::BuildResult) {
    println!("Block #{}", result.block.number);
    println!("  Considered: {} txs", result.considered);
    println!("  Included: {} txs", result.included.len());
    println!("  Skipped: {} txs", result.skipped);
    println!("  Gas: {} / {} ({:.2}%)", 
        result.block.gas_used,
        result.block.gas_limit,
        result.block.fullness()
    );
    println!("  Value: {}", result.block.total_value);
    
    println!("  Transactions:");
    for tx in &result.block.transactions {
        println!(
            "    TX {:>2}: gas {:>7}, fee {:>3}, value {:>10}",
            tx.id, tx.gas_limit, tx.priority_fee_gwei, tx.value()
        );
    }
}