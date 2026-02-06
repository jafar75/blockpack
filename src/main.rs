use std::time::Duration;

use blockpack::{
    SimConfig, StreamConfig, BuilderMetrics, SimulationOutput,
    run_simulation, run_monte_carlo,
};

fn main() {
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!();
    
    // Demo 1: Simulation with detailed metrics
    demo_with_metrics();
    
    println!();
    println!("{:=<70}", "");
    println!();
    
    // Demo 2: Monte Carlo with JSON output
    demo_json_output();
}

fn demo_with_metrics() {
    println!("Demo 1: Simulation with Detailed Metrics");
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
    let result = run_simulation(config.clone());
    println!("Done in {:?}", result.real_duration);
    println!();
    
    // Build metrics from results
    let mut metrics = BuilderMetrics::new();
    
    for stats in &result.block_stats {
        // Simulate tx recording (approximate from block data)
        let avg_fee = if stats.tx_count > 0 {
            stats.total_value / stats.gas_used
        } else {
            0
        };
        
        for _ in 0..stats.tx_count {
            metrics.record_tx(
                stats.gas_used / stats.tx_count as u64,
                avg_fee,
                true,
                false,
            );
        }
        
        metrics.record_block(
            stats.tx_count,
            stats.gas_used,
            stats.total_value,
            config.block_gas_limit,
        );
    }
    
    // Display metrics report
    let report = metrics.report();
    println!("{}", report.display());
    
    // Display fee histogram
    println!("Fee Distribution:");
    println!("{}", metrics.fee_histogram.display(30));
}

fn demo_json_output() {
    println!("Demo 2: JSON Output");
    println!("{:-<70}", "");
    println!();
    
    let config = SimConfig {
        block_gas_limit: 500_000,
        block_time: Duration::from_secs(6),
        sim_duration: Duration::from_secs(30),
        stream_config: StreamConfig {
            tx_rate: 30.0,
            log_normal_fees: true,
            ..Default::default()
        },
        start_block: 1,
    };
    
    println!("Running simulation...");
    let result = run_simulation(config.clone());
    
    // Create JSON output
    let output = SimulationOutput::from_result(&result, config.block_gas_limit);
    
    println!();
    println!("Simulation Summary (JSON):");
    println!("{}", serde_json::to_string_pretty(&output.summary).unwrap());
    
    println!();
    println!("First block detail:");
    if let Some(block) = output.blocks.first() {
        println!("{}", serde_json::to_string_pretty(block).unwrap());
    }
    
    println!();
    println!("{:-<70}", "");
    println!();
    
    // Monte Carlo summary
    println!("Monte Carlo Analysis (5 runs):");
    let mc_result = run_monte_carlo(config.clone(), 5);
    
    println!();
    println!("{{");
    println!("  \"iterations\": {},", mc_result.iterations());
    println!("  \"avg_blocks\": {:.1},", mc_result.avg_blocks());
    println!("  \"avg_total_value\": {:.0},", mc_result.avg_total_value());
    println!("  \"value_std_dev\": {:.0},", mc_result.value_std_dev());
    println!("  \"avg_inclusion_rate_pct\": {:.1},", mc_result.avg_inclusion_rate() * 100.0);
    println!("  \"avg_speedup\": {:.0}", mc_result.avg_speedup());
    println!("}}");
}