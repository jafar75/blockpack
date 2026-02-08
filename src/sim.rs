//! Discrete event simulation for fast block building tests
//! 
//! This module provides a way to run the entire block building pipeline
//! in simulated time, allowing for fast testing and benchmarking without
//! real-world timing delays.

use std::time::Duration;

use crate::block::Block;
use crate::builder::{Builder, UpdateResult, IncrementalStats};
use crate::stream::{StreamConfig, TxGenerator};
use crate::time::{EventQueue, TimedEvent, SimulatedClock, Clock};
use crate::tx::Tx;

use rand::Rng;
use rand_distr::{Distribution, Exp};

/// Events in the simulation
#[derive(Debug, Clone)]
pub enum SimEvent {
    /// A transaction arrives
    TxArrival(Tx),
    
    /// Time to seal a block
    BlockSeal,
    
    /// End the simulation
    End,
}

/// Result of a simulation run
#[derive(Debug)]
pub struct SimulationResult {
    /// All blocks produced
    pub blocks: Vec<Block>,
    
    /// Transactions that were generated but not included
    pub pending_txs: Vec<Tx>,
    
    /// Total transactions generated
    pub txs_generated: u64,
    
    /// Total transactions included in blocks
    pub txs_included: u64,
    
    /// Simulated duration
    pub sim_duration: Duration,
    
    /// Real wall-clock duration
    pub real_duration: Duration,
    
    /// Per-block stats
    pub block_stats: Vec<BlockSimStats>,
}

impl SimulationResult {
    pub fn total_value(&self) -> u64 {
        self.blocks.iter().map(|b| b.total_value).sum()
    }
    
    pub fn total_gas(&self) -> u64 {
        self.blocks.iter().map(|b| b.gas_used).sum()
    }
    
    pub fn avg_block_fullness(&self, gas_limit: u64) -> f64 {
        if self.blocks.is_empty() {
            return 0.0;
        }
        let avg_gas = self.total_gas() as f64 / self.blocks.len() as f64;
        (avg_gas / gas_limit as f64) * 100.0
    }
    
    pub fn inclusion_rate(&self) -> f64 {
        if self.txs_generated == 0 {
            return 0.0;
        }
        self.txs_included as f64 / self.txs_generated as f64
    }
    
    pub fn speedup(&self) -> f64 {
        if self.real_duration.as_secs_f64() == 0.0 {
            return 0.0;
        }
        self.sim_duration.as_secs_f64() / self.real_duration.as_secs_f64()
    }
}

/// Stats for a single block in simulation
#[derive(Debug, Clone)]
pub struct BlockSimStats {
    pub block_number: u64,
    pub seal_time: Duration,
    pub tx_count: usize,
    pub gas_used: u64,
    pub total_value: u64,
    pub incremental_stats: IncrementalStats,
}

/// Configuration for the simulation
#[derive(Debug, Clone)]
pub struct SimConfig {
    /// Block gas limit
    pub block_gas_limit: u64,
    
    /// Time between blocks
    pub block_time: Duration,
    
    /// Total simulation duration
    pub sim_duration: Duration,
    
    /// Transaction stream configuration
    pub stream_config: StreamConfig,
    
    /// Starting block number
    pub start_block: u64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            block_gas_limit: 30_000_000,
            block_time: Duration::from_secs(12),
            sim_duration: Duration::from_secs(120), // 2 minutes = ~10 blocks
            stream_config: StreamConfig::default(),
            start_block: 1,
        }
    }
}

impl SimConfig {
    /// Quick test config
    pub fn test() -> Self {
        Self {
            block_gas_limit: 1_000_000,
            block_time: Duration::from_millis(500),
            sim_duration: Duration::from_secs(5),
            stream_config: StreamConfig {
                tx_rate: 100.0,
                ..Default::default()
            },
            start_block: 1,
        }
    }
}

/// Run a discrete event simulation of block building
pub fn run_simulation(config: SimConfig) -> SimulationResult {
    let real_start = std::time::Instant::now();
    let mut clock = SimulatedClock::new();
    
    // Initialize components
    let mut builder = Builder::with_start_block(config.block_gas_limit, config.start_block);
    let mut generator = TxGenerator::new(config.stream_config.clone());
    let mut rng = rand::thread_rng();
    
    // Results collection
    let mut blocks = Vec::new();
    let mut block_stats = Vec::new();
    let mut pending_txs = Vec::new();
    let mut txs_generated = 0u64;
    let mut txs_included = 0u64;
    
    // Build event queue
    let mut events: EventQueue<SimEvent> = EventQueue::new();
    
    // Schedule block seals
    let mut seal_time = config.block_time;
    while seal_time <= config.sim_duration {
        events.schedule(seal_time, SimEvent::BlockSeal);
        seal_time += config.block_time;
    }
    
    // Schedule end
    events.schedule(config.sim_duration, SimEvent::End);
    
    // Schedule transaction arrivals (Poisson process)
    let exp_dist = Exp::new(config.stream_config.tx_rate).unwrap();
    let mut next_arrival = Duration::ZERO;
    
    loop {
        let wait: f64 = exp_dist.sample(&mut rng);
        next_arrival += Duration::from_secs_f64(wait);
        
        if next_arrival > config.sim_duration {
            break;
        }
        
        let tx = generator.generate();
        txs_generated += 1;
        events.schedule(next_arrival, SimEvent::TxArrival(tx));
    }
    
    // Track incremental stats per block
    let mut current_block_stats = IncrementalStats::default();
    
    // Run simulation
    while let Some(event) = events.pop() {
        clock.advance(event.time.saturating_sub(clock.now()));
        
        match event.event {
            SimEvent::TxArrival(tx) => {
                let result = builder.process_tx(tx.clone());
                current_block_stats.record(&result);
                
                match result {
                    UpdateResult::Added 
                    | UpdateResult::Replaced(_) 
                    | UpdateResult::ReplacedMultiple(_) => {}
                    UpdateResult::Rejected 
                    | UpdateResult::Duplicate 
                    | UpdateResult::InsufficientFee => {
                        pending_txs.push(tx);
                    }
                }
            }
            
            SimEvent::BlockSeal => {
                if let Some(block) = builder.seal() {
                    let stats = BlockSimStats {
                        block_number: block.number,
                        seal_time: clock.now(),
                        tx_count: block.tx_count(),
                        gas_used: block.gas_used,
                        total_value: block.total_value,
                        incremental_stats: current_block_stats.clone(),
                    };
                    
                    txs_included += block.tx_count() as u64;
                    blocks.push(block);
                    block_stats.push(stats);
                    current_block_stats = IncrementalStats::default();
                }
            }
            
            SimEvent::End => {
                // Seal any remaining block
                if let Some(block) = builder.seal() {
                    let stats = BlockSimStats {
                        block_number: block.number,
                        seal_time: clock.now(),
                        tx_count: block.tx_count(),
                        gas_used: block.gas_used,
                        total_value: block.total_value,
                        incremental_stats: current_block_stats.clone(),
                    };
                    
                    txs_included += block.tx_count() as u64;
                    blocks.push(block);
                    block_stats.push(stats);
                }
                break;
            }
        }
    }
    
    // Collect pending from builder's candidate if any
    if let Some(candidate) = builder.candidate() {
        for tx in &candidate.transactions {
            pending_txs.push(tx.clone());
        }
    }
    
    SimulationResult {
        blocks,
        pending_txs,
        txs_generated,
        txs_included,
        sim_duration: config.sim_duration,
        real_duration: real_start.elapsed(),
        block_stats,
    }
}

/// Run multiple simulations and aggregate results
pub fn run_monte_carlo(config: SimConfig, iterations: usize) -> MonteCarloResult {
    let mut results = Vec::with_capacity(iterations);
    
    for _ in 0..iterations {
        results.push(run_simulation(config.clone()));
    }
    
    MonteCarloResult { results }
}

/// Results from multiple simulation runs
#[derive(Debug)]
pub struct MonteCarloResult {
    pub results: Vec<SimulationResult>,
}

impl MonteCarloResult {
    pub fn iterations(&self) -> usize {
        self.results.len()
    }
    
    pub fn avg_blocks(&self) -> f64 {
        let total: usize = self.results.iter().map(|r| r.blocks.len()).sum();
        total as f64 / self.results.len() as f64
    }
    
    pub fn avg_inclusion_rate(&self) -> f64 {
        let sum: f64 = self.results.iter().map(|r| r.inclusion_rate()).sum();
        sum / self.results.len() as f64
    }
    
    pub fn avg_total_value(&self) -> f64 {
        let sum: u64 = self.results.iter().map(|r| r.total_value()).sum();
        sum as f64 / self.results.len() as f64
    }
    
    pub fn avg_speedup(&self) -> f64 {
        let sum: f64 = self.results.iter().map(|r| r.speedup()).sum();
        sum / self.results.len() as f64
    }
    
    pub fn value_std_dev(&self) -> f64 {
        let mean = self.avg_total_value();
        let variance: f64 = self.results.iter()
            .map(|r| {
                let diff = r.total_value() as f64 - mean;
                diff * diff
            })
            .sum::<f64>() / self.results.len() as f64;
        variance.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_simulation() {
        let config = SimConfig::test();
        let result = run_simulation(config);
        
        assert!(!result.blocks.is_empty(), "Should produce at least one block");
        assert!(result.txs_generated > 0, "Should generate transactions");
        assert!(result.txs_included > 0, "Should include some transactions");
    }
    
    #[test]
    fn test_simulation_speedup() {
        let config = SimConfig {
            sim_duration: Duration::from_secs(60), // 1 minute simulated
            block_time: Duration::from_secs(12),
            stream_config: StreamConfig {
                tx_rate: 50.0,
                ..Default::default()
            },
            ..Default::default()
        };
        
        let result = run_simulation(config);
        
        // Should complete much faster than 60 seconds
        assert!(result.real_duration < Duration::from_secs(5),
            "Simulation should be fast, took {:?}", result.real_duration);
        
        assert!(result.speedup() > 10.0,
            "Expected >10x speedup, got {:.1}x", result.speedup());
    }
    
    #[test]
    fn test_simulation_block_timing() {
        let config = SimConfig {
            block_time: Duration::from_secs(1),
            sim_duration: Duration::from_secs(5),
            stream_config: StreamConfig {
                tx_rate: 100.0,
                ..Default::default()
            },
            ..Default::default()
        };
        
        let result = run_simulation(config);
        
        // Should have ~5 blocks (one per second for 5 seconds)
        assert!(result.blocks.len() >= 4 && result.blocks.len() <= 6,
            "Expected ~5 blocks, got {}", result.blocks.len());
        
        // Check seal times are roughly at block intervals
        for (i, stats) in result.block_stats.iter().enumerate() {
            let expected = Duration::from_secs((i + 1) as u64);
            assert!(stats.seal_time >= expected.saturating_sub(Duration::from_millis(100)),
                "Block {} sealed too early at {:?}", i, stats.seal_time);
        }
    }
    
    #[test]
    fn test_monte_carlo() {
        let config = SimConfig::test();
        let mc_result = run_monte_carlo(config, 5);
        
        assert_eq!(mc_result.iterations(), 5);
        assert!(mc_result.avg_blocks() > 0.0);
        assert!(mc_result.avg_inclusion_rate() > 0.0);
        assert!(mc_result.avg_total_value() > 0.0);
    }
    
    #[test]
    fn test_simulation_result_metrics() {
        let config = SimConfig::test();
        let result = run_simulation(config);
        
        // Test metric calculations
        assert!(result.total_value() > 0);
        assert!(result.total_gas() > 0);
        assert!(result.avg_block_fullness(1_000_000) >= 0.0);
        assert!(result.inclusion_rate() >= 0.0 && result.inclusion_rate() <= 1.0);
    }
}