use std::time::Duration;

use blockpack::{
    Tx, Pool, Builder, PoolPressure,
    SimConfig, StreamConfig, BuilderMetrics, SimulationOutput,
    run_simulation, run_monte_carlo,
};

fn main() {
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!();
    
    // Demo 1: EIP-1559 transactions
    demo_eip1559();
    
    println!();
    println!("{:=<70}", "");
    println!();
    
    // Demo 2: Pool pressure management
    demo_pool_pressure();
    
    println!();
    println!("{:=<70}", "");
    println!();
    
    // Demo 3: Full simulation with metrics
    demo_simulation_with_metrics();
}

fn demo_eip1559() {
    println!("Demo 1: EIP-1559 Transaction Support");
    println!("{:-<70}", "");
    println!();
    
    let mut builder = Builder::new(200_000);
    builder.set_base_fee(30); // Set base fee to 30 gwei
    
    println!("Base fee: {} gwei", builder.base_fee());
    println!();
    
    // Create mix of legacy and EIP-1559 transactions
    let txs = vec![
        // Legacy tx: priority_fee = 50
        ("Legacy", Tx::new(1, 50_000, 50)),
        
        // EIP-1559: max_fee=100, priority=40 -> effective = min(40, 100-30) = 40
        ("EIP-1559 (fits)", Tx::new_eip1559(2, 50_000, 40, 100)),
        
        // EIP-1559: max_fee=50, priority=30 -> effective = min(30, 50-30) = 20
        ("EIP-1559 (capped)", Tx::new_eip1559(3, 50_000, 30, 50)),
        
        // EIP-1559: max_fee=25 < base_fee=30 -> rejected
        ("EIP-1559 (can't afford)", Tx::new_eip1559(4, 50_000, 20, 25)),
    ];
    
    println!("{:<25} {:>10} {:>10} {:>10} {:>15}", 
        "Type", "Gas", "Priority", "MaxFee", "Result");
    println!("{:-<70}", "");
    
    for (name, tx) in txs {
        let gas = tx.gas_limit;
        let priority = tx.priority_fee_gwei;
        let max_fee = tx.max_fee_gwei.map(|f| f.to_string()).unwrap_or("-".to_string());
        
        let result = builder.process_tx(tx);
        
        println!("{:<25} {:>10} {:>10} {:>10} {:>15?}", 
            name, gas, priority, max_fee, result);
    }
    
    println!();
    let candidate = builder.candidate().unwrap();
    println!("Block result: {} txs, {} gas, {} value",
        candidate.tx_count(), candidate.gas_used, candidate.total_value);
    
    let stats = builder.stats();
    println!("Stats: {} added, {} insufficient fee, {} rejected",
        stats.txs_added, stats.txs_insufficient_fee, stats.txs_rejected);
}

fn demo_pool_pressure() {
    println!("Demo 2: Pool Pressure Management");
    println!("{:-<70}", "");
    println!();
    
    let mut pool = Pool::new(20); // Small pool to show pressure quickly
    
    println!("Pool capacity: 20 transactions");
    println!();
    
    // Fill pool gradually and show pressure changes
    let checkpoints = [5, 10, 16, 19, 20];
    let mut next_checkpoint = 0;
    
    println!("{:>5} {:>10} {:>12} {:>15}", "TXs", "Fill%", "Pressure", "Min Threshold");
    println!("{:-<50}", "");
    
    for i in 1..=25 {
        let fee = 10 + (i % 20); // Varying fees
        let result = pool.insert(Tx::new(i, 21_000, fee));
        
        if next_checkpoint < checkpoints.len() && pool.len() >= checkpoints[next_checkpoint] {
            let stats = pool.stats();
            println!("{:>5} {:>9.1}% {:>12?} {:>15.1}",
                stats.count, 
                pool.fill_percent(),
                stats.pressure,
                stats.min_threshold);
            next_checkpoint += 1;
        }
        
        // Show rejections at critical pressure
        if result.is_some() && pool.pressure() == PoolPressure::Critical {
            println!("  -> TX {} rejected (fee {} < threshold)", i, fee);
        }
    }
    
    println!();
    let stats = pool.stats();
    println!("Final stats:");
    println!("  Inserted: {}", stats.total_inserted);
    println!("  Evicted: {}", stats.total_evicted);
    println!("  Rejected: {}", stats.total_rejected);
}

fn demo_simulation_with_metrics() {
    println!("Demo 3: Simulation with Metrics");
    println!("{:-<70}", "");
    println!();
    
    let config = SimConfig {
        block_gas_limit: 1_000_000,
        block_time: Duration::from_secs(12),
        sim_duration: Duration::from_secs(120),
        stream_config: StreamConfig {
            tx_rate: 50.0,
            min_gas: 21_000,
            max_gas: 300_000,
            min_fee: 5,
            max_fee: 150,
            log_normal_fees: true,
            fee_mu: 2.5,
            fee_sigma: 0.7,
            ..Default::default()
        },
        start_block: 1,
    };
    
    println!("Running simulation...");
    println!("  Duration: {:?}", config.sim_duration);
    println!("  Block time: {:?}", config.block_time);
    println!("  TX rate: {}/s", config.stream_config.tx_rate);
    println!();
    
    let result = run_simulation(config.clone());
    
    println!("Completed in {:?} (real time)", result.real_duration);
    println!("Speedup: {:.0}x", result.speedup());
    println!();
    
    // Build metrics from results
    let mut metrics = BuilderMetrics::new();
    
    for stats in &result.block_stats {
        let avg_gas = if stats.tx_count > 0 {
            stats.gas_used / stats.tx_count as u64
        } else {
            0
        };
        let avg_fee = if stats.gas_used > 0 {
            stats.total_value / stats.gas_used
        } else {
            0
        };
        
        for _ in 0..stats.tx_count {
            metrics.record_tx(avg_gas, avg_fee, true, false);
        }
        
        metrics.record_block(
            stats.tx_count,
            stats.gas_used,
            stats.total_value,
            config.block_gas_limit,
        );
    }
    
    // Display metrics
    let report = metrics.report();
    println!("{}", report.display());
    
    // JSON output sample
    println!("JSON Summary:");
    let output = SimulationOutput::from_result(&result, config.block_gas_limit);
    println!("{}", serde_json::to_string_pretty(&output.summary).unwrap());
    
    println!();
    println!("{:-<70}", "");
    println!();
    
    // Quick Monte Carlo
    println!("Monte Carlo (5 runs of 60s each):");
    let mc_config = SimConfig {
        sim_duration: Duration::from_secs(60),
        ..config
    };
    
    let mc = run_monte_carlo(mc_config, 5);
    println!("  Avg blocks: {:.1}", mc.avg_blocks());
    println!("  Avg value: {:.0}", mc.avg_total_value());
    println!("  Avg inclusion: {:.1}%", mc.avg_inclusion_rate() * 100.0);
    println!("  Value std dev: {:.0}", mc.value_std_dev());
}