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
    
    /// Transaction replaced multiple others (with count)
    ReplacedMultiple(usize),
    
    /// Transaction was rejected (not better than worst)
    Rejected,
    
    /// Transaction already exists in block
    Duplicate,
    
    /// Transaction can't afford current base fee
    InsufficientFee,
}

/// Statistics about incremental building
#[derive(Debug, Clone, Default)]
pub struct IncrementalStats {
    pub txs_received: u64,
    pub txs_added: u64,
    pub txs_replaced: u64,
    pub txs_rejected: u64,
    pub txs_duplicate: u64,
    pub txs_insufficient_fee: u64,
    pub multi_replacements: u64,
}

impl IncrementalStats {
    pub fn record(&mut self, result: &UpdateResult) {
        self.txs_received += 1;
        match result {
            UpdateResult::Added => self.txs_added += 1,
            UpdateResult::Replaced(_) => self.txs_replaced += 1,
            UpdateResult::ReplacedMultiple(n) => {
                self.txs_replaced += 1;
                self.multi_replacements += *n as u64;
            }
            UpdateResult::Rejected => self.txs_rejected += 1,
            UpdateResult::Duplicate => self.txs_duplicate += 1,
            UpdateResult::InsufficientFee => self.txs_insufficient_fee += 1,
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
    
    /// Current base fee (for EIP-1559 transactions)
    base_fee_gwei: u64,
}

impl Builder {
    /// Create a new builder
    pub fn new(block_gas_limit: u64) -> Self {
        Self {
            block_gas_limit,
            next_block_number: 1,
            candidate: None,
            stats: IncrementalStats::default(),
            base_fee_gwei: 0,
        }
    }
    
    /// Create a new builder starting at a specific block number
    pub fn with_start_block(block_gas_limit: u64, start_block: u64) -> Self {
        Self {
            block_gas_limit,
            next_block_number: start_block,
            candidate: None,
            stats: IncrementalStats::default(),
            base_fee_gwei: 0,
        }
    }
    
    /// Set the base fee for EIP-1559 calculations
    pub fn set_base_fee(&mut self, base_fee_gwei: u64) {
        self.base_fee_gwei = base_fee_gwei;
    }
    
    /// Get current base fee
    pub fn base_fee(&self) -> u64 {
        self.base_fee_gwei
    }
    
    /// Get or create the candidate block
    fn get_or_create_candidate(&mut self) -> &mut Block {
        if self.candidate.is_none() {
            self.candidate = Some(Block::new(self.next_block_number, self.block_gas_limit));
        }
        self.candidate.as_mut().unwrap()
    }
    
    /// Process a new transaction incrementally
    pub fn process_tx(&mut self, tx: Tx) -> UpdateResult {
        // Check if tx can afford base fee (EIP-1559)
        if !tx.can_afford_base_fee(self.base_fee_gwei) {
            let result = UpdateResult::InsufficientFee;
            self.stats.record(&result);
            return result;
        }
        
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
        let result = self.try_smart_replace(tx);
        self.stats.record(&result);
        result
    }
    
    /// Smart replacement: considers single and multi-tx swaps
    fn try_smart_replace(&mut self, new_tx: Tx) -> UpdateResult {
        let candidate = self.candidate.as_mut().unwrap();
        let base_fee = self.base_fee_gwei;
        
        // Get new tx effective value
        let new_tx_value = new_tx.value_at_base_fee(base_fee);
        let new_tx_density = new_tx.value_density_at_base_fee(base_fee);
        
        // Collect and sort current txs by density (worst first)
        let mut sorted: Vec<Tx> = candidate.transactions.iter().cloned().collect();
        sorted.sort_by(|a, b| {
            a.value_density_at_base_fee(base_fee)
                .partial_cmp(&b.value_density_at_base_fee(base_fee))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Find minimum set of worst txs to evict
        let mut to_evict: Vec<TxId> = Vec::new();
        let mut freed_gas = candidate.remaining_gas();
        let mut evicted_value = 0u64;
        
        for tx in &sorted {
            if freed_gas >= new_tx.gas_limit {
                break;
            }
            
            let tx_density = tx.value_density_at_base_fee(base_fee);
            
            // Only evict if new tx has better density
            if new_tx_density <= tx_density {
                break;
            }
            
            to_evict.push(tx.id);
            freed_gas += tx.gas_limit;
            evicted_value += tx.value_at_base_fee(base_fee);
        }
        
        // Check if we can fit
        if freed_gas < new_tx.gas_limit {
            return UpdateResult::Rejected;
        }
        
        // Must be net positive value
        if new_tx_value <= evicted_value {
            return UpdateResult::Rejected;
        }
        
        // Need at least one tx to evict
        if to_evict.is_empty() {
            return UpdateResult::Rejected;
        }
        
        // Perform the evictions
        let evict_count = to_evict.len();
        let first_evicted = to_evict[0];
        
        for id in &to_evict {
            candidate.remove_tx(*id);
        }
        
        // Verify we have space (sanity check)
        if !candidate.can_fit(&new_tx) {
            return UpdateResult::Rejected;
        }
        
        candidate.add_tx(new_tx);
        
        if evict_count > 1 {
            UpdateResult::ReplacedMultiple(evict_count)
        } else {
            UpdateResult::Replaced(first_evicted)
        }
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
    
    /// Finalize without returning block
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
        let result = builder.process_tx(make_tx(1, 21_000, 100));
        
        assert_eq!(result, UpdateResult::Duplicate);
        assert_eq!(builder.candidate().unwrap().tx_count(), 1);
    }
    
    #[test]
    fn test_process_tx_replace_better() {
        let mut builder = Builder::new(50_000);
        
        builder.process_tx(make_tx(1, 25_000, 10));
        builder.process_tx(make_tx(2, 25_000, 20));
        
        let result = builder.process_tx(make_tx(3, 25_000, 50));
        
        assert!(matches!(result, UpdateResult::Replaced(1)));
        
        let candidate = builder.candidate().unwrap();
        assert_eq!(candidate.tx_count(), 2);
        assert!(!candidate.transactions.iter().any(|t| t.id == 1));
        assert!(candidate.transactions.iter().any(|t| t.id == 3));
    }
    
    #[test]
    fn test_process_tx_reject_worse() {
        let mut builder = Builder::new(50_000);
        
        builder.process_tx(make_tx(1, 25_000, 50));
        builder.process_tx(make_tx(2, 25_000, 40));
        
        let result = builder.process_tx(make_tx(3, 25_000, 30));
        
        assert_eq!(result, UpdateResult::Rejected);
        assert_eq!(builder.candidate().unwrap().tx_count(), 2);
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
        
        assert_eq!(builder.next_block_number(), 2);
        assert!(builder.candidate().is_none());
    }
    
    #[test]
    fn test_incremental_stats() {
        let mut builder = Builder::new(50_000);
        
        builder.process_tx(make_tx(1, 20_000, 10));
        builder.process_tx(make_tx(2, 20_000, 20));
        builder.process_tx(make_tx(1, 20_000, 30)); // Duplicate
        builder.process_tx(make_tx(3, 20_000, 50)); // Replace
        builder.process_tx(make_tx(4, 20_000, 5));  // Rejected
        
        let stats = builder.stats();
        assert_eq!(stats.txs_received, 5);
        assert_eq!(stats.txs_added, 2);
        assert_eq!(stats.txs_duplicate, 1);
        assert_eq!(stats.txs_replaced, 1);
        assert_eq!(stats.txs_rejected, 1);
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
    fn test_eip1559_insufficient_fee() {
        let mut builder = Builder::new(100_000);
        builder.set_base_fee(50);
        
        // max_fee = 40, can't afford base_fee = 50
        let tx = Tx::new_eip1559(1, 21_000, 10, 40);
        let result = builder.process_tx(tx);
        
        assert_eq!(result, UpdateResult::InsufficientFee);
    }
    
    #[test]
    fn test_eip1559_effective_value() {
        let mut builder = Builder::new(100_000);
        builder.set_base_fee(30);
        
        // max_fee = 50, priority = 25, base = 30
        // effective = min(25, 50-30) = min(25, 20) = 20
        let tx = Tx::new_eip1559(1, 21_000, 25, 50);
        let result = builder.process_tx(tx);
        
        assert_eq!(result, UpdateResult::Added);
    }
    
    #[test]
    fn test_multi_replace_reports_count() {
        let mut builder = Builder::new(100_000);
        
        // Fill with small txs
        builder.process_tx(make_tx(1, 30_000, 10));
        builder.process_tx(make_tx(2, 30_000, 15));
        builder.process_tx(make_tx(3, 30_000, 20));
        
        // Add large tx that needs multi-eviction
        let result = builder.process_tx(make_tx(4, 70_000, 50));
        
        assert!(matches!(result, UpdateResult::ReplacedMultiple(_)));
    }
    
    #[test]
    fn test_block_never_exceeds_gas_limit() {
        let mut builder = Builder::new(200_000);
        
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
                "Block gas {} exceeded limit {}",
                candidate.gas_used, candidate.gas_limit
            );
        }
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
    fn test_base_fee_getter_setter() {
        let mut builder = Builder::new(100_000);
        
        assert_eq!(builder.base_fee(), 0);
        builder.set_base_fee(25);
        assert_eq!(builder.base_fee(), 25);
    }
}