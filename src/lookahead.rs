//! Multi-block lookahead planning
//!
//! Optimizes transaction placement across multiple future blocks
//! rather than just the immediate next block.

use std::collections::HashMap;
use crate::tx::{Tx, TxId};

/// A planned block in the lookahead window
#[derive(Debug, Clone)]
pub struct PlannedBlock {
    /// Block number
    pub number: u64,
    
    /// Gas limit for this block
    pub gas_limit: u64,
    
    /// Transactions planned for this block
    pub transactions: Vec<Tx>,
    
    /// Current gas used
    pub gas_used: u64,
    
    /// Total value of planned transactions
    pub total_value: u64,
}

impl PlannedBlock {
    pub fn new(number: u64, gas_limit: u64) -> Self {
        Self {
            number,
            gas_limit,
            transactions: Vec::new(),
            gas_used: 0,
            total_value: 0,
        }
    }
    
    /// Remaining gas capacity
    pub fn remaining_gas(&self) -> u64 {
        self.gas_limit.saturating_sub(self.gas_used)
    }
    
    /// Check if a transaction fits
    pub fn can_fit(&self, tx: &Tx) -> bool {
        tx.gas_limit <= self.remaining_gas()
    }
    
    /// Add a transaction to the plan
    pub fn add_tx(&mut self, tx: Tx) -> bool {
        if !self.can_fit(&tx) {
            return false;
        }
        
        self.gas_used += tx.gas_limit;
        self.total_value += tx.value();
        self.transactions.push(tx);
        true
    }
    
    /// Remove a transaction from the plan
    pub fn remove_tx(&mut self, tx_id: TxId) -> Option<Tx> {
        if let Some(pos) = self.transactions.iter().position(|t| t.id == tx_id) {
            let tx = self.transactions.remove(pos);
            self.gas_used -= tx.gas_limit;
            self.total_value -= tx.value();
            Some(tx)
        } else {
            None
        }
    }
    
    /// Block fullness as percentage
    pub fn fullness(&self) -> f64 {
        if self.gas_limit == 0 {
            return 0.0;
        }
        (self.gas_used as f64 / self.gas_limit as f64) * 100.0
    }
    
    /// Number of transactions
    pub fn tx_count(&self) -> usize {
        self.transactions.len()
    }
    
    /// Check if block has a specific transaction
    pub fn contains(&self, tx_id: TxId) -> bool {
        self.transactions.iter().any(|t| t.id == tx_id)
    }
    
    /// Clear all transactions
    pub fn clear(&mut self) {
        self.transactions.clear();
        self.gas_used = 0;
        self.total_value = 0;
    }
}

/// Priority level for transaction scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TxPriority {
    /// Must be in immediate next block
    Urgent,
    
    /// Should be included soon (within 2-3 blocks)
    High,
    
    /// Normal priority, can wait
    Normal,
    
    /// Low priority, include when convenient
    Low,
}

impl Default for TxPriority {
    fn default() -> Self {
        TxPriority::Normal
    }
}

/// Transaction with scheduling metadata
#[derive(Debug, Clone)]
pub struct ScheduledTx {
    pub tx: Tx,
    pub priority: TxPriority,
    pub deadline_block: Option<u64>,
}

impl ScheduledTx {
    pub fn new(tx: Tx) -> Self {
        Self {
            tx,
            priority: TxPriority::Normal,
            deadline_block: None,
        }
    }
    
    pub fn with_priority(tx: Tx, priority: TxPriority) -> Self {
        Self {
            tx,
            priority,
            deadline_block: None,
        }
    }
    
    pub fn with_deadline(tx: Tx, deadline_block: u64) -> Self {
        Self {
            tx,
            priority: TxPriority::Normal,
            deadline_block: Some(deadline_block),
        }
    }
    
    pub fn urgent(tx: Tx) -> Self {
        Self {
            tx,
            priority: TxPriority::Urgent,
            deadline_block: None,
        }
    }
    
    /// Check if transaction has expired (past deadline)
    pub fn is_expired(&self, current_block: u64) -> bool {
        self.deadline_block.map(|d| current_block > d).unwrap_or(false)
    }
    
    /// Check if transaction is due (at or past deadline)
    pub fn is_due(&self, current_block: u64) -> bool {
        self.deadline_block.map(|d| current_block >= d).unwrap_or(false)
    }
}

/// Multi-block lookahead planner
#[derive(Debug)]
pub struct LookaheadPlanner {
    /// Number of blocks to plan ahead
    lookahead_depth: usize,
    
    /// Gas limit per block
    block_gas_limit: u64,
    
    /// Starting block number
    start_block: u64,
    
    /// Planned blocks
    planned_blocks: Vec<PlannedBlock>,
    
    /// Unscheduled transactions
    pending: Vec<ScheduledTx>,
    
    /// Track which tx is in which block
    tx_assignments: HashMap<TxId, u64>,
}

impl LookaheadPlanner {
    /// Create a new planner
    pub fn new(start_block: u64, block_gas_limit: u64, lookahead_depth: usize) -> Self {
        let planned_blocks = (0..lookahead_depth)
            .map(|i| PlannedBlock::new(start_block + i as u64, block_gas_limit))
            .collect();
        
        Self {
            lookahead_depth,
            block_gas_limit,
            start_block,
            planned_blocks,
            pending: Vec::new(),
            tx_assignments: HashMap::new(),
        }
    }
    
    /// Add a transaction to be scheduled
    pub fn add_tx(&mut self, scheduled_tx: ScheduledTx) {
        self.pending.push(scheduled_tx);
    }
    
    /// Run the planning algorithm
    /// Assigns transactions to blocks based on priority and fit
    pub fn plan(&mut self) {
        // Sort pending by priority (urgent first) then by value density
        self.pending.sort_by(|a, b| {
            match a.priority.cmp(&b.priority) {
                std::cmp::Ordering::Equal => {
                    b.tx.value_density()
                        .partial_cmp(&a.tx.value_density())
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                other => other,
            }
        });
        
        // Clear existing assignments
        for block in &mut self.planned_blocks {
            block.clear();
        }
        self.tx_assignments.clear();
        
        // Take pending out to avoid borrow issues
        let pending = std::mem::take(&mut self.pending);
        let mut remaining = Vec::new();
        
        // Schedule transactions
        for scheduled_tx in pending {
            if !self.schedule_tx(scheduled_tx.clone()) {
                remaining.push(scheduled_tx);
            }
        }
        
        self.pending = remaining;
    }
    
    /// Try to schedule a single transaction
    fn schedule_tx(&mut self, scheduled_tx: ScheduledTx) -> bool {
        let tx = &scheduled_tx.tx;
        
        // Determine which blocks are valid for this tx
        let valid_blocks: Vec<usize> = match scheduled_tx.priority {
            TxPriority::Urgent => vec![0], // First block only
            TxPriority::High => (0..2.min(self.lookahead_depth)).collect(),
            TxPriority::Normal => (0..self.lookahead_depth).collect(),
            TxPriority::Low => (0..self.lookahead_depth).collect(),
        };
        
        // Consider deadline
        let valid_blocks: Vec<usize> = if let Some(deadline) = scheduled_tx.deadline_block {
            valid_blocks.into_iter()
                .filter(|&i| {
                    let block_num = self.start_block + i as u64;
                    block_num <= deadline
                })
                .collect()
        } else {
            valid_blocks
        };
        
        // Try to fit in first available block
        for &block_idx in &valid_blocks {
            if self.planned_blocks[block_idx].can_fit(tx) {
                let block_num = self.planned_blocks[block_idx].number;
                self.planned_blocks[block_idx].add_tx(tx.clone());
                self.tx_assignments.insert(tx.id, block_num);
                return true;
            }
        }
        
        // Try to bump lower priority tx if this is urgent/high
        if scheduled_tx.priority <= TxPriority::High {
            for &block_idx in &valid_blocks {
                if let Some(bumped_id) = self.try_bump(block_idx, tx) {
                    let block_num = self.planned_blocks[block_idx].number;
                    self.planned_blocks[block_idx].add_tx(tx.clone());
                    self.tx_assignments.insert(tx.id, block_num);
                    self.tx_assignments.remove(&bumped_id);
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Try to bump a lower-value tx to make room
    /// Returns the ID of the bumped tx if successful
    fn try_bump(&mut self, block_idx: usize, new_tx: &Tx) -> Option<TxId> {
        let block = &mut self.planned_blocks[block_idx];
        
        // Find worst tx that would free enough space
        let mut candidates: Vec<(usize, f64, u64)> = block.transactions
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                // Would freeing this tx allow new_tx to fit?
                block.remaining_gas() + t.gas_limit >= new_tx.gas_limit
            })
            .filter(|(_, t)| {
                // New tx must be better value density
                new_tx.value_density() > t.value_density()
            })
            .map(|(i, t)| (i, t.value_density(), t.id))
            .collect();
        
        // Sort by density (worst first)
        candidates.sort_by(|a, b| {
            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Bump the worst candidate
        if let Some((_, _, bumped_id)) = candidates.first() {
            block.remove_tx(*bumped_id);
            Some(*bumped_id)
        } else {
            None
        }
    }
    
    /// Get the plan for a specific block
    pub fn get_block_plan(&self, block_number: u64) -> Option<&PlannedBlock> {
        let idx = block_number.checked_sub(self.start_block)? as usize;
        self.planned_blocks.get(idx)
    }
    
    /// Get the next block plan (first in window)
    pub fn get_next_block(&self) -> &PlannedBlock {
        &self.planned_blocks[0]
    }
    
    /// Advance the window by one block
    /// Removes the first block and adds a new one at the end
    pub fn advance(&mut self) -> PlannedBlock {
        let completed = self.planned_blocks.remove(0);
        self.start_block += 1;
        
        // Add new block at the end
        let new_block_num = self.start_block + self.lookahead_depth as u64 - 1;
        self.planned_blocks.push(PlannedBlock::new(new_block_num, self.block_gas_limit));
        
        // Remove assignments for completed block
        self.tx_assignments.retain(|_, &mut block| block >= self.start_block);
        
        completed
    }
    
    /// Get total value across all planned blocks
    pub fn total_planned_value(&self) -> u64 {
        self.planned_blocks.iter().map(|b| b.total_value).sum()
    }
    
    /// Get total transactions across all planned blocks
    pub fn total_planned_txs(&self) -> usize {
        self.planned_blocks.iter().map(|b| b.tx_count()).sum()
    }
    
    /// Get number of pending (unscheduled) transactions
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    
    /// Get the lookahead depth
    pub fn depth(&self) -> usize {
        self.lookahead_depth
    }
    
    /// Remove expired transactions from pending
    pub fn remove_expired(&mut self, current_block: u64) -> usize {
        let before = self.pending.len();
        self.pending.retain(|stx| !stx.is_expired(current_block));
        before - self.pending.len()
    }
    
    /// Get statistics about the plan
    pub fn stats(&self) -> LookaheadStats {
        LookaheadStats {
            lookahead_depth: self.lookahead_depth,
            start_block: self.start_block,
            total_planned_txs: self.total_planned_txs(),
            total_planned_value: self.total_planned_value(),
            pending_count: self.pending.len(),
            block_stats: self.planned_blocks.iter()
                .map(|b| BlockPlanStats {
                    block_number: b.number,
                    tx_count: b.tx_count(),
                    gas_used: b.gas_used,
                    gas_limit: b.gas_limit,
                    fullness: b.fullness(),
                    total_value: b.total_value,
                })
                .collect(),
        }
    }
}

/// Statistics for a planned block
#[derive(Debug, Clone)]
pub struct BlockPlanStats {
    pub block_number: u64,
    pub tx_count: usize,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub fullness: f64,
    pub total_value: u64,
}

/// Overall lookahead statistics
#[derive(Debug, Clone)]
pub struct LookaheadStats {
    pub lookahead_depth: usize,
    pub start_block: u64,
    pub total_planned_txs: usize,
    pub total_planned_value: u64,
    pub pending_count: usize,
    pub block_stats: Vec<BlockPlanStats>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_tx(id: TxId, gas: u64, fee: u64) -> Tx {
        Tx::new(id, gas, fee)
    }
    
    #[test]
    fn test_planned_block() {
        let mut block = PlannedBlock::new(1, 100_000);
        
        assert!(block.add_tx(make_tx(1, 30_000, 10)));
        assert!(block.add_tx(make_tx(2, 30_000, 20)));
        
        assert_eq!(block.tx_count(), 2);
        assert_eq!(block.gas_used, 60_000);
        assert_eq!(block.remaining_gas(), 40_000);
        assert!((block.fullness() - 60.0).abs() < 0.1);
    }
    
    #[test]
    fn test_planned_block_remove() {
        let mut block = PlannedBlock::new(1, 100_000);
        
        block.add_tx(make_tx(1, 30_000, 10));
        block.add_tx(make_tx(2, 30_000, 20));
        
        let removed = block.remove_tx(1);
        assert!(removed.is_some());
        assert_eq!(block.tx_count(), 1);
        assert_eq!(block.gas_used, 30_000);
    }
    
    #[test]
    fn test_scheduled_tx_priority() {
        let tx = make_tx(1, 21_000, 10);
        
        let normal = ScheduledTx::new(tx.clone());
        assert_eq!(normal.priority, TxPriority::Normal);
        
        let urgent = ScheduledTx::urgent(tx.clone());
        assert_eq!(urgent.priority, TxPriority::Urgent);
        
        let high = ScheduledTx::with_priority(tx.clone(), TxPriority::High);
        assert_eq!(high.priority, TxPriority::High);
    }
    
    #[test]
    fn test_scheduled_tx_deadline() {
        let tx = make_tx(1, 21_000, 10);
        let stx = ScheduledTx::with_deadline(tx, 100);
        
        assert!(!stx.is_expired(99));
        assert!(!stx.is_expired(100));
        assert!(stx.is_expired(101));
        
        assert!(!stx.is_due(99));
        assert!(stx.is_due(100));
        assert!(stx.is_due(101));
    }
    
    #[test]
    fn test_lookahead_planner_basic() {
        let mut planner = LookaheadPlanner::new(1, 100_000, 3);
        
        planner.add_tx(ScheduledTx::new(make_tx(1, 30_000, 10)));
        planner.add_tx(ScheduledTx::new(make_tx(2, 30_000, 20)));
        planner.add_tx(ScheduledTx::new(make_tx(3, 30_000, 30)));
        
        planner.plan();
        
        assert_eq!(planner.total_planned_txs(), 3);
        assert_eq!(planner.pending_count(), 0);
    }
    
    #[test]
    fn test_lookahead_planner_urgent() {
        let mut planner = LookaheadPlanner::new(1, 50_000, 3);
        
        // Add normal txs that fill first block
        planner.add_tx(ScheduledTx::new(make_tx(1, 30_000, 10)));
        planner.add_tx(ScheduledTx::new(make_tx(2, 30_000, 10)));
        
        // Add urgent tx - should bump one of the normal txs
        planner.add_tx(ScheduledTx::urgent(make_tx(3, 30_000, 50)));
        
        planner.plan();
        
        let first_block = planner.get_next_block();
        assert!(first_block.contains(3)); // Urgent tx should be in first block
    }
    
    #[test]
    fn test_lookahead_planner_overflow_to_next() {
        let mut planner = LookaheadPlanner::new(1, 50_000, 3);
        
        // Add more txs than fit in one block
        planner.add_tx(ScheduledTx::new(make_tx(1, 30_000, 30)));
        planner.add_tx(ScheduledTx::new(make_tx(2, 30_000, 20)));
        planner.add_tx(ScheduledTx::new(make_tx(3, 30_000, 10)));
        
        planner.plan();
        
        // First block should have tx 1 (highest value density)
        let block1 = planner.get_block_plan(1).unwrap();
        assert!(block1.contains(1));
        
        // Remaining should spill to block 2
        let block2 = planner.get_block_plan(2).unwrap();
        assert!(block2.tx_count() > 0);
    }
    
    #[test]
    fn test_lookahead_planner_advance() {
        let mut planner = LookaheadPlanner::new(1, 100_000, 3);
        
        planner.add_tx(ScheduledTx::new(make_tx(1, 30_000, 10)));
        planner.plan();
        
        assert_eq!(planner.get_next_block().number, 1);
        
        let completed = planner.advance();
        assert_eq!(completed.number, 1);
        assert_eq!(planner.get_next_block().number, 2);
        
        // Should now have blocks 2, 3, 4
        assert!(planner.get_block_plan(2).is_some());
        assert!(planner.get_block_plan(3).is_some());
        assert!(planner.get_block_plan(4).is_some());
        assert!(planner.get_block_plan(1).is_none());
    }
    
    #[test]
    fn test_lookahead_planner_deadline() {
        let mut planner = LookaheadPlanner::new(1, 100_000, 5);
        
        // Tx with deadline at block 2 - give it high value so it gets scheduled
        planner.add_tx(ScheduledTx::with_deadline(make_tx(1, 30_000, 50), 2));
        
        // Add some other txs
        planner.add_tx(ScheduledTx::new(make_tx(2, 40_000, 20)));
        planner.add_tx(ScheduledTx::new(make_tx(3, 40_000, 10)));
        
        planner.plan();
        
        // Tx 1 should be in block 1 or 2 (before or at deadline)
        let in_block_1 = planner.get_block_plan(1).unwrap().contains(1);
        let in_block_2 = planner.get_block_plan(2).unwrap().contains(1);
        assert!(in_block_1 || in_block_2, "Tx with deadline should be in block 1 or 2");
    }
    
    #[test]
    fn test_lookahead_stats() {
        let mut planner = LookaheadPlanner::new(1, 100_000, 3);
        
        planner.add_tx(ScheduledTx::new(make_tx(1, 30_000, 10)));
        planner.add_tx(ScheduledTx::new(make_tx(2, 30_000, 20)));
        planner.plan();
        
        let stats = planner.stats();
        
        assert_eq!(stats.lookahead_depth, 3);
        assert_eq!(stats.start_block, 1);
        assert_eq!(stats.total_planned_txs, 2);
        assert_eq!(stats.block_stats.len(), 3);
    }
    
    #[test]
    fn test_remove_expired() {
        let mut planner = LookaheadPlanner::new(10, 100_000, 3);
        
        planner.add_tx(ScheduledTx::with_deadline(make_tx(1, 30_000, 10), 5)); // Expired
        planner.add_tx(ScheduledTx::with_deadline(make_tx(2, 30_000, 20), 15)); // Valid
        planner.add_tx(ScheduledTx::new(make_tx(3, 30_000, 30))); // No deadline
        
        let removed = planner.remove_expired(10);
        
        assert_eq!(removed, 1);
        assert_eq!(planner.pending_count(), 2);
    }
}