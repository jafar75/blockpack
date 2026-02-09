//! MEV Bundle support
//!
//! Bundles are atomic groups of transactions that must be included together
//! or not at all. They are typically submitted by MEV searchers.

use crate::tx::{Tx, TxId};

/// Unique identifier for a bundle
pub type BundleId = u64;

/// An atomic bundle of transactions
#[derive(Debug, Clone)]
pub struct Bundle {
    /// Unique bundle identifier
    pub id: BundleId,
    
    /// Transactions in the bundle (order matters)
    pub transactions: Vec<Tx>,
    
    /// Block number this bundle targets (None = any block)
    pub target_block: Option<u64>,
    
    /// Minimum timestamp for inclusion (None = no minimum)
    pub min_timestamp: Option<u64>,
    
    /// Maximum timestamp for inclusion (None = no maximum)
    pub max_timestamp: Option<u64>,
    
    /// Revert protection: if true, bundle is dropped if any tx reverts
    pub revert_protection: bool,
}

impl Bundle {
    /// Create a new bundle
    pub fn new(id: BundleId, transactions: Vec<Tx>) -> Self {
        Self {
            id,
            transactions,
            target_block: None,
            min_timestamp: None,
            max_timestamp: None,
            revert_protection: true,
        }
    }
    
    /// Create a bundle targeting a specific block
    pub fn for_block(id: BundleId, transactions: Vec<Tx>, target_block: u64) -> Self {
        Self {
            id,
            transactions,
            target_block: Some(target_block),
            min_timestamp: None,
            max_timestamp: None,
            revert_protection: true,
        }
    }
    
    /// Builder pattern: set target block
    pub fn with_target_block(mut self, block: u64) -> Self {
        self.target_block = Some(block);
        self
    }
    
    /// Builder pattern: set timestamp constraints
    pub fn with_timestamp_range(mut self, min: Option<u64>, max: Option<u64>) -> Self {
        self.min_timestamp = min;
        self.max_timestamp = max;
        self
    }
    
    /// Builder pattern: set revert protection
    pub fn with_revert_protection(mut self, enabled: bool) -> Self {
        self.revert_protection = enabled;
        self
    }
    
    /// Total gas required by the bundle
    pub fn total_gas(&self) -> u64 {
        self.transactions.iter().map(|tx| tx.gas_limit).sum()
    }
    
    /// Total value (fees) of the bundle
    pub fn total_value(&self) -> u64 {
        self.transactions.iter().map(|tx| tx.value()).sum()
    }
    
    /// Total value at a specific base fee
    pub fn total_value_at_base_fee(&self, base_fee: u64) -> u64 {
        self.transactions.iter().map(|tx| tx.value_at_base_fee(base_fee)).sum()
    }
    
    /// Bundle value density (value per gas)
    pub fn value_density(&self) -> f64 {
        let gas = self.total_gas();
        if gas == 0 {
            return 0.0;
        }
        self.total_value() as f64 / gas as f64
    }
    
    /// Bundle value density at a specific base fee
    pub fn value_density_at_base_fee(&self, base_fee: u64) -> f64 {
        let gas = self.total_gas();
        if gas == 0 {
            return 0.0;
        }
        self.total_value_at_base_fee(base_fee) as f64 / gas as f64
    }
    
    /// Number of transactions in the bundle
    pub fn tx_count(&self) -> usize {
        self.transactions.len()
    }
    
    /// Check if bundle is empty
    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }
    
    /// Check if bundle is valid for a given block number
    pub fn is_valid_for_block(&self, block_number: u64) -> bool {
        match self.target_block {
            Some(target) => target == block_number,
            None => true,
        }
    }
    
    /// Check if bundle is valid for a given timestamp
    pub fn is_valid_for_timestamp(&self, timestamp: u64) -> bool {
        if let Some(min) = self.min_timestamp {
            if timestamp < min {
                return false;
            }
        }
        if let Some(max) = self.max_timestamp {
            if timestamp > max {
                return false;
            }
        }
        true
    }
    
    /// Check if all transactions can afford the base fee
    pub fn can_afford_base_fee(&self, base_fee: u64) -> bool {
        self.transactions.iter().all(|tx| tx.can_afford_base_fee(base_fee))
    }
    
    /// Get all transaction IDs in the bundle
    pub fn tx_ids(&self) -> Vec<TxId> {
        self.transactions.iter().map(|tx| tx.id).collect()
    }
}

/// Pool for managing bundles
#[derive(Debug, Default)]
pub struct BundlePool {
    /// All bundles indexed by ID
    bundles: std::collections::HashMap<BundleId, Bundle>,
    
    /// Maximum number of bundles to hold
    max_size: usize,
    
    /// Stats
    total_received: u64,
    total_included: u64,
    total_expired: u64,
}

impl BundlePool {
    /// Create a new bundle pool
    pub fn new(max_size: usize) -> Self {
        Self {
            bundles: std::collections::HashMap::with_capacity(max_size.min(1000)),
            max_size,
            total_received: 0,
            total_included: 0,
            total_expired: 0,
        }
    }
    
    /// Add a bundle to the pool
    /// Returns false if pool is full and bundle wasn't better than worst
    pub fn add(&mut self, bundle: Bundle) -> bool {
        self.total_received += 1;
        
        // Replace if same ID exists
        if self.bundles.contains_key(&bundle.id) {
            self.bundles.insert(bundle.id, bundle);
            return true;
        }
        
        // Check capacity
        if self.bundles.len() >= self.max_size {
            // Find worst bundle by value density
            let worst = self.bundles
                .iter()
                .min_by(|(_, a), (_, b)| {
                    a.value_density()
                        .partial_cmp(&b.value_density())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(&id, b)| (id, b.value_density()));
            
            if let Some((worst_id, worst_density)) = worst {
                if bundle.value_density() > worst_density {
                    self.bundles.remove(&worst_id);
                } else {
                    return false;
                }
            }
        }
        
        self.bundles.insert(bundle.id, bundle);
        true
    }
    
    /// Remove a bundle
    pub fn remove(&mut self, id: BundleId) -> Option<Bundle> {
        self.bundles.remove(&id)
    }
    
    /// Get a bundle by ID
    pub fn get(&self, id: BundleId) -> Option<&Bundle> {
        self.bundles.get(&id)
    }
    
    /// Get all bundles valid for a specific block
    pub fn get_for_block(&self, block_number: u64) -> Vec<&Bundle> {
        self.bundles
            .values()
            .filter(|b| b.is_valid_for_block(block_number))
            .collect()
    }
    
    /// Get all bundles sorted by value density (highest first)
    pub fn sorted_by_density(&self) -> Vec<&Bundle> {
        let mut bundles: Vec<_> = self.bundles.values().collect();
        bundles.sort_by(|a, b| {
            b.value_density()
                .partial_cmp(&a.value_density())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        bundles
    }
    
    /// Get bundles for a block, sorted by density
    pub fn get_for_block_sorted(&self, block_number: u64) -> Vec<&Bundle> {
        let mut bundles = self.get_for_block(block_number);
        bundles.sort_by(|a, b| {
            b.value_density()
                .partial_cmp(&a.value_density())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        bundles
    }
    
    /// Mark a bundle as included
    pub fn mark_included(&mut self, id: BundleId) {
        if self.bundles.remove(&id).is_some() {
            self.total_included += 1;
        }
    }
    
    /// Remove expired bundles (past target block)
    pub fn remove_expired(&mut self, current_block: u64) -> usize {
        let expired: Vec<_> = self.bundles
            .iter()
            .filter(|(_, b)| {
                b.target_block.map(|t| t < current_block).unwrap_or(false)
            })
            .map(|(&id, _)| id)
            .collect();
        
        let count = expired.len();
        for id in expired {
            self.bundles.remove(&id);
        }
        self.total_expired += count as u64;
        count
    }
    
    /// Remove bundles with expired timestamps
    pub fn remove_expired_by_timestamp(&mut self, current_timestamp: u64) -> usize {
        let expired: Vec<_> = self.bundles
            .iter()
            .filter(|(_, b)| {
                b.max_timestamp.map(|t| t < current_timestamp).unwrap_or(false)
            })
            .map(|(&id, _)| id)
            .collect();
        
        let count = expired.len();
        for id in expired {
            self.bundles.remove(&id);
        }
        self.total_expired += count as u64;
        count
    }
    
    /// Number of bundles in the pool
    pub fn len(&self) -> usize {
        self.bundles.len()
    }
    
    /// Check if pool is empty
    pub fn is_empty(&self) -> bool {
        self.bundles.is_empty()
    }
    
    /// Clear all bundles
    pub fn clear(&mut self) {
        self.bundles.clear();
    }
    
    /// Get pool statistics
    pub fn stats(&self) -> BundlePoolStats {
        BundlePoolStats {
            count: self.bundles.len(),
            total_received: self.total_received,
            total_included: self.total_included,
            total_expired: self.total_expired,
        }
    }
}

/// Statistics about the bundle pool
#[derive(Debug, Clone, Default)]
pub struct BundlePoolStats {
    pub count: usize,
    pub total_received: u64,
    pub total_included: u64,
    pub total_expired: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_tx(id: TxId, gas: u64, fee: u64) -> Tx {
        Tx::new(id, gas, fee)
    }
    
    fn make_bundle(id: BundleId, txs: Vec<(TxId, u64, u64)>) -> Bundle {
        let transactions = txs.into_iter()
            .map(|(id, gas, fee)| make_tx(id, gas, fee))
            .collect();
        Bundle::new(id, transactions)
    }
    
    #[test]
    fn test_bundle_new() {
        let bundle = make_bundle(1, vec![(1, 21_000, 10), (2, 50_000, 20)]);
        
        assert_eq!(bundle.id, 1);
        assert_eq!(bundle.tx_count(), 2);
        assert_eq!(bundle.total_gas(), 71_000);
        assert_eq!(bundle.total_value(), 21_000 * 10 + 50_000 * 20);
    }
    
    #[test]
    fn test_bundle_value_density() {
        let bundle = make_bundle(1, vec![(1, 100_000, 20)]);
        
        assert_eq!(bundle.value_density(), 20.0);
    }
    
    #[test]
    fn test_bundle_for_block() {
        let bundle = Bundle::for_block(
            1,
            vec![make_tx(1, 21_000, 10)],
            100,
        );
        
        assert!(bundle.is_valid_for_block(100));
        assert!(!bundle.is_valid_for_block(99));
        assert!(!bundle.is_valid_for_block(101));
    }
    
    #[test]
    fn test_bundle_timestamp_range() {
        let bundle = Bundle::new(1, vec![make_tx(1, 21_000, 10)])
            .with_timestamp_range(Some(100), Some(200));
        
        assert!(!bundle.is_valid_for_timestamp(99));
        assert!(bundle.is_valid_for_timestamp(100));
        assert!(bundle.is_valid_for_timestamp(150));
        assert!(bundle.is_valid_for_timestamp(200));
        assert!(!bundle.is_valid_for_timestamp(201));
    }
    
    #[test]
    fn test_bundle_can_afford_base_fee() {
        let txs = vec![
            Tx::new_eip1559(1, 21_000, 10, 50),
            Tx::new_eip1559(2, 21_000, 10, 100),
        ];
        let bundle = Bundle::new(1, txs);
        
        assert!(bundle.can_afford_base_fee(50));
        assert!(!bundle.can_afford_base_fee(60)); // First tx can't afford
    }
    
    #[test]
    fn test_bundle_pool_add() {
        let mut pool = BundlePool::new(10);
        
        let bundle = make_bundle(1, vec![(1, 21_000, 10)]);
        assert!(pool.add(bundle));
        
        assert_eq!(pool.len(), 1);
        assert!(pool.get(1).is_some());
    }
    
    #[test]
    fn test_bundle_pool_capacity() {
        let mut pool = BundlePool::new(2);
        
        pool.add(make_bundle(1, vec![(1, 21_000, 10)])); // density = 10
        pool.add(make_bundle(2, vec![(2, 21_000, 20)])); // density = 20
        
        assert_eq!(pool.len(), 2);
        
        // Add better bundle, should evict bundle 1
        pool.add(make_bundle(3, vec![(3, 21_000, 30)])); // density = 30
        
        assert_eq!(pool.len(), 2);
        assert!(pool.get(1).is_none());
        assert!(pool.get(2).is_some());
        assert!(pool.get(3).is_some());
    }
    
    #[test]
    fn test_bundle_pool_get_for_block() {
        let mut pool = BundlePool::new(10);
        
        pool.add(Bundle::for_block(1, vec![make_tx(1, 21_000, 10)], 100));
        pool.add(Bundle::for_block(2, vec![make_tx(2, 21_000, 20)], 101));
        pool.add(Bundle::new(3, vec![make_tx(3, 21_000, 30)])); // Any block
        
        let for_100 = pool.get_for_block(100);
        assert_eq!(for_100.len(), 2); // Bundle 1 and 3
        
        let for_101 = pool.get_for_block(101);
        assert_eq!(for_101.len(), 2); // Bundle 2 and 3
    }
    
    #[test]
    fn test_bundle_pool_remove_expired() {
        let mut pool = BundlePool::new(10);
        
        pool.add(Bundle::for_block(1, vec![make_tx(1, 21_000, 10)], 100));
        pool.add(Bundle::for_block(2, vec![make_tx(2, 21_000, 20)], 101));
        pool.add(Bundle::for_block(3, vec![make_tx(3, 21_000, 30)], 102));
        
        let removed = pool.remove_expired(102);
        
        assert_eq!(removed, 2); // Bundles 1 and 2 expired
        assert_eq!(pool.len(), 1);
        assert!(pool.get(3).is_some());
    }
    
    #[test]
    fn test_bundle_pool_sorted() {
        let mut pool = BundlePool::new(10);
        
        pool.add(make_bundle(1, vec![(1, 21_000, 10)])); // density = 10
        pool.add(make_bundle(2, vec![(2, 21_000, 30)])); // density = 30
        pool.add(make_bundle(3, vec![(3, 21_000, 20)])); // density = 20
        
        let sorted = pool.sorted_by_density();
        
        assert_eq!(sorted[0].id, 2);
        assert_eq!(sorted[1].id, 3);
        assert_eq!(sorted[2].id, 1);
    }
    
    #[test]
    fn test_bundle_pool_stats() {
        let mut pool = BundlePool::new(10);
        
        pool.add(make_bundle(1, vec![(1, 21_000, 10)]));
        pool.add(make_bundle(2, vec![(2, 21_000, 20)]));
        pool.mark_included(1);
        
        let stats = pool.stats();
        
        assert_eq!(stats.count, 1);
        assert_eq!(stats.total_received, 2);
        assert_eq!(stats.total_included, 1);
    }
}