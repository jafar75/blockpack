//! Metrics collection and reporting for block building
//!
//! Provides real-time statistics tracking, histograms, and reporting.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Rolling window statistics tracker
#[derive(Debug)]
pub struct RollingStats {
    values: VecDeque<f64>,
    max_size: usize,
    sum: f64,
}

impl RollingStats {
    pub fn new(window_size: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(window_size),
            max_size: window_size,
            sum: 0.0,
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.values.len() >= self.max_size {
            if let Some(old) = self.values.pop_front() {
                self.sum -= old;
            }
        }
        self.values.push_back(value);
        self.sum += value;
    }

    pub fn mean(&self) -> f64 {
        if self.values.is_empty() {
            0.0
        } else {
            self.sum / self.values.len() as f64
        }
    }

    pub fn min(&self) -> f64 {
        self.values.iter().cloned().fold(f64::INFINITY, f64::min)
    }

    pub fn max(&self) -> f64 {
        self.values.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    }

    pub fn std_dev(&self) -> f64 {
        if self.values.len() < 2 {
            return 0.0;
        }
        let mean = self.mean();
        let variance: f64 = self.values.iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>() / (self.values.len() - 1) as f64;
        variance.sqrt()
    }

    pub fn count(&self) -> usize {
        self.values.len()
    }

    pub fn percentile(&self, p: f64) -> f64 {
        if self.values.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f64> = self.values.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }
}

/// Simple histogram for distribution analysis
#[derive(Debug)]
pub struct Histogram {
    buckets: Vec<u64>,
    min: f64,
    max: f64,
    bucket_width: f64,
    count: u64,
    sum: f64,
}

impl Histogram {
    pub fn new(min: f64, max: f64, num_buckets: usize) -> Self {
        Self {
            buckets: vec![0; num_buckets],
            min,
            max,
            bucket_width: (max - min) / num_buckets as f64,
            count: 0,
            sum: 0.0,
        }
    }

    pub fn record(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        
        let buckets_len = self.buckets.len();
        
        let idx = if value <= self.min {
            0
        } else if value >= self.max {
            buckets_len - 1
        } else {
            ((value - self.min) / self.bucket_width) as usize
        };
        
        self.buckets[idx.min(buckets_len - 1)] += 1;
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    /// Get bucket boundaries and counts for display
    pub fn buckets(&self) -> Vec<(f64, f64, u64)> {
        self.buckets.iter().enumerate().map(|(i, &count)| {
            let low = self.min + i as f64 * self.bucket_width;
            let high = low + self.bucket_width;
            (low, high, count)
        }).collect()
    }

    /// ASCII histogram display
    pub fn display(&self, width: usize) -> String {
        let max_count = *self.buckets.iter().max().unwrap_or(&1);
        let mut output = String::new();
        
        for (low, high, count) in self.buckets() {
            let bar_len = if max_count > 0 {
                (count as f64 / max_count as f64 * width as f64) as usize
            } else {
                0
            };
            let bar: String = "█".repeat(bar_len);
            output.push_str(&format!("{:8.1}-{:8.1} |{:<width$}| {}\n", 
                low, high, bar, count, width = width));
        }
        output
    }
}

/// Rate tracker (events per second)
#[derive(Debug)]
pub struct RateTracker {
    window: Duration,
    events: VecDeque<Instant>,
}

impl RateTracker {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            events: VecDeque::new(),
        }
    }

    pub fn record(&mut self) {
        self.record_at(Instant::now());
    }

    pub fn record_at(&mut self, time: Instant) {
        self.events.push_back(time);
        self.prune(time);
    }

    fn prune(&mut self, now: Instant) {
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        while let Some(&front) = self.events.front() {
            if front < cutoff {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn rate(&self) -> f64 {
        self.rate_at(Instant::now())
    }

    pub fn rate_at(&self, now: Instant) -> f64 {
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        let count = self.events.iter().filter(|&&t| t >= cutoff).count();
        count as f64 / self.window.as_secs_f64()
    }

    pub fn count(&self) -> usize {
        self.events.len()
    }
}

/// Comprehensive metrics collector for block building
#[derive(Debug)]
pub struct BuilderMetrics {
    // Transaction metrics
    pub tx_received: u64,
    pub tx_added: u64,
    pub tx_replaced: u64,
    pub tx_rejected: u64,
    
    // Block metrics
    pub blocks_built: u64,
    pub total_gas_used: u64,
    pub total_value_extracted: u64,
    
    // Rolling stats
    pub block_fullness: RollingStats,
    pub block_value: RollingStats,
    pub block_tx_count: RollingStats,
    pub tx_gas: RollingStats,
    pub tx_fee: RollingStats,
    
    // Histograms
    pub fee_histogram: Histogram,
    pub gas_histogram: Histogram,
    
    // Rate trackers
    pub tx_rate: RateTracker,
    pub block_rate: RateTracker,
    
    // Timing
    pub start_time: Instant,
    pub last_block_time: Option<Instant>,
}

impl BuilderMetrics {
    pub fn new() -> Self {
        Self {
            tx_received: 0,
            tx_added: 0,
            tx_replaced: 0,
            tx_rejected: 0,
            
            blocks_built: 0,
            total_gas_used: 0,
            total_value_extracted: 0,
            
            block_fullness: RollingStats::new(100),
            block_value: RollingStats::new(100),
            block_tx_count: RollingStats::new(100),
            tx_gas: RollingStats::new(1000),
            tx_fee: RollingStats::new(1000),
            
            fee_histogram: Histogram::new(0.0, 200.0, 20),
            gas_histogram: Histogram::new(21_000.0, 500_000.0, 20),
            
            tx_rate: RateTracker::new(Duration::from_secs(60)),
            block_rate: RateTracker::new(Duration::from_secs(300)),
            
            start_time: Instant::now(),
            last_block_time: None,
        }
    }

    pub fn record_tx(&mut self, gas: u64, fee: u64, added: bool, replaced: bool) {
        self.tx_received += 1;
        self.tx_rate.record();
        
        self.tx_gas.push(gas as f64);
        self.tx_fee.push(fee as f64);
        self.gas_histogram.record(gas as f64);
        self.fee_histogram.record(fee as f64);
        
        if added {
            self.tx_added += 1;
        } else if replaced {
            self.tx_replaced += 1;
        } else {
            self.tx_rejected += 1;
        }
    }

    pub fn record_block(&mut self, tx_count: usize, gas_used: u64, value: u64, gas_limit: u64) {
        self.blocks_built += 1;
        self.block_rate.record();
        
        self.total_gas_used += gas_used;
        self.total_value_extracted += value;
        
        let fullness = (gas_used as f64 / gas_limit as f64) * 100.0;
        self.block_fullness.push(fullness);
        self.block_value.push(value as f64);
        self.block_tx_count.push(tx_count as f64);
        
        self.last_block_time = Some(Instant::now());
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn acceptance_rate(&self) -> f64 {
        if self.tx_received == 0 {
            0.0
        } else {
            (self.tx_added + self.tx_replaced) as f64 / self.tx_received as f64
        }
    }

    pub fn avg_gas_per_block(&self) -> f64 {
        if self.blocks_built == 0 {
            0.0
        } else {
            self.total_gas_used as f64 / self.blocks_built as f64
        }
    }

    pub fn avg_value_per_block(&self) -> f64 {
        if self.blocks_built == 0 {
            0.0
        } else {
            self.total_value_extracted as f64 / self.blocks_built as f64
        }
    }

    /// Generate a summary report
    pub fn report(&self) -> MetricsReport {
        MetricsReport {
            uptime: self.uptime(),
            
            tx_received: self.tx_received,
            tx_added: self.tx_added,
            tx_replaced: self.tx_replaced,
            tx_rejected: self.tx_rejected,
            tx_rate: self.tx_rate.rate(),
            acceptance_rate: self.acceptance_rate(),
            
            blocks_built: self.blocks_built,
            block_rate: self.block_rate.rate(),
            total_gas_used: self.total_gas_used,
            total_value: self.total_value_extracted,
            
            avg_block_fullness: self.block_fullness.mean(),
            avg_block_value: self.block_value.mean(),
            avg_block_txs: self.block_tx_count.mean(),
            
            avg_tx_gas: self.tx_gas.mean(),
            avg_tx_fee: self.tx_fee.mean(),
            
            p50_fee: self.tx_fee.percentile(50.0),
            p95_fee: self.tx_fee.percentile(95.0),
            p99_fee: self.tx_fee.percentile(99.0),
        }
    }
}

impl Default for BuilderMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of metrics for reporting
#[derive(Debug, Clone)]
pub struct MetricsReport {
    pub uptime: Duration,
    
    // Transaction stats
    pub tx_received: u64,
    pub tx_added: u64,
    pub tx_replaced: u64,
    pub tx_rejected: u64,
    pub tx_rate: f64,
    pub acceptance_rate: f64,
    
    // Block stats
    pub blocks_built: u64,
    pub block_rate: f64,
    pub total_gas_used: u64,
    pub total_value: u64,
    
    // Averages
    pub avg_block_fullness: f64,
    pub avg_block_value: f64,
    pub avg_block_txs: f64,
    pub avg_tx_gas: f64,
    pub avg_tx_fee: f64,
    
    // Percentiles
    pub p50_fee: f64,
    pub p95_fee: f64,
    pub p99_fee: f64,
}

impl MetricsReport {
    pub fn display(&self) -> String {
        format!(
            r#"
================== BUILDER METRICS ==================
Uptime: {:?}

TRANSACTIONS
  Received:     {:>12}    Rate: {:.1}/s
  Added:        {:>12}    Acceptance: {:.1}%
  Replaced:     {:>12}
  Rejected:     {:>12}

BLOCKS
  Built:        {:>12}    Rate: {:.4}/s
  Total Gas:    {:>12}
  Total Value:  {:>12}

AVERAGES (rolling window)
  Block Fullness: {:>8.1}%
  Block Value:    {:>12.0}
  Block TXs:      {:>8.1}
  TX Gas:         {:>12.0}
  TX Fee:         {:>8.1} gwei

FEE PERCENTILES
  p50: {:>6.1}  p95: {:>6.1}  p99: {:>6.1} gwei
====================================================
"#,
            self.uptime,
            self.tx_received, self.tx_rate,
            self.tx_added, self.acceptance_rate * 100.0,
            self.tx_replaced,
            self.tx_rejected,
            self.blocks_built, self.block_rate,
            self.total_gas_used,
            self.total_value,
            self.avg_block_fullness,
            self.avg_block_value,
            self.avg_block_txs,
            self.avg_tx_gas,
            self.avg_tx_fee,
            self.p50_fee, self.p95_fee, self.p99_fee,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_stats() {
        let mut stats = RollingStats::new(5);
        
        for i in 1..=5 {
            stats.push(i as f64);
        }
        
        assert_eq!(stats.count(), 5);
        assert_eq!(stats.mean(), 3.0);
        assert_eq!(stats.min(), 1.0);
        assert_eq!(stats.max(), 5.0);
        
        // Push more to trigger eviction
        stats.push(10.0);
        assert_eq!(stats.count(), 5);
        assert_eq!(stats.min(), 2.0); // 1 was evicted
    }

    #[test]
    fn test_histogram() {
        let mut hist = Histogram::new(0.0, 100.0, 10);
        
        for i in 0..100 {
            hist.record(i as f64);
        }
        
        assert_eq!(hist.count(), 100);
        assert!((hist.mean() - 49.5).abs() < 0.1);
        
        let buckets = hist.buckets();
        assert_eq!(buckets.len(), 10);
    }

    #[test]
    fn test_rate_tracker() {
        let mut tracker = RateTracker::new(Duration::from_secs(1));
        let base = Instant::now();
        
        // Record 10 events over 1 second
        for i in 0..10 {
            tracker.record_at(base + Duration::from_millis(i * 100));
        }
        
        let rate = tracker.rate_at(base + Duration::from_millis(999));
        assert!(rate >= 9.0 && rate <= 11.0, "Rate was {}", rate);
    }

    #[test]
    fn test_builder_metrics() {
        let mut metrics = BuilderMetrics::new();
        
        // Record some transactions
        metrics.record_tx(21_000, 50, true, false);
        metrics.record_tx(100_000, 30, false, true);
        metrics.record_tx(50_000, 10, false, false);
        
        assert_eq!(metrics.tx_received, 3);
        assert_eq!(metrics.tx_added, 1);
        assert_eq!(metrics.tx_replaced, 1);
        assert_eq!(metrics.tx_rejected, 1);
        
        // Record a block
        metrics.record_block(10, 900_000, 5_000_000, 1_000_000);
        
        assert_eq!(metrics.blocks_built, 1);
        assert_eq!(metrics.total_gas_used, 900_000);
    }

    #[test]
    fn test_metrics_report() {
        let mut metrics = BuilderMetrics::new();
        
        for i in 0..100 {
            metrics.record_tx(21_000 + i * 1000, 10 + i, true, false);
        }
        metrics.record_block(50, 900_000, 5_000_000, 1_000_000);
        
        let report = metrics.report();
        assert_eq!(report.tx_received, 100);
        assert_eq!(report.blocks_built, 1);
        assert!(report.avg_block_fullness > 0.0);
    }

    #[test]
    fn test_percentiles() {
        let mut stats = RollingStats::new(100);
        
        for i in 1..=100 {
            stats.push(i as f64);
        }
        
        assert!((stats.percentile(50.0) - 50.0).abs() < 2.0);
        assert!((stats.percentile(95.0) - 95.0).abs() < 2.0);
    }
}