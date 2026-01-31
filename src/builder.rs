use crate::block::Block;
use crate::pool::Pool;
use crate::tx::{Tx, TxId};

/// Result of a block building operation
#[derive(Debug)]
pub struct BuildResult {
    /// The built block
    pub block: Block,
    
    /// IDs of transactions that were included
    pub included: Vec<TxId>,
    
    /// Number of transactions considered
    pub considered: usize,
    
    /// Number of transactions skipped (didn't fit)
    pub skipped: usize,
}

/// Result of an incremental update attempt
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateResult {
    /// Transaction was added (had space)
    Added,
    
    /// Transaction replaced another (with the evicted tx id)
    Replaced(TxId),
    
    /// Transaction was rejected (not better than worst)
    Rejected,
    
    /// Transaction already exists in block
    Duplicate,
}

/// Statistics about incremental building
#[derive(Debug, Clone, Default)]
pub struct IncrementalStats {
    pub txs_received: u64,
    pub txs_added: u64,
    pub txs_replaced: u64,
    pub txs_rejected: u64,
    pub txs_duplicate: u64,
}

impl IncrementalStats {
    pub fn record(&mut self, result: &UpdateResult) {
        self.txs_received += 1;
        match result {
            UpdateResult::Added => self.txs_added += 1,
            UpdateResult::Replaced(_) => self.txs_replaced += 1,
            UpdateResult::Rejected => self.txs_rejected += 1,
            UpdateResult::Duplicate => self.txs_duplicate += 1,
        }
    }
    
    pub fn acceptance_rate(&self) -> f64 {
        if self.txs_received == 0 {
            return 0.0;
        }
        (self.txs_added + self.txs_replaced) as f64 / self.txs_received as f64
    }
}

/// Block builder with greedy and incremental algorithms
#[derive(Debug)]
pub struct Builder {
    /// Gas limit for blocks
    block_gas_limit: u64,
    
    /// Current block number
    next_block_number: u64,
    
    /// Current candidate block being built incrementally
    candidate: Option<Block>,
    
    /// Stats for incremental building
    stats: IncrementalStats,
}

impl Builder {
    /// Create a new builder
    pub fn new(block_gas_limit: u64) -> Self {
        Self {
            block_gas_limit,
            next_block_number: 1,
            candidate: None,
            stats: IncrementalStats::default(),
        }
    }
    
    /// Create a new builder starting at a specific block number
    pub fn with_start_block(block_gas_limit: u64, start_block: u64) -> Self {
        Self {
            block_gas_limit,
            next_block_number: start_block,
            candidate: None,
            stats: IncrementalStats::default(),
        }
    }
    
    /// Get or create the candidate block
    fn get_or_create_candidate(&mut self) -> &mut Block {
        if self.candidate.is_none() {
            self.candidate = Some(Block::new(self.next_block_number, self.block_gas_limit));
        }
        self.candidate.as_mut().unwrap()
    }
    
    /// Process a new transaction incrementally
    /// 
    /// Algorithm:
    /// 1. If tx already in block, reject as duplicate
    /// 2. If tx fits in remaining space, add it
    /// 3. If tx doesn't fit, check if it's better than worst tx
    /// 4. If better and would fit after eviction, replace
    /// 5. Otherwise reject
    pub fn process_tx(&mut self, tx: Tx) -> UpdateResult {
        let candidate = self.get_or_create_candidate();
        
        // Check for duplicate
        if candidate.transactions.iter().any(|t| t.id == tx.id) {
            let result = UpdateResult::Duplicate;
            self.stats.record(&result);
            return result;
        }
        
        // Try to add directly
        if candidate.can_fit(&tx) {
            candidate.add_tx(tx);
            let result = UpdateResult::Added;
            self.stats.record(&result);
            return result;
        }
        
        // Block is full (for this tx), try replacement
        let result = self.try_replace(tx);
        self.stats.record(&result);
        result
    }
    
    /// Try to replace worst transaction(s) with new tx
    fn try_replace(&mut self, new_tx: Tx) -> UpdateResult {
        let candidate = self.candidate.as_mut().unwrap();
        
        // Find worst tx by density
        let worst = match candidate.worst_tx() {
            Some((_, tx)) => tx.clone(),
            None => return UpdateResult::Rejected,
        };
        
        // New tx must be better than worst
        if new_tx.value_density() <= worst.value_density() {
            return UpdateResult::Rejected;
        }
        
        // Check if new tx would fit after removing worst
        let space_after_eviction = candidate.remaining_gas() + worst.gas_limit;
        if new_tx.gas_limit > space_after_eviction {
            // Still doesn't fit, try multi-eviction
            return self.try_multi_replace(new_tx);
        }
        
        // Single replacement works
        let evicted_id = worst.id;
        candidate.remove_tx(evicted_id);
        candidate.add_tx(new_tx);
        
        UpdateResult::Replaced(evicted_id)
    }
    
    /// Try to replace multiple worst transactions
    /// Only if total value improves
    fn try_multi_replace(&mut self, new_tx: Tx) -> UpdateResult {
        let candidate = self.candidate.as_mut().unwrap();
        
        // Collect txs sorted by density (worst first)
        let mut sorted: Vec<_> = candidate.transactions.iter().cloned().collect();
        sorted.sort_by(|a, b| {
            a.value_density()
                .partial_cmp(&b.value_density())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Find minimum set of worst txs to evict
        let mut to_evict = Vec::new();
        let mut freed_gas = candidate.remaining_gas();
        let mut evicted_value = 0u64;
        
        for tx in &sorted {
            if freed_gas >= new_tx.gas_limit {
                break;
            }
            // Only evict if new tx has better density
            if new_tx.value_density() <= tx.value_density() {
                break;
            }
            to_evict.push(tx.id);
            freed_gas += tx.gas_limit;
            evicted_value += tx.value();
        }
        
        // Check if we can fit and if it's worth it
        if freed_gas < new_tx.gas_limit {
            return UpdateResult::Rejected;
        }
        
        // Must be net positive value
        if new_tx.value() <= evicted_value {
            return UpdateResult::Rejected;
        }
        
        // Need at least one tx to evict
        if to_evict.is_empty() {
            return UpdateResult::Rejected;
        }
        
        // Perform the evictions
        let first_evicted = to_evict[0];
        for id in &to_evict {
            candidate.remove_tx(*id);
        }
        
        // Verify we actually have space now (sanity check)
        if !candidate.can_fit(&new_tx) {
            // This shouldn't happen, but if it does, rollback would be needed
            // For now, just reject (defensive programming)
            return UpdateResult::Rejected;
        }
        
        candidate.add_tx(new_tx);
        
        // Return first evicted (could extend to return all)
        UpdateResult::Replaced(first_evicted)
    }
    
    /// Get reference to current candidate block
    pub fn candidate(&self) -> Option<&Block> {
        self.candidate.as_ref()
    }
    
    /// Get mutable reference to current candidate block
    pub fn candidate_mut(&mut self) -> Option<&mut Block> {
        self.candidate.as_mut()
    }
    
    /// Seal the candidate block and return it
    /// Resets for next block
    pub fn seal(&mut self) -> Option<Block> {
        let block = self.candidate.take();
        if block.is_some() {
            self.next_block_number += 1;
            self.stats = IncrementalStats::default();
        }
        block
    }
    
    /// Get incremental stats
    pub fn stats(&self) -> &IncrementalStats {
        &self.stats
    }
    
    /// Build a block from the pool using greedy by density
    pub fn build_greedy(&mut self, pool: &Pool) -> BuildResult {
        let mut block = Block::new(self.next_block_number, self.block_gas_limit);
        let mut included = Vec::new();
        let mut skipped = 0;
        
        let sorted = pool.sorted_by_density();
        let considered = sorted.len();
        
        for tx in sorted {
            if block.can_fit(tx) {
                included.push(tx.id);
                block.add_tx(tx.clone());
            } else {
                skipped += 1;
            }
        }
        
        BuildResult { block, included, considered, skipped }
    }
    
    /// Build a block using greedy, then try to fill gaps
    pub fn build_greedy_with_backfill(&mut self, pool: &Pool) -> BuildResult {
        let mut block = Block::new(self.next_block_number, self.block_gas_limit);
        let mut included = Vec::new();
        let mut skipped_txs: Vec<&Tx> = Vec::new();
        
        let sorted = pool.sorted_by_density();
        let considered = sorted.len();
        
        for tx in sorted {
            if block.can_fit(tx) {
                included.push(tx.id);
                block.add_tx(tx.clone());
            } else {
                skipped_txs.push(tx);
            }
        }
        
        skipped_txs.sort_by_key(|tx| tx.gas_limit);
        
        let mut final_skipped = 0;
        for tx in skipped_txs {
            if block.can_fit(tx) {
                included.push(tx.id);
                block.add_tx(tx.clone());
            } else {
                final_skipped += 1;
            }
        }
        
        BuildResult { block, included, considered, skipped: final_skipped }
    }
    
    /// Finalize without returning block (just advance number)
    pub fn seal_block(&mut self) {
        self.candidate = None;
        self.next_block_number += 1;
        self.stats = IncrementalStats::default();
    }
    
    pub fn next_block_number(&self) -> u64 {
        self.next_block_number
    }
    
    pub fn gas_limit(&self) -> u64 {
        self.block_gas_limit
    }
}

/// Standalone function for one-off greedy build
pub fn greedy_build(pool: &Pool, block_number: u64, gas_limit: u64) -> BuildResult {
    let mut builder = Builder::with_start_block(gas_limit, block_number);
    builder.build_greedy(pool)
}

/// Standalone function for one-off greedy build with backfill
pub fn greedy_build_with_backfill(pool: &Pool, block_number: u64, gas_limit: u64) -> BuildResult {
    let mut builder = Builder::with_start_block(gas_limit, block_number);
    builder.build_greedy_with_backfill(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_tx(id: TxId, gas: u64, fee: u64) -> Tx {
        Tx::new(id, gas, fee)
    }
    
    fn setup_pool() -> Pool {
        let mut pool = Pool::new(100);
        pool.insert(make_tx(1, 21_000, 50));
        pool.insert(make_tx(2, 100_000, 30));
        pool.insert(make_tx(3, 250_000, 10));
        pool.insert(make_tx(4, 21_000, 100));
        pool.insert(make_tx(5, 500_000, 5));
        pool
    }
    
    #[test]
    fn test_process_tx_add() {
        let mut builder = Builder::new(100_000);
        
        let result = builder.process_tx(make_tx(1, 21_000, 50));
        assert_eq!(result, UpdateResult::Added);
        
        let candidate = builder.candidate().unwrap();
        assert_eq!(candidate.tx_count(), 1);
        assert_eq!(candidate.gas_used, 21_000);
    }
    
    #[test]
    fn test_process_tx_duplicate() {
        let mut builder = Builder::new(100_000);
        
        builder.process_tx(make_tx(1, 21_000, 50));
        let result = builder.process_tx(make_tx(1, 21_000, 100)); // Same ID
        
        assert_eq!(result, UpdateResult::Duplicate);
        assert_eq!(builder.candidate().unwrap().tx_count(), 1);
    }
    
    #[test]
    fn test_process_tx_replace_better() {
        let mut builder = Builder::new(50_000);
        
        // Fill block
        builder.process_tx(make_tx(1, 25_000, 10)); // density 10
        builder.process_tx(make_tx(2, 25_000, 20)); // density 20
        
        // Block is full, try adding better tx
        let result = builder.process_tx(make_tx(3, 25_000, 50)); // density 50
        
        assert!(matches!(result, UpdateResult::Replaced(1))); // Should evict tx 1
        
        let candidate = builder.candidate().unwrap();
        assert_eq!(candidate.tx_count(), 2);
        assert!(!candidate.transactions.iter().any(|t| t.id == 1));
        assert!(candidate.transactions.iter().any(|t| t.id == 3));
    }
    
    #[test]
    fn test_process_tx_reject_worse() {
        let mut builder = Builder::new(50_000);
        
        // Fill with good txs
        builder.process_tx(make_tx(1, 25_000, 50)); // density 50
        builder.process_tx(make_tx(2, 25_000, 40)); // density 40
        
        // Try adding worse tx
        let result = builder.process_tx(make_tx(3, 25_000, 30)); // density 30
        
        assert_eq!(result, UpdateResult::Rejected);
        assert_eq!(builder.candidate().unwrap().tx_count(), 2);
    }
    
    #[test]
    fn test_process_tx_multi_replace() {
        let mut builder = Builder::new(100_000);
        
        // Fill with small low-density txs
        builder.process_tx(make_tx(1, 30_000, 10)); // density 10
        builder.process_tx(make_tx(2, 30_000, 15)); // density 15
        builder.process_tx(make_tx(3, 30_000, 20)); // density 20
        // 90k used, 10k remaining
        
        // Add large high-density tx that needs multi-eviction
        let result = builder.process_tx(make_tx(4, 70_000, 50)); // density 50
        
        // Should evict tx 1 and tx 2 (lowest density) to make room
        assert!(matches!(result, UpdateResult::Replaced(_)));
        
        let candidate = builder.candidate().unwrap();
        assert!(candidate.transactions.iter().any(|t| t.id == 4));
        // Value should have improved
        assert!(candidate.total_value > 30_000 * 10 + 30_000 * 15 + 30_000 * 20);
    }
    
    #[test]
    fn test_seal_returns_block() {
        let mut builder = Builder::new(100_000);
        
        builder.process_tx(make_tx(1, 21_000, 50));
        builder.process_tx(make_tx(2, 21_000, 60));
        
        let block = builder.seal();
        assert!(block.is_some());
        
        let block = block.unwrap();
        assert_eq!(block.number, 1);
        assert_eq!(block.tx_count(), 2);
        
        // Builder should be ready for next block
        assert_eq!(builder.next_block_number(), 2);
        assert!(builder.candidate().is_none());
    }
    
    #[test]
    fn test_incremental_stats() {
        let mut builder = Builder::new(50_000);
        
        builder.process_tx(make_tx(1, 20_000, 10)); // Added
        builder.process_tx(make_tx(2, 20_000, 20)); // Added
        builder.process_tx(make_tx(1, 20_000, 30)); // Duplicate
        builder.process_tx(make_tx(3, 20_000, 50)); // Replaced
        builder.process_tx(make_tx(4, 20_000, 5));  // Rejected
        
        let stats = builder.stats();
        assert_eq!(stats.txs_received, 5);
        assert_eq!(stats.txs_added, 2);
        assert_eq!(stats.txs_duplicate, 1);
        assert_eq!(stats.txs_replaced, 1);
        assert_eq!(stats.txs_rejected, 1);
        assert!((stats.acceptance_rate() - 0.6).abs() < 0.001);
    }
    
    #[test]
    fn test_stats_reset_on_seal() {
        let mut builder = Builder::new(100_000);
        
        builder.process_tx(make_tx(1, 21_000, 50));
        assert_eq!(builder.stats().txs_received, 1);
        
        builder.seal();
        assert_eq!(builder.stats().txs_received, 0);
    }
    
    #[test]
    fn test_greedy_build_all_fit() {
        let pool = setup_pool();
        let mut builder = Builder::new(30_000_000);
        
        let result = builder.build_greedy(&pool);
        
        assert_eq!(result.considered, 5);
        assert_eq!(result.included.len(), 5);
        assert_eq!(result.skipped, 0);
    }
    
    #[test]
    fn test_multi_replace_respects_gas_limit() {
        let mut builder = Builder::new(100_000);
        
        // Fill block completely
        builder.process_tx(make_tx(1, 50_000, 10)); // density 10
        builder.process_tx(make_tx(2, 50_000, 15)); // density 15
        // Block is exactly full: 100k used
        
        assert_eq!(builder.candidate().unwrap().gas_used, 100_000);
        
        // Try to add tx that's larger than any single tx
        // Would need to evict both, but tx is bigger than block
        let result = builder.process_tx(make_tx(3, 120_000, 50));
        assert_eq!(result, UpdateResult::Rejected);
        
        // Gas should still be exactly at limit
        assert_eq!(builder.candidate().unwrap().gas_used, 100_000);
        assert!(builder.candidate().unwrap().gas_used <= builder.gas_limit());
    }
    
    #[test]
    fn test_block_never_exceeds_gas_limit() {
        let mut builder = Builder::new(200_000);
        
        // Simulate the exact scenario from main.rs
        let stream = vec![
            make_tx(1, 50_000, 10),
            make_tx(2, 60_000, 12),
            make_tx(3, 40_000, 15),
            make_tx(4, 50_000, 8),
            make_tx(5, 45_000, 25),
            make_tx(6, 55_000, 30),
            make_tx(7, 70_000, 35),
            make_tx(8, 30_000, 20),
            make_tx(9, 80_000, 40),
            make_tx(10, 50_000, 5),
        ];
        
        for tx in stream {
            builder.process_tx(tx);
            let candidate = builder.candidate().unwrap();
            assert!(
                candidate.gas_used <= candidate.gas_limit,
                "Block gas {} exceeded limit {} after tx",
                candidate.gas_used, candidate.gas_limit
            );
        }
    }
    
    #[test]
    fn test_value_improvement_over_time() {
        let mut builder = Builder::new(100_000);
        
        // Simulate stream: worse txs arrive first
        let stream = vec![
            make_tx(1, 50_000, 10),  // density 10
            make_tx(2, 50_000, 15),  // density 15
            make_tx(3, 50_000, 30),  // density 30 - should replace tx1
            make_tx(4, 50_000, 25),  // density 25 - should replace tx2
            make_tx(5, 50_000, 40),  // density 40 - should replace tx4
        ];
        
        let mut values = Vec::new();
        
        for tx in stream {
            builder.process_tx(tx);
            values.push(builder.candidate().unwrap().total_value);
        }
        
        // Value should generally increase
        assert!(values.last() > values.first());
        
        // Final block should have best txs
        let candidate = builder.candidate().unwrap();
        let densities: Vec<_> = candidate.transactions.iter()
            .map(|t| t.value_density() as u64)
            .collect();
        
        // Should have the two highest: 30 and 40
        assert!(densities.contains(&30));
        assert!(densities.contains(&40));
    }
}