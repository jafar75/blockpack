use std::collections::HashMap;
use crate::tx::{Tx, TxId};

/// Pool pressure level
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoolPressure {
    /// Below 50% capacity
    Low,
    /// 50-80% capacity
    Medium,
    /// Above 80% capacity
    High,
    /// At or near capacity
    Critical,
}

impl Default for PoolPressure {
    fn default() -> Self {
        PoolPressure::Low
    }
}

/// A pool of pending transactions awaiting inclusion in a block
#[derive(Debug)]
pub struct Pool {
    /// All transactions indexed by ID
    txs: HashMap<TxId, Tx>,
    
    /// Maximum number of transactions to hold
    max_size: usize,
    
    /// Minimum density threshold (dynamic based on pressure)
    min_density_threshold: f64,
    
    /// Stats tracking
    total_inserted: u64,
    total_evicted: u64,
    total_rejected: u64,
}

impl Pool {
    /// Create a new empty pool
    pub fn new(max_size: usize) -> Self {
        Self {
            txs: HashMap::with_capacity(max_size.min(1000)),
            max_size,
            min_density_threshold: 0.0,
            total_inserted: 0,
            total_evicted: 0,
            total_rejected: 0,
        }
    }
    
    /// Number of transactions in the pool
    pub fn len(&self) -> usize {
        self.txs.len()
    }
    
    /// Check if pool is empty
    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }
    
    /// Check if pool is at capacity
    pub fn is_full(&self) -> bool {
        self.txs.len() >= self.max_size
    }
    
    /// Get current pressure level
    pub fn pressure(&self) -> PoolPressure {
        let ratio = self.txs.len() as f64 / self.max_size as f64;
        if ratio >= 0.95 {
            PoolPressure::Critical
        } else if ratio >= 0.80 {
            PoolPressure::High
        } else if ratio >= 0.50 {
            PoolPressure::Medium
        } else {
            PoolPressure::Low
        }
    }
    
    /// Get fill percentage
    pub fn fill_percent(&self) -> f64 {
        (self.txs.len() as f64 / self.max_size as f64) * 100.0
    }
    
    /// Insert a transaction into the pool
    /// Returns None if inserted successfully
    /// Returns Some(tx) if pool is full and tx was rejected
    pub fn insert(&mut self, tx: Tx) -> Option<Tx> {
        // If tx already exists, replace it
        if self.txs.contains_key(&tx.id) {
            self.txs.insert(tx.id, tx);
            return None;
        }
        
        // Check against minimum threshold under pressure
        if self.pressure() != PoolPressure::Low {
            if tx.value_density() < self.min_density_threshold {
                self.total_rejected += 1;
                return Some(tx);
            }
        }
        
        // If pool is full, try to evict lowest value density tx
        if self.is_full() {
            if let Some((worst_id, worst_density)) = self.find_worst() {
                if tx.value_density() > worst_density {
                    self.txs.remove(&worst_id);
                    self.total_evicted += 1;
                    self.update_threshold();
                } else {
                    self.total_rejected += 1;
                    return Some(tx);
                }
            }
        }
        
        self.txs.insert(tx.id, tx);
        self.total_inserted += 1;
        self.update_threshold();
        None
    }
    
    /// Update the minimum density threshold based on pool state
    fn update_threshold(&mut self) {
        match self.pressure() {
            PoolPressure::Low => {
                self.min_density_threshold = 0.0;
            }
            PoolPressure::Medium => {
                if let Some(p10) = self.percentile_density(10.0) {
                    self.min_density_threshold = p10;
                }
            }
            PoolPressure::High => {
                if let Some(p25) = self.percentile_density(25.0) {
                    self.min_density_threshold = p25;
                }
            }
            PoolPressure::Critical => {
                if let Some(p50) = self.percentile_density(50.0) {
                    self.min_density_threshold = p50;
                }
            }
        }
    }
    
    /// Calculate percentile of density values
    fn percentile_density(&self, p: f64) -> Option<f64> {
        if self.txs.is_empty() {
            return None;
        }
        
        let mut densities: Vec<f64> = self.txs.values()
            .map(|tx| tx.value_density())
            .collect();
        densities.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let idx = ((p / 100.0) * (densities.len() - 1) as f64).round() as usize;
        Some(densities[idx.min(densities.len() - 1)])
    }
    
    /// Remove a transaction by ID
    pub fn remove(&mut self, id: TxId) -> Option<Tx> {
        let result = self.txs.remove(&id);
        if result.is_some() {
            self.update_threshold();
        }
        result
    }
    
    /// Get a transaction by ID
    pub fn get(&self, id: TxId) -> Option<&Tx> {
        self.txs.get(&id)
    }
    
    /// Check if a transaction exists
    pub fn contains(&self, id: TxId) -> bool {
        self.txs.contains_key(&id)
    }
    
    /// Find the transaction with lowest value density
    fn find_worst(&self) -> Option<(TxId, f64)> {
        self.txs
            .values()
            .min_by(|a, b| {
                a.value_density()
                    .partial_cmp(&b.value_density())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|tx| (tx.id, tx.value_density()))
    }
    
    /// Get all transactions sorted by value density (highest first)
    pub fn sorted_by_density(&self) -> Vec<&Tx> {
        let mut txs: Vec<_> = self.txs.values().collect();
        txs.sort_by(|a, b| {
            b.value_density()
                .partial_cmp(&a.value_density())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        txs
    }
    
    /// Get all transactions sorted by value (highest first)
    pub fn sorted_by_value(&self) -> Vec<&Tx> {
        let mut txs: Vec<_> = self.txs.values().collect();
        txs.sort_by(|a, b| b.value().cmp(&a.value()));
        txs
    }
    
    /// Iterate over all transactions
    pub fn iter(&self) -> impl Iterator<Item = &Tx> {
        self.txs.values()
    }
    
    /// Remove multiple transactions by ID
    pub fn remove_many(&mut self, ids: &[TxId]) -> usize {
        let mut removed = 0;
        for id in ids {
            if self.txs.remove(id).is_some() {
                removed += 1;
            }
        }
        if removed > 0 {
            self.update_threshold();
        }
        removed
    }
    
    /// Clear all transactions from the pool
    pub fn clear(&mut self) {
        self.txs.clear();
        self.min_density_threshold = 0.0;
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        if self.txs.is_empty() {
            return PoolStats {
                pressure: self.pressure(),
                min_threshold: self.min_density_threshold,
                total_inserted: self.total_inserted,
                total_evicted: self.total_evicted,
                total_rejected: self.total_rejected,
                ..Default::default()
            };
        }
        
        let mut total_gas = 0u64;
        let mut total_value = 0u64;
        let mut min_density = f64::MAX;
        let mut max_density = f64::MIN;
        
        for tx in self.txs.values() {
            total_gas += tx.gas_limit;
            total_value += tx.value();
            let d = tx.value_density();
            min_density = min_density.min(d);
            max_density = max_density.max(d);
        }
        
        PoolStats {
            count: self.txs.len(),
            total_gas,
            total_value,
            min_density,
            max_density,
            avg_density: total_value as f64 / total_gas as f64,
            pressure: self.pressure(),
            min_threshold: self.min_density_threshold,
            total_inserted: self.total_inserted,
            total_evicted: self.total_evicted,
            total_rejected: self.total_rejected,
        }
    }
}

/// Statistics about the pool state
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    pub count: usize,
    pub total_gas: u64,
    pub total_value: u64,
    pub min_density: f64,
    pub max_density: f64,
    pub avg_density: f64,
    pub pressure: PoolPressure,
    pub min_threshold: f64,
    pub total_inserted: u64,
    pub total_evicted: u64,
    pub total_rejected: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_tx(id: TxId, gas: u64, fee: u64) -> Tx {
        Tx::new(id, gas, fee)
    }
    
    #[test]
    fn test_new_pool() {
        let pool = Pool::new(100);
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
        assert!(!pool.is_full());
        assert_eq!(pool.pressure(), PoolPressure::Low);
    }
    
    #[test]
    fn test_insert_and_get() {
        let mut pool = Pool::new(100);
        let tx = make_tx(1, 21_000, 10);
        
        assert!(pool.insert(tx).is_none());
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(1));
        
        let retrieved = pool.get(1).unwrap();
        assert_eq!(retrieved.id, 1);
        assert_eq!(retrieved.gas_limit, 21_000);
    }
    
    #[test]
    fn test_insert_duplicate_replaces() {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 21_000, 10));
        pool.insert(make_tx(1, 50_000, 20));
        
        assert_eq!(pool.len(), 1);
        let tx = pool.get(1).unwrap();
        assert_eq!(tx.gas_limit, 50_000);
    }
    
    #[test]
    fn test_remove() {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 21_000, 10));
        pool.insert(make_tx(2, 21_000, 20));
        
        let removed = pool.remove(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, 1);
        assert_eq!(pool.len(), 1);
        assert!(!pool.contains(1));
    }
    
    #[test]
    fn test_full_pool_eviction() {
        let mut pool = Pool::new(3);
        pool.insert(make_tx(1, 21_000, 10));
        pool.insert(make_tx(2, 21_000, 20));
        pool.insert(make_tx(3, 21_000, 30));
        
        assert!(pool.is_full());
        
        let rejected = pool.insert(make_tx(4, 21_000, 25));
        assert!(rejected.is_none());
        assert!(!pool.contains(1));
        assert!(pool.contains(4));
    }
    
    #[test]
    fn test_full_pool_rejection() {
        let mut pool = Pool::new(3);
        pool.insert(make_tx(1, 21_000, 20));
        pool.insert(make_tx(2, 21_000, 30));
        pool.insert(make_tx(3, 21_000, 40));
        
        let rejected = pool.insert(make_tx(4, 21_000, 10));
        assert!(rejected.is_some());
        assert_eq!(rejected.unwrap().id, 4);
        assert!(!pool.contains(4));
    }
    
    #[test]
    fn test_sorted_by_density() {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 21_000, 10));
        pool.insert(make_tx(2, 21_000, 30));
        pool.insert(make_tx(3, 21_000, 20));
        
        let sorted = pool.sorted_by_density();
        assert_eq!(sorted[0].id, 2);
        assert_eq!(sorted[1].id, 3);
        assert_eq!(sorted[2].id, 1);
    }
    
    #[test]
    fn test_pool_pressure_levels() {
        let mut pool = Pool::new(100);
        
        // Low pressure (< 50%)
        for i in 1..=40 {
            pool.insert(make_tx(i, 21_000, 10));
        }
        assert_eq!(pool.pressure(), PoolPressure::Low);
        
        // Medium pressure (50-80%)
        for i in 41..=70 {
            pool.insert(make_tx(i, 21_000, 10));
        }
        assert_eq!(pool.pressure(), PoolPressure::Medium);
        
        // High pressure (80-95%)
        for i in 71..=90 {
            pool.insert(make_tx(i, 21_000, 10));
        }
        assert_eq!(pool.pressure(), PoolPressure::High);
        
        // Critical pressure (>= 95%)
        for i in 91..=100 {
            pool.insert(make_tx(i, 21_000, 10));
        }
        assert_eq!(pool.pressure(), PoolPressure::Critical);
    }
    
    #[test]
    fn test_stats_with_pressure() {
        let mut pool = Pool::new(10);
        for i in 1..=8 {
            pool.insert(make_tx(i, 21_000, i * 10));
        }
        
        let stats = pool.stats();
        assert_eq!(stats.count, 8);
        assert_eq!(stats.pressure, PoolPressure::High);
        assert!(stats.total_inserted > 0);
    }
    
    #[test]
    fn test_remove_many() {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 21_000, 10));
        pool.insert(make_tx(2, 21_000, 20));
        pool.insert(make_tx(3, 21_000, 30));
        
        let removed = pool.remove_many(&[1, 3, 999]);
        assert_eq!(removed, 2);
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(2));
    }
    
    #[test]
    fn test_clear() {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 21_000, 10));
        pool.insert(make_tx(2, 21_000, 20));
        
        pool.clear();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }
    
    #[test]
    fn test_fill_percent() {
        let mut pool = Pool::new(100);
        assert_eq!(pool.fill_percent(), 0.0);
        
        for i in 1..=50 {
            pool.insert(make_tx(i, 21_000, 10));
        }
        assert_eq!(pool.fill_percent(), 50.0);
    }
    
    #[test]
    fn test_sorted_by_value() {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 100_000, 10)); // value = 1_000_000
        pool.insert(make_tx(2, 21_000, 30));  // value = 630_000
        pool.insert(make_tx(3, 50_000, 25));  // value = 1_250_000
        
        let sorted = pool.sorted_by_value();
        assert_eq!(sorted[0].id, 3); // value 1_250_000
        assert_eq!(sorted[1].id, 1); // value 1_000_000
        assert_eq!(sorted[2].id, 2); // value 630_000
    }
}