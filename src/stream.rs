use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use rand::Rng;
use rand_distr::{Distribution, Exp, LogNormal};

use crate::tx::Tx;

/// Configuration for the transaction stream generator
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Average transactions per second
    pub tx_rate: f64,
    
    /// Minimum gas per transaction
    pub min_gas: u64,
    
    /// Maximum gas per transaction
    pub max_gas: u64,
    
    /// Minimum priority fee in gwei
    pub min_fee: u64,
    
    /// Maximum priority fee in gwei
    pub max_fee: u64,
    
    /// Use log-normal distribution for fees (more realistic)
    /// If false, uses uniform distribution
    pub log_normal_fees: bool,
    
    /// Log-normal mu parameter (mean of underlying normal)
    pub fee_mu: f64,
    
    /// Log-normal sigma parameter (std dev of underlying normal)
    pub fee_sigma: f64,
    
    /// Total number of transactions to generate (None = infinite)
    pub tx_count: Option<u64>,
    
    /// Duration to run (None = infinite, or until tx_count reached)
    pub duration: Option<Duration>,
    
    /// Starting transaction ID
    pub start_id: u64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            tx_rate: 50.0,           // 50 tx/s
            min_gas: 21_000,         // Simple transfer
            max_gas: 500_000,        // Complex contract call
            min_fee: 1,
            max_fee: 200,
            log_normal_fees: true,
            fee_mu: 2.5,             // ~12 gwei median
            fee_sigma: 0.8,          // Reasonable spread
            tx_count: None,
            duration: None,
            start_id: 1,
        }
    }
}

impl StreamConfig {
    /// Create config for a quick test run
    pub fn test(tx_count: u64, tx_rate: f64) -> Self {
        Self {
            tx_count: Some(tx_count),
            tx_rate,
            ..Default::default()
        }
    }
    
    /// Create config with specific duration
    pub fn timed(duration: Duration, tx_rate: f64) -> Self {
        Self {
            duration: Some(duration),
            tx_rate,
            ..Default::default()
        }
    }
}

/// Statistics from a completed stream
#[derive(Debug, Clone, Default)]
pub struct StreamStats {
    pub txs_generated: u64,
    pub total_gas: u64,
    pub total_value: u64,
    pub min_fee: u64,
    pub max_fee: u64,
    pub avg_fee: f64,
    pub duration: Duration,
    pub actual_rate: f64,
}

/// Transaction generator
pub struct TxGenerator {
    config: StreamConfig,
    next_id: u64,
    rng: rand::rngs::ThreadRng,
    fee_dist: Option<LogNormal<f64>>,
    stats: StreamStats,
}

impl TxGenerator {
    pub fn new(config: StreamConfig) -> Self {
        let fee_dist = if config.log_normal_fees {
            Some(LogNormal::new(config.fee_mu, config.fee_sigma).unwrap())
        } else {
            None
        };
        
        Self {
            next_id: config.start_id,
            config,
            rng: rand::thread_rng(),
            fee_dist,
            stats: StreamStats {
                min_fee: u64::MAX,
                max_fee: 0,
                ..Default::default()
            },
        }
    }
    
    /// Generate a single transaction
    pub fn generate(&mut self) -> Tx {
        let id = self.next_id;
        self.next_id += 1;
        
        // Random gas (uniform)
        let gas = self.rng.gen_range(self.config.min_gas..=self.config.max_gas);
        
        // Random fee (log-normal or uniform)
        let fee = if let Some(ref dist) = self.fee_dist {
            let raw: f64 = dist.sample(&mut self.rng);
            (raw as u64).clamp(self.config.min_fee, self.config.max_fee)
        } else {
            self.rng.gen_range(self.config.min_fee..=self.config.max_fee)
        };
        
        // Update stats
        self.stats.txs_generated += 1;
        self.stats.total_gas += gas;
        self.stats.total_value += fee * gas;
        self.stats.min_fee = self.stats.min_fee.min(fee);
        self.stats.max_fee = self.stats.max_fee.max(fee);
        
        Tx::new(id, gas, fee)
    }
    
    /// Generate multiple transactions
    pub fn generate_batch(&mut self, count: usize) -> Vec<Tx> {
        (0..count).map(|_| self.generate()).collect()
    }
    
    /// Get current stats
    pub fn stats(&self) -> &StreamStats {
        &self.stats
    }
    
    /// Finalize stats with duration
    pub fn finalize_stats(&mut self, duration: Duration) -> StreamStats {
        self.stats.duration = duration;
        if duration.as_secs_f64() > 0.0 {
            self.stats.actual_rate = self.stats.txs_generated as f64 / duration.as_secs_f64();
        }
        if self.stats.txs_generated > 0 {
            self.stats.avg_fee = self.stats.total_value as f64 
                / self.stats.total_gas as f64;
        }
        self.stats.clone()
    }
}

/// Run stream generator in a separate thread
/// Sends transactions to the provided channel
pub fn spawn_stream(
    config: StreamConfig,
    tx_sender: Sender<Tx>,
) -> JoinHandle<StreamStats> {
    thread::spawn(move || run_stream(config, tx_sender))
}

/// Run stream generator (blocking)
pub fn run_stream(config: StreamConfig, tx_sender: Sender<Tx>) -> StreamStats {
    let mut generator = TxGenerator::new(config.clone());
    let start = Instant::now();
    
    // Exponential distribution for inter-arrival times (Poisson process)
    let lambda = config.tx_rate;
    let exp_dist = Exp::new(lambda).unwrap();
    let mut rng = rand::thread_rng();
    
    let mut txs_sent = 0u64;
    
    loop {
        // Check termination conditions
        if let Some(count) = config.tx_count {
            if txs_sent >= count {
                break;
            }
        }
        
        if let Some(duration) = config.duration {
            if start.elapsed() >= duration {
                break;
            }
        }
        
        // Generate and send transaction
        let tx = generator.generate();
        if tx_sender.send(tx).is_err() {
            // Receiver dropped, stop generating
            break;
        }
        txs_sent += 1;
        
        // Wait for next arrival (Poisson process)
        let wait_secs: f64 = exp_dist.sample(&mut rng);
        let wait = Duration::from_secs_f64(wait_secs);
        
        // Cap maximum wait to avoid hanging
        let wait = wait.min(Duration::from_secs(1));
        
        if !wait.is_zero() {
            thread::sleep(wait);
        }
    }
    
    generator.finalize_stats(start.elapsed())
}

/// Generate a batch of transactions without streaming
/// Useful for testing and benchmarking
pub fn generate_batch(config: StreamConfig, count: usize) -> (Vec<Tx>, StreamStats) {
    let mut generator = TxGenerator::new(config);
    let start = Instant::now();
    let txs = generator.generate_batch(count);
    let stats = generator.finalize_stats(start.elapsed());
    (txs, stats)
}

/// Simulated time stream - generates transactions with simulated timestamps
/// Returns all transactions immediately but with realistic "arrival" patterns
pub fn generate_simulated_stream(
    config: StreamConfig,
    duration: Duration,
) -> Vec<(Duration, Tx)> {
    let mut generator = TxGenerator::new(config.clone());
    let mut rng = rand::thread_rng();
    let exp_dist = Exp::new(config.tx_rate).unwrap();
    
    let mut results = Vec::new();
    let mut current_time = Duration::ZERO;
    
    while current_time < duration {
        let tx = generator.generate();
        results.push((current_time, tx));
        
        let wait_secs: f64 = exp_dist.sample(&mut rng);
        current_time += Duration::from_secs_f64(wait_secs);
    }
    
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    
    #[test]
    fn test_generator_basic() {
        let config = StreamConfig::default();
        let mut generator = TxGenerator::new(config);
        
        let tx = generator.generate();
        assert_eq!(tx.id, 1);
        assert!(tx.gas_limit >= 21_000 && tx.gas_limit <= 500_000);
        assert!(tx.priority_fee_gwei >= 1 && tx.priority_fee_gwei <= 200);
        
        let tx2 = generator.generate();
        assert_eq!(tx2.id, 2);
    }
    
    #[test]
    fn test_generator_batch() {
        let config = StreamConfig::test(100, 50.0);
        let mut generator = TxGenerator::new(config);
        
        let txs = generator.generate_batch(100);
        assert_eq!(txs.len(), 100);
        
        // IDs should be sequential
        for (i, tx) in txs.iter().enumerate() {
            assert_eq!(tx.id, (i + 1) as u64);
        }
    }
    
    #[test]
    fn test_generator_stats() {
        let config = StreamConfig::default();
        let mut generator = TxGenerator::new(config);
        
        generator.generate_batch(10);
        
        let stats = generator.stats();
        assert_eq!(stats.txs_generated, 10);
        assert!(stats.total_gas > 0);
        assert!(stats.min_fee <= stats.max_fee);
    }
    
    #[test]
    fn test_log_normal_fee_distribution() {
        let config = StreamConfig {
            log_normal_fees: true,
            fee_mu: 2.5,
            fee_sigma: 0.8,
            ..Default::default()
        };
        let mut generator = TxGenerator::new(config);
        
        let txs = generator.generate_batch(1000);
        
        // Check distribution shape - most fees should be in middle range
        let low_count = txs.iter().filter(|t| t.priority_fee_gwei < 10).count();
        let mid_count = txs.iter().filter(|t| t.priority_fee_gwei >= 10 && t.priority_fee_gwei < 50).count();
        let high_count = txs.iter().filter(|t| t.priority_fee_gwei >= 50).count();
        
        // Log-normal should have long tail - more low/mid than high
        assert!(mid_count > high_count, "Expected more mid-range fees");
        assert!(low_count + mid_count > high_count, "Expected left-skewed distribution");
    }
    
    #[test]
    fn test_uniform_fee_distribution() {
        let config = StreamConfig {
            log_normal_fees: false,
            min_fee: 10,
            max_fee: 100,
            ..Default::default()
        };
        let mut generator = TxGenerator::new(config);
        
        let txs = generator.generate_batch(1000);
        
        // All fees should be in range
        for tx in &txs {
            assert!(tx.priority_fee_gwei >= 10 && tx.priority_fee_gwei <= 100);
        }
    }
    
    #[test]
    fn test_stream_with_count_limit() {
        let config = StreamConfig::test(10, 1000.0); // Fast rate, 10 txs
        let (sender, receiver) = unbounded();
        
        let stats = run_stream(config, sender);
        
        assert_eq!(stats.txs_generated, 10);
        
        // All txs should be in channel
        let received: Vec<_> = receiver.try_iter().collect();
        assert_eq!(received.len(), 10);
    }
    
    #[test]
    fn test_stream_with_duration_limit() {
        let config = StreamConfig::timed(Duration::from_millis(100), 100.0);
        let (sender, receiver) = unbounded();
        
        let stats = run_stream(config, sender);
        
        // Should have generated ~10 txs (100ms * 100tx/s)
        // Allow some variance due to timing
        assert!(stats.txs_generated >= 5 && stats.txs_generated <= 20,
            "Expected ~10 txs, got {}", stats.txs_generated);
        
        let received: Vec<_> = receiver.try_iter().collect();
        assert_eq!(received.len(), stats.txs_generated as usize);
    }
    
    #[test]
    fn test_spawn_stream() {
        let config = StreamConfig::test(20, 1000.0);
        let (sender, receiver) = unbounded();
        
        let handle = spawn_stream(config, sender);
        let stats = handle.join().unwrap();
        
        assert_eq!(stats.txs_generated, 20);
        
        let received: Vec<_> = receiver.try_iter().collect();
        assert_eq!(received.len(), 20);
    }
    
    #[test]
    fn test_generate_batch_fn() {
        let config = StreamConfig::default();
        let (txs, stats) = generate_batch(config, 50);
        
        assert_eq!(txs.len(), 50);
        assert_eq!(stats.txs_generated, 50);
    }
    
    #[test]
    fn test_simulated_stream() {
        let config = StreamConfig {
            tx_rate: 10.0, // 10 tx/s
            ..Default::default()
        };
        
        let stream = generate_simulated_stream(config, Duration::from_secs(1));
        
        // Should have ~10 txs
        assert!(stream.len() >= 5 && stream.len() <= 20,
            "Expected ~10 txs, got {}", stream.len());
        
        // Timestamps should be increasing
        for i in 1..stream.len() {
            assert!(stream[i].0 >= stream[i-1].0);
        }
        
        // All timestamps should be within duration
        for (time, _) in &stream {
            assert!(*time <= Duration::from_secs(1));
        }
    }
    
    #[test]
    fn test_receiver_drop_stops_stream() {
        let config = StreamConfig {
            tx_rate: 1000.0,
            tx_count: Some(1_000_000), // Would take forever
            ..Default::default()
        };
        let (sender, receiver) = unbounded();
        
        let handle = spawn_stream(config, sender);
        
        // Drop receiver after small delay
        thread::sleep(Duration::from_millis(10));
        drop(receiver);
        
        // Stream should stop
        let stats = handle.join().unwrap();
        assert!(stats.txs_generated < 1_000_000);
    }
}