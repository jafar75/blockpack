use std::time::Duration;

use blockpack::{
    SimConfig, StreamConfig,
    run_simulation, run_monte_carlo,
};

fn main() {
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!();
    
    // Demo 1: Single simulation run
    demo_single_simulation();
    
    println!();
    println!("{:=<70}", "");
    println!();
    
    // Demo 2: Monte Carlo analysis
    demo_monte_carlo();
}

fn demo_single_simulation() {
    println!("Demo 1: Discrete Event Simulation");
    println!("{:-<70}", "");
    println!();
    
    let config = SimConfig {
        block_gas_limit: 1_000_000,
        block_time: Duration::from_secs(12),
        sim_duration: Duration::from_secs(120), // 2 minutes simulated
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
    
    println!("Configuration:");
    println!("  Simulated duration: {:?}", config.sim_duration);
    println!("  Block time: {:?}", config.block_time);
    println!("  Block gas limit: {}", config.block_gas_limit);
    println!("  TX rate: {}/s", config.stream_config.tx_rate);
    println!();
    
    println!("Running simulation...");
    let result = run_simulation(config.clone());
    
    println!("Done in {:?} (real time)", result.real_duration);
    println!();
    
    println!("Block Results:");
    println!("{:-<70}", "");
    println!("{:>6} {:>8} {:>10} {:>12} {:>10} {:>10}",
        "Block", "TXs", "Gas", "Value", "Fullness", "Accept%");
    println!("{:-<70}", "");
    
    for stats in &result.block_stats {
        let fullness = (stats.gas_used as f64 / config.block_gas_limit as f64) * 100.0;
        println!("{:>6} {:>8} {:>10} {:>12} {:>9.1}% {:>9.1}%",
            stats.block_number,
            stats.tx_count,
            stats.gas_used,
            stats.total_value,
            fullness,
            stats.incremental_stats.acceptance_rate() * 100.0,
        );
    }
    println!("{:-<70}", "");
    
    println!();
    println!("Summary:");
    println!("  Blocks produced: {}", result.blocks.len());
    println!("  TXs generated: {}", result.txs_generated);
    println!("  TXs included: {}", result.txs_included);
    println!("  Inclusion rate: {:.1}%", result.inclusion_rate() * 100.0);
    println!("  Total value: {}", result.total_value());
    println!("  Total gas: {}", result.total_gas());
    println!("  Avg block fullness: {:.1}%", result.avg_block_fullness(config.block_gas_limit));
    println!();
    println!("  Simulation speedup: {:.0}x", result.speedup());
}

fn demo_monte_carlo() {
    println!("Demo 2: Monte Carlo Analysis (10 iterations)");
    println!("{:-<70}", "");
    println!();
    
    let config = SimConfig {
        block_gas_limit: 1_000_000,
        block_time: Duration::from_secs(12),
        sim_duration: Duration::from_secs(60),
        stream_config: StreamConfig {
            tx_rate: 50.0,
            log_normal_fees: true,
            ..Default::default()
        },
        start_block: 1,
    };
    
    println!("Running 10 simulations of {:?} each...", config.sim_duration);
    println!();
    
    let start = std::time::Instant::now();
    let mc_result = run_monte_carlo(config.clone(), 10);
    let total_time = start.elapsed();
    
    println!("Results across {} iterations:", mc_result.iterations());
    println!();
    
    // Show individual run results
    println!("{:>5} {:>8} {:>12} {:>12} {:>10}",
        "Run", "Blocks", "Value", "Incl. Rate", "Speedup");
    println!("{:-<55}", "");
    
    for (i, result) in mc_result.results.iter().enumerate() {
        println!("{:>5} {:>8} {:>12} {:>11.1}% {:>9.0}x",
            i + 1,
            result.blocks.len(),
            result.total_value(),
            result.inclusion_rate() * 100.0,
            result.speedup(),
        );
    }
    println!("{:-<55}", "");
    
    println!();
    println!("Aggregate Statistics:");
    println!("  Avg blocks per run: {:.1}", mc_result.avg_blocks());
    println!("  Avg total value: {:.0}", mc_result.avg_total_value());
    println!("  Value std dev: {:.0}", mc_result.value_std_dev());
    println!("  Avg inclusion rate: {:.1}%", mc_result.avg_inclusion_rate() * 100.0);
    println!("  Avg speedup: {:.0}x", mc_result.avg_speedup());
    println!();
    println!("  Total real time: {:?}", total_time);
    println!("  Time per iteration: {:?}", total_time / 10);
    println!("  Total simulated time: {:?}", config.sim_duration * 10);
}