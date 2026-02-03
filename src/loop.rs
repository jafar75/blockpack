use std::time::{Duration, Instant};
use crossbeam_channel::{Receiver, Select, TryRecvError};

use crate::block::Block;
use crate::builder::{Builder, IncrementalStats, UpdateResult};
use crate::tx::Tx;

/// Commands that can be sent to the builder loop
#[derive(Debug, Clone)]
pub enum BuilderCommand {
    /// Seal current block immediately
    SealNow,
    
    /// Shutdown the builder loop
    Shutdown,
}

/// Events emitted by the builder loop
#[derive(Debug)]
pub enum BuilderEvent {
    /// A new transaction was processed
    TxProcessed {
        tx_id: u64,
        result: UpdateResult,
        block_value: u64,
        block_gas: u64,
    },
    
    /// A block was sealed
    BlockSealed {
        block: Block,
        stats: IncrementalStats,
        build_duration: Duration,
    },
    
    /// Builder loop has shut down
    Shutdown,
}

/// Configuration for the builder loop
#[derive(Debug, Clone)]
pub struct BuilderLoopConfig {
    /// Gas limit for blocks
    pub block_gas_limit: u64,
    
    /// How often to seal blocks (simulated block time)
    pub block_time: Duration,
    
    /// Starting block number
    pub start_block: u64,
}

impl Default for BuilderLoopConfig {
    fn default() -> Self {
        Self {
            block_gas_limit: 30_000_000,
            block_time: Duration::from_secs(12),
            start_block: 1,
        }
    }
}

/// Result of running the builder loop
#[derive(Debug, Default)]
pub struct LoopStats {
    pub blocks_built: u64,
    pub total_txs_received: u64,
    pub total_txs_included: u64,
    pub total_value_extracted: u64,
    pub total_gas_used: u64,
    pub run_duration: Duration,
}

impl LoopStats {
    pub fn avg_block_value(&self) -> f64 {
        if self.blocks_built == 0 {
            0.0
        } else {
            self.total_value_extracted as f64 / self.blocks_built as f64
        }
    }
    
    pub fn avg_block_fullness(&self, gas_limit: u64) -> f64 {
        if self.blocks_built == 0 {
            0.0
        } else {
            let avg_gas = self.total_gas_used as f64 / self.blocks_built as f64;
            (avg_gas / gas_limit as f64) * 100.0
        }
    }
    
    pub fn inclusion_rate(&self) -> f64 {
        if self.total_txs_received == 0 {
            0.0
        } else {
            self.total_txs_included as f64 / self.total_txs_received as f64
        }
    }
}

/// Run the builder loop
/// 
/// Receives transactions from `tx_rx`, processes them incrementally,
/// and seals blocks either on timer or on command.
/// 
/// Emits events to `event_tx` for monitoring.
pub fn run_builder_loop(
    config: BuilderLoopConfig,
    tx_rx: Receiver<Tx>,
    cmd_rx: Receiver<BuilderCommand>,
    event_tx: Option<crossbeam_channel::Sender<BuilderEvent>>,
) -> LoopStats {
    let mut builder = Builder::with_start_block(config.block_gas_limit, config.start_block);
    let mut stats = LoopStats::default();
    let loop_start = Instant::now();
    let mut block_start = Instant::now();
    
    // Helper to emit events
    let emit = |event: BuilderEvent, tx: &Option<crossbeam_channel::Sender<BuilderEvent>>| {
        if let Some(sender) = tx {
            let _ = sender.try_send(event);
        }
    };
    
    loop {
        // Check if it's time to seal
        let time_until_seal = config.block_time
            .checked_sub(block_start.elapsed())
            .unwrap_or(Duration::ZERO);
        
        if time_until_seal.is_zero() {
            // Time to seal
            if let Some(block) = builder.seal() {
                let build_duration = block_start.elapsed();
                let block_stats = builder.stats().clone();
                
                stats.blocks_built += 1;
                stats.total_txs_included += block.tx_count() as u64;
                stats.total_value_extracted += block.total_value;
                stats.total_gas_used += block.gas_used;
                
                emit(BuilderEvent::BlockSealed {
                    block,
                    stats: block_stats,
                    build_duration,
                }, &event_tx);
            }
            block_start = Instant::now();
        }
        
        // Use select to wait on multiple channels with timeout
        let mut sel = Select::new();
        let tx_idx = sel.recv(&tx_rx);
        let cmd_idx = sel.recv(&cmd_rx);
        
        // Wait with timeout until next seal time
        let result = sel.select_timeout(time_until_seal);
        
        match result {
            Ok(oper) => {
                if oper.index() == tx_idx {
                    // Transaction received
                    match oper.recv(&tx_rx) {
                        Ok(tx) => {
                            let tx_id = tx.id;
                            let result = builder.process_tx(tx);
                            stats.total_txs_received += 1;
                            
                            if let Some(candidate) = builder.candidate() {
                                emit(BuilderEvent::TxProcessed {
                                    tx_id,
                                    result,
                                    block_value: candidate.total_value,
                                    block_gas: candidate.gas_used,
                                }, &event_tx);
                            }
                        }
                        Err(_) => {
                            // Channel closed, seal final block and exit
                            if let Some(block) = builder.seal() {
                                let build_duration = block_start.elapsed();
                                stats.blocks_built += 1;
                                stats.total_txs_included += block.tx_count() as u64;
                                stats.total_value_extracted += block.total_value;
                                stats.total_gas_used += block.gas_used;
                                
                                emit(BuilderEvent::BlockSealed {
                                    block,
                                    stats: IncrementalStats::default(),
                                    build_duration,
                                }, &event_tx);
                            }
                            break;
                        }
                    }
                } else if oper.index() == cmd_idx {
                    // Command received
                    match oper.recv(&cmd_rx) {
                        Ok(BuilderCommand::SealNow) => {
                            if let Some(block) = builder.seal() {
                                let build_duration = block_start.elapsed();
                                stats.blocks_built += 1;
                                stats.total_txs_included += block.tx_count() as u64;
                                stats.total_value_extracted += block.total_value;
                                stats.total_gas_used += block.gas_used;
                                
                                emit(BuilderEvent::BlockSealed {
                                    block,
                                    stats: IncrementalStats::default(),
                                    build_duration,
                                }, &event_tx);
                            }
                            block_start = Instant::now();
                        }
                        Ok(BuilderCommand::Shutdown) => {
                            // Seal any pending block before shutdown
                            if let Some(block) = builder.seal() {
                                let build_duration = block_start.elapsed();
                                stats.blocks_built += 1;
                                stats.total_txs_included += block.tx_count() as u64;
                                stats.total_value_extracted += block.total_value;
                                stats.total_gas_used += block.gas_used;
                                
                                emit(BuilderEvent::BlockSealed {
                                    block,
                                    stats: IncrementalStats::default(),
                                    build_duration,
                                }, &event_tx);
                            }
                            emit(BuilderEvent::Shutdown, &event_tx);
                            break;
                        }
                        Err(_) => {
                            // Command channel closed, continue with just tx channel
                        }
                    }
                }
            }
            Err(_) => {
                // Timeout - will seal on next iteration
            }
        }
    }
    
    stats.run_duration = loop_start.elapsed();
    emit(BuilderEvent::Shutdown, &event_tx);
    stats
}

/// Simpler synchronous builder that processes a batch of transactions
/// Useful for testing and benchmarking
pub fn build_from_batch(
    txs: Vec<Tx>,
    block_gas_limit: u64,
    block_time: Duration,
) -> Vec<Block> {
    let mut builder = Builder::new(block_gas_limit);
    let mut blocks = Vec::new();
    let mut last_seal = Instant::now();
    
    for tx in txs {
        // Check if we should seal based on simulated time
        if last_seal.elapsed() >= block_time {
            if let Some(block) = builder.seal() {
                blocks.push(block);
            }
            last_seal = Instant::now();
        }
        
        builder.process_tx(tx);
    }
    
    // Seal final block
    if let Some(block) = builder.seal() {
        blocks.push(block);
    }
    
    blocks
}

/// Non-blocking poll-based builder for integration with other event loops
pub struct PollingBuilder {
    builder: Builder,
    config: BuilderLoopConfig,
    block_start: Instant,
    stats: LoopStats,
}

impl PollingBuilder {
    pub fn new(config: BuilderLoopConfig) -> Self {
        Self {
            builder: Builder::with_start_block(config.block_gas_limit, config.start_block),
            config,
            block_start: Instant::now(),
            stats: LoopStats::default(),
        }
    }
    
    /// Process a transaction, returns the result
    pub fn process_tx(&mut self, tx: Tx) -> UpdateResult {
        self.stats.total_txs_received += 1;
        self.builder.process_tx(tx)
    }
    
    /// Check if it's time to seal, and seal if so
    /// Returns the sealed block if one was produced
    pub fn maybe_seal(&mut self) -> Option<Block> {
        if self.block_start.elapsed() >= self.config.block_time {
            self.force_seal()
        } else {
            None
        }
    }
    
    /// Force seal the current block
    pub fn force_seal(&mut self) -> Option<Block> {
        if let Some(block) = self.builder.seal() {
            self.stats.blocks_built += 1;
            self.stats.total_txs_included += block.tx_count() as u64;
            self.stats.total_value_extracted += block.total_value;
            self.stats.total_gas_used += block.gas_used;
            self.block_start = Instant::now();
            Some(block)
        } else {
            None
        }
    }
    
    /// Time remaining until next scheduled seal
    pub fn time_until_seal(&self) -> Duration {
        self.config.block_time
            .checked_sub(self.block_start.elapsed())
            .unwrap_or(Duration::ZERO)
    }
    
    /// Get current candidate block
    pub fn candidate(&self) -> Option<&Block> {
        self.builder.candidate()
    }
    
    /// Get accumulated stats
    pub fn stats(&self) -> &LoopStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use crossbeam_channel::unbounded;
    
    fn make_tx(id: u64, gas: u64, fee: u64) -> Tx {
        Tx::new(id, gas, fee)
    }
    
    #[test]
    fn test_polling_builder() {
        let config = BuilderLoopConfig {
            block_gas_limit: 100_000,
            block_time: Duration::from_millis(100),
            start_block: 1,
        };
        
        let mut builder = PollingBuilder::new(config);
        
        builder.process_tx(make_tx(1, 21_000, 50));
        builder.process_tx(make_tx(2, 21_000, 60));
        
        assert!(builder.candidate().is_some());
        assert_eq!(builder.candidate().unwrap().tx_count(), 2);
        
        let block = builder.force_seal().unwrap();
        assert_eq!(block.tx_count(), 2);
        assert_eq!(builder.stats().blocks_built, 1);
    }
    
    #[test]
    fn test_build_from_batch() {
        let txs = vec![
            make_tx(1, 21_000, 50),
            make_tx(2, 21_000, 60),
            make_tx(3, 21_000, 70),
        ];
        
        let blocks = build_from_batch(txs, 100_000, Duration::from_secs(1000));
        
        // With long block time, all should be in one block
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].tx_count(), 3);
    }
    
    #[test]
    fn test_builder_loop_shutdown() {
        let config = BuilderLoopConfig {
            block_gas_limit: 100_000,
            block_time: Duration::from_secs(10), // Long time, won't auto-seal
            start_block: 1,
        };
        
        let (tx_tx, tx_rx) = unbounded();
        let (cmd_tx, cmd_rx) = unbounded();
        let (event_tx, event_rx) = unbounded();
        
        // Spawn builder loop in thread
        let handle = thread::spawn(move || {
            run_builder_loop(config, tx_rx, cmd_rx, Some(event_tx))
        });
        
        // Send some transactions
        tx_tx.send(make_tx(1, 21_000, 50)).unwrap();
        tx_tx.send(make_tx(2, 21_000, 60)).unwrap();
        
        // Give it a moment to process
        thread::sleep(Duration::from_millis(10));
        
        // Shutdown
        cmd_tx.send(BuilderCommand::Shutdown).unwrap();
        
        let stats = handle.join().unwrap();
        
        assert_eq!(stats.total_txs_received, 2);
        assert_eq!(stats.blocks_built, 1); // Should seal on shutdown
        
        // Check we got events
        let mut got_sealed = false;
        let mut got_shutdown = false;
        while let Ok(event) = event_rx.try_recv() {
            match event {
                BuilderEvent::BlockSealed { block, .. } => {
                    assert_eq!(block.tx_count(), 2);
                    got_sealed = true;
                }
                BuilderEvent::Shutdown => got_shutdown = true,
                _ => {}
            }
        }
        assert!(got_sealed);
        assert!(got_shutdown);
    }
    
    #[test]
    fn test_builder_loop_channel_close() {
        let config = BuilderLoopConfig {
            block_gas_limit: 100_000,
            block_time: Duration::from_secs(10),
            start_block: 1,
        };
        
        let (tx_tx, tx_rx) = unbounded();
        let (_cmd_tx, cmd_rx) = unbounded();
        
        let handle = thread::spawn(move || {
            run_builder_loop(config, tx_rx, cmd_rx, None)
        });
        
        tx_tx.send(make_tx(1, 21_000, 50)).unwrap();
        tx_tx.send(make_tx(2, 21_000, 60)).unwrap();
        
        // Close channel
        drop(tx_tx);
        
        let stats = handle.join().unwrap();
        assert_eq!(stats.total_txs_received, 2);
        assert_eq!(stats.blocks_built, 1);
    }
    
    #[test]
    fn test_loop_stats() {
        let mut stats = LoopStats::default();
        
        stats.blocks_built = 10;
        stats.total_value_extracted = 1_000_000;
        stats.total_gas_used = 200_000_000;
        stats.total_txs_received = 100;
        stats.total_txs_included = 80;
        
        assert_eq!(stats.avg_block_value(), 100_000.0);
        assert!((stats.avg_block_fullness(30_000_000) - 66.67).abs() < 0.1);
        assert_eq!(stats.inclusion_rate(), 0.8);
    }
}