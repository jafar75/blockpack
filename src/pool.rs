use std::collections::HashMap;
use crate::tx::{Tx, TxId};

/// A pool of pending transactions awaiting inclusion in a block
#[derive(Debug)]
pub struct Pool {
    /// All transactions indexed by ID
    txs: HashMap<TxId, Tx>,
    
    /// Maximum number of transactions to hold
    max_size: usize,
}

impl Pool {
    /// Create a new empty pool
    pub fn new(max_size: usize) -> Self {
        Self {
            txs: HashMap::with_capacity(max_size.min(1000)),
            max_size,
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
    
    /// Insert a transaction into the pool
    /// Returns None if inserted successfully
    /// Returns Some(tx) if pool is full and tx was rejected
    /// Replaces existing tx with same ID
    pub fn insert(&mut self, tx: Tx) -> Option<Tx> {
        // If tx already exists, replace it
        if self.txs.contains_key(&tx.id) {
            self.txs.insert(tx.id, tx);
            return None;
        }
        
        // If pool is full, try to evict lowest value density tx
        if self.is_full() {
            // Only evict if new tx is better than worst
            if let Some((worst_id, worst_density)) = self.find_worst() {
                if tx.value_density() > worst_density {
                    self.txs.remove(&worst_id);
                } else {
                    // New tx isn't better than worst, reject it
                    return Some(tx);
                }
            }
        }
        
        self.txs.insert(tx.id, tx);
        None
    }
    
    /// Remove a transaction by ID
    pub fn remove(&mut self, id: TxId) -> Option<Tx> {
        self.txs.remove(&id)
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
    /// Returns (id, density) if pool is non-empty
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
    
    /// Iterate over all transactions (arbitrary order)
    pub fn iter(&self) -> impl Iterator<Item = &Tx> {
        self.txs.values()
    }
    
    /// Remove multiple transactions by ID
    /// Returns the number of transactions removed
    pub fn remove_many(&mut self, ids: &[TxId]) -> usize {
        let mut removed = 0;
        for id in ids {
            if self.txs.remove(id).is_some() {
                removed += 1;
            }
        }
        removed
    }
    
    /// Clear all transactions from the pool
    pub fn clear(&mut self) {
        self.txs.clear();
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        if self.txs.is_empty() {
            return PoolStats::default();
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
        pool.insert(make_tx(1, 50_000, 20)); // Same ID, different values
        
        assert_eq!(pool.len(), 1);
        let tx = pool.get(1).unwrap();
        assert_eq!(tx.gas_limit, 50_000); // Should be replaced
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
        pool.insert(make_tx(1, 21_000, 10)); // density = 10
        pool.insert(make_tx(2, 21_000, 20)); // density = 20
        pool.insert(make_tx(3, 21_000, 30)); // density = 30
        
        assert!(pool.is_full());
        
        // Insert better tx, should evict tx 1 (lowest density)
        let rejected = pool.insert(make_tx(4, 21_000, 25));
        assert!(rejected.is_none());
        assert!(!pool.contains(1)); // Evicted
        assert!(pool.contains(4));  // Added
    }
    
    #[test]
    fn test_full_pool_rejection() {
        let mut pool = Pool::new(3);
        pool.insert(make_tx(1, 21_000, 20));
        pool.insert(make_tx(2, 21_000, 30));
        pool.insert(make_tx(3, 21_000, 40));
        
        // Insert worse tx, should be rejected
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
        assert_eq!(sorted[0].id, 2); // density 30
        assert_eq!(sorted[1].id, 3); // density 20
        assert_eq!(sorted[2].id, 1); // density 10
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
    
    #[test]
    fn test_remove_many() {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 21_000, 10));
        pool.insert(make_tx(2, 21_000, 20));
        pool.insert(make_tx(3, 21_000, 30));
        
        let removed = pool.remove_many(&[1, 3, 999]); // 999 doesn't exist
        assert_eq!(removed, 2);
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(2));
    }
    
    #[test]
    fn test_stats() {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 21_000, 10)); // value = 210_000
        pool.insert(make_tx(2, 21_000, 20)); // value = 420_000
        pool.insert(make_tx(3, 21_000, 30)); // value = 630_000
        
        let stats = pool.stats();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.total_gas, 63_000);
        assert_eq!(stats.total_value, 1_260_000);
        assert_eq!(stats.min_density, 10.0);
        assert_eq!(stats.max_density, 30.0);
        assert_eq!(stats.avg_density, 20.0);
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
}