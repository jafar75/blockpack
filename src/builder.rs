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

/// Block builder using greedy algorithm
#[derive(Debug)]
pub struct Builder {
    /// Gas limit for blocks
    block_gas_limit: u64,
    
    /// Current block number
    next_block_number: u64,
}

impl Builder {
    /// Create a new builder
    pub fn new(block_gas_limit: u64) -> Self {
        Self {
            block_gas_limit,
            next_block_number: 1,
        }
    }
    
    /// Create a new builder starting at a specific block number
    pub fn with_start_block(block_gas_limit: u64, start_block: u64) -> Self {
        Self {
            block_gas_limit,
            next_block_number: start_block,
        }
    }
    
    /// Build a block from the pool using greedy by density
    /// 
    /// Algorithm:
    /// 1. Sort all pool transactions by value density (descending)
    /// 2. Iterate through sorted list
    /// 3. Add each tx if it fits, skip if it doesn't
    /// 4. Stop when we've considered all transactions
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
        
        BuildResult {
            block,
            included,
            considered,
            skipped,
        }
    }
    
    /// Build a block using greedy, then try to fill gaps
    /// 
    /// After greedy pass, some space may remain that couldn't fit
    /// large transactions. This does a second pass looking for
    /// smaller transactions that fit in the remaining space.
    pub fn build_greedy_with_backfill(&mut self, pool: &Pool) -> BuildResult {
        let mut block = Block::new(self.next_block_number, self.block_gas_limit);
        let mut included = Vec::new();
        let mut skipped_txs: Vec<&Tx> = Vec::new();
        
        let sorted = pool.sorted_by_density();
        let considered = sorted.len();
        
        // First pass: greedy by density
        for tx in sorted {
            if block.can_fit(tx) {
                included.push(tx.id);
                block.add_tx(tx.clone());
            } else {
                skipped_txs.push(tx);
            }
        }
        
        // Second pass: try to fit skipped txs by size (smallest first)
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
        
        BuildResult {
            block,
            included,
            considered,
            skipped: final_skipped,
        }
    }
    
    /// Finalize the current block and advance to next
    /// Call this after you're done with a build result
    pub fn seal_block(&mut self) {
        self.next_block_number += 1;
    }
    
    /// Get the next block number that will be built
    pub fn next_block_number(&self) -> u64 {
        self.next_block_number
    }
    
    /// Get the gas limit used for blocks
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
        pool.insert(make_tx(1, 21_000, 50));    // density 50
        pool.insert(make_tx(2, 100_000, 30));   // density 30
        pool.insert(make_tx(3, 250_000, 10));   // density 10
        pool.insert(make_tx(4, 21_000, 100));   // density 100
        pool.insert(make_tx(5, 500_000, 5));    // density 5
        pool
    }
    
    #[test]
    fn test_builder_new() {
        let builder = Builder::new(30_000_000);
        assert_eq!(builder.next_block_number(), 1);
        assert_eq!(builder.gas_limit(), 30_000_000);
    }
    
    #[test]
    fn test_greedy_build_all_fit() {
        let pool = setup_pool();
        let mut builder = Builder::new(30_000_000); // Plenty of space
        
        let result = builder.build_greedy(&pool);
        
        assert_eq!(result.considered, 5);
        assert_eq!(result.included.len(), 5);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.block.tx_count(), 5);
        
        // Check ordering: should be by density (100, 50, 30, 10, 5)
        assert_eq!(result.included[0], 4); // density 100
        assert_eq!(result.included[1], 1); // density 50
        assert_eq!(result.included[2], 2); // density 30
        assert_eq!(result.included[3], 3); // density 10
        assert_eq!(result.included[4], 5); // density 5
    }
    
    #[test]
    fn test_greedy_build_limited_space() {
        let pool = setup_pool();
        let mut builder = Builder::new(200_000); // Limited space
        
        let result = builder.build_greedy(&pool);
        
        // Should fit: tx4 (21k) + tx1 (21k) + tx2 (100k) = 142k
        // Skip: tx3 (250k) - too big
        // Skip: tx5 (500k) - too big
        assert_eq!(result.block.tx_count(), 3);
        assert!(result.included.contains(&4));
        assert!(result.included.contains(&1));
        assert!(result.included.contains(&2));
        assert_eq!(result.skipped, 2);
    }
    
    #[test]
    fn test_greedy_build_backfill() {
        let mut pool = Pool::new(100);
        // Create a scenario where backfill helps
        pool.insert(make_tx(1, 80_000, 100));   // density 100, large
        pool.insert(make_tx(2, 80_000, 90));    // density 90, large
        pool.insert(make_tx(3, 10_000, 50));    // density 50, small
        pool.insert(make_tx(4, 10_000, 40));    // density 40, small
        
        let mut builder = Builder::new(100_000);
        
        // Regular greedy: takes tx1 (80k), skips tx2 (would exceed)
        // then skips tx3, tx4 even though they fit
        let greedy_result = builder.build_greedy(&pool);
        
        // With backfill: takes tx1, then on second pass fits tx3 or tx4
        let mut builder2 = Builder::new(100_000);
        let backfill_result = builder2.build_greedy_with_backfill(&pool);
        
        // Backfill should include more transactions
        assert!(backfill_result.block.tx_count() >= greedy_result.block.tx_count());
        assert!(backfill_result.block.gas_used >= greedy_result.block.gas_used);
    }
    
    #[test]
    fn test_seal_block_advances_number() {
        let mut builder = Builder::new(30_000_000);
        assert_eq!(builder.next_block_number(), 1);
        
        builder.seal_block();
        assert_eq!(builder.next_block_number(), 2);
        
        builder.seal_block();
        assert_eq!(builder.next_block_number(), 3);
    }
    
    #[test]
    fn test_standalone_greedy_build() {
        let pool = setup_pool();
        let result = greedy_build(&pool, 42, 30_000_000);
        
        assert_eq!(result.block.number, 42);
        assert_eq!(result.included.len(), 5);
    }
    
    #[test]
    fn test_empty_pool() {
        let pool = Pool::new(100);
        let mut builder = Builder::new(30_000_000);
        
        let result = builder.build_greedy(&pool);
        
        assert_eq!(result.considered, 0);
        assert_eq!(result.included.len(), 0);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.block.tx_count(), 0);
    }
    
    #[test]
    fn test_block_value_maximization() {
        let mut pool = Pool::new(100);
        // High density but low total value
        pool.insert(make_tx(1, 10_000, 100)); // density 100, value 1M
        // Lower density but fits well
        pool.insert(make_tx(2, 50_000, 80));  // density 80, value 4M
        pool.insert(make_tx(3, 40_000, 60));  // density 60, value 2.4M
        
        let mut builder = Builder::new(100_000);
        let result = builder.build_greedy(&pool);
        
        // Greedy by density picks: tx1 (10k), tx2 (50k), tx3 (40k) = 100k total
        assert_eq!(result.block.gas_used, 100_000);
        assert_eq!(result.block.tx_count(), 3);
        
        // Total value: 1M + 4M + 2.4M = 7.4M
        assert_eq!(result.block.total_value, 7_400_000);
    }
}