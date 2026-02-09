//! Transaction conflict detection
//!
//! Detects transactions that touch the same state and cannot be included together
//! or must be ordered carefully.

use std::collections::{HashMap, HashSet};
use crate::tx::{TxId, Address};

/// Represents a storage slot that can be accessed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StorageSlot {
    pub contract: Address,
    pub slot: u64,
}

impl StorageSlot {
    pub fn new(contract: Address, slot: u64) -> Self {
        Self { contract, slot }
    }
}

/// Access pattern for a transaction
#[derive(Debug, Clone, Default)]
pub struct AccessList {
    /// Storage slots read by this transaction
    pub reads: HashSet<StorageSlot>,
    
    /// Storage slots written by this transaction
    pub writes: HashSet<StorageSlot>,
    
    /// Contracts called by this transaction
    pub contracts_touched: HashSet<Address>,
}

impl AccessList {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_read(&mut self, contract: Address, slot: u64) {
        self.reads.insert(StorageSlot::new(contract, slot));
        self.contracts_touched.insert(contract);
    }
    
    pub fn add_write(&mut self, contract: Address, slot: u64) {
        self.writes.insert(StorageSlot::new(contract, slot));
        self.contracts_touched.insert(contract);
    }
    
    pub fn add_contract(&mut self, contract: Address) {
        self.contracts_touched.insert(contract);
    }
    
    /// Check if this access list conflicts with another
    /// Conflict = one writes to a slot the other reads or writes
    pub fn conflicts_with(&self, other: &AccessList) -> bool {
        // Write-Write conflict
        if !self.writes.is_disjoint(&other.writes) {
            return true;
        }
        
        // Write-Read conflict (either direction)
        if !self.writes.is_disjoint(&other.reads) {
            return true;
        }
        if !self.reads.is_disjoint(&other.writes) {
            return true;
        }
        
        false
    }
    
    /// Check if this tx must come before another (dependency)
    /// True if other reads something this tx writes
    pub fn must_precede(&self, other: &AccessList) -> bool {
        !self.writes.is_disjoint(&other.reads)
    }
}

/// Type of conflict between two transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictType {
    /// Both write to the same slot
    WriteWrite,
    
    /// One writes, the other reads the same slot
    WriteRead,
    
    /// No conflict
    None,
}

/// Tracks conflicts between transactions
#[derive(Debug, Default)]
pub struct ConflictTracker {
    /// Access lists per transaction
    access_lists: HashMap<TxId, AccessList>,
    
    /// Cached conflict pairs
    conflicts: HashSet<(TxId, TxId)>,
}

impl ConflictTracker {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a transaction's access list
    pub fn register(&mut self, tx_id: TxId, access_list: AccessList) {
        // Check conflicts with existing txs
        for (&other_id, other_access) in &self.access_lists {
            if access_list.conflicts_with(other_access) {
                let pair = if tx_id < other_id {
                    (tx_id, other_id)
                } else {
                    (other_id, tx_id)
                };
                self.conflicts.insert(pair);
            }
        }
        
        self.access_lists.insert(tx_id, access_list);
    }
    
    /// Remove a transaction
    pub fn remove(&mut self, tx_id: TxId) {
        self.access_lists.remove(&tx_id);
        self.conflicts.retain(|&(a, b)| a != tx_id && b != tx_id);
    }
    
    /// Check if two transactions conflict
    pub fn conflicts(&self, tx1: TxId, tx2: TxId) -> bool {
        let pair = if tx1 < tx2 { (tx1, tx2) } else { (tx2, tx1) };
        self.conflicts.contains(&pair)
    }
    
    /// Get all transactions that conflict with a given transaction
    pub fn get_conflicts(&self, tx_id: TxId) -> Vec<TxId> {
        self.conflicts
            .iter()
            .filter_map(|&(a, b)| {
                if a == tx_id {
                    Some(b)
                } else if b == tx_id {
                    Some(a)
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get conflict type between two transactions
    pub fn conflict_type(&self, tx1: TxId, tx2: TxId) -> ConflictType {
        let access1 = match self.access_lists.get(&tx1) {
            Some(a) => a,
            None => return ConflictType::None,
        };
        let access2 = match self.access_lists.get(&tx2) {
            Some(a) => a,
            None => return ConflictType::None,
        };
        
        if !access1.writes.is_disjoint(&access2.writes) {
            ConflictType::WriteWrite
        } else if !access1.writes.is_disjoint(&access2.reads) 
               || !access1.reads.is_disjoint(&access2.writes) {
            ConflictType::WriteRead
        } else {
            ConflictType::None
        }
    }
    
    /// Find a non-conflicting subset of transactions
    /// Uses greedy selection by transaction ID order
    pub fn find_non_conflicting(&self, tx_ids: &[TxId]) -> Vec<TxId> {
        let mut selected = Vec::new();
        let mut excluded: HashSet<TxId> = HashSet::new();
        
        for &tx_id in tx_ids {
            if excluded.contains(&tx_id) {
                continue;
            }
            
            selected.push(tx_id);
            
            // Exclude all txs that conflict with this one
            for conflicting in self.get_conflicts(tx_id) {
                excluded.insert(conflicting);
            }
        }
        
        selected
    }
    
    /// Get number of tracked transactions
    pub fn len(&self) -> usize {
        self.access_lists.len()
    }
    
    /// Check if tracker is empty
    pub fn is_empty(&self) -> bool {
        self.access_lists.is_empty()
    }
    
    /// Get total number of conflict pairs
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }
    
    /// Clear all tracked data
    pub fn clear(&mut self) {
        self.access_lists.clear();
        self.conflicts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_access_list_no_conflict() {
        let mut a1 = AccessList::new();
        a1.add_read(100, 1);
        a1.add_write(100, 2);
        
        let mut a2 = AccessList::new();
        a2.add_read(100, 3);
        a2.add_write(100, 4);
        
        assert!(!a1.conflicts_with(&a2));
    }
    
    #[test]
    fn test_access_list_write_write_conflict() {
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_write(100, 1);
        
        assert!(a1.conflicts_with(&a2));
    }
    
    #[test]
    fn test_access_list_write_read_conflict() {
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_read(100, 1);
        
        assert!(a1.conflicts_with(&a2));
        assert!(a2.conflicts_with(&a1));
    }
    
    #[test]
    fn test_access_list_read_read_no_conflict() {
        let mut a1 = AccessList::new();
        a1.add_read(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_read(100, 1);
        
        assert!(!a1.conflicts_with(&a2));
    }
    
    #[test]
    fn test_must_precede() {
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_read(100, 1);
        
        assert!(a1.must_precede(&a2));
        assert!(!a2.must_precede(&a1));
    }
    
    #[test]
    fn test_conflict_tracker_register() {
        let mut tracker = ConflictTracker::new();
        
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_read(100, 1);
        
        let mut a3 = AccessList::new();
        a3.add_read(100, 2);
        
        tracker.register(1, a1);
        tracker.register(2, a2);
        tracker.register(3, a3);
        
        assert!(tracker.conflicts(1, 2));
        assert!(!tracker.conflicts(1, 3));
        assert!(!tracker.conflicts(2, 3));
        
        assert_eq!(tracker.len(), 3);
        assert_eq!(tracker.conflict_count(), 1);
    }
    
    #[test]
    fn test_conflict_tracker_remove() {
        let mut tracker = ConflictTracker::new();
        
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_read(100, 1);
        
        tracker.register(1, a1);
        tracker.register(2, a2);
        
        assert!(tracker.conflicts(1, 2));
        
        tracker.remove(1);
        
        assert!(!tracker.conflicts(1, 2));
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.conflict_count(), 0);
    }
    
    #[test]
    fn test_get_conflicts() {
        let mut tracker = ConflictTracker::new();
        
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_read(100, 1);
        
        let mut a3 = AccessList::new();
        a3.add_write(100, 1);
        
        tracker.register(1, a1);
        tracker.register(2, a2);
        tracker.register(3, a3);
        
        let conflicts = tracker.get_conflicts(1);
        assert_eq!(conflicts.len(), 2);
        assert!(conflicts.contains(&2));
        assert!(conflicts.contains(&3));
    }
    
    #[test]
    fn test_conflict_type() {
        let mut tracker = ConflictTracker::new();
        
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_write(100, 1);
        
        let mut a3 = AccessList::new();
        a3.add_read(100, 1);
        
        tracker.register(1, a1);
        tracker.register(2, a2);
        tracker.register(3, a3);
        
        assert_eq!(tracker.conflict_type(1, 2), ConflictType::WriteWrite);
        assert_eq!(tracker.conflict_type(1, 3), ConflictType::WriteRead);
        assert_eq!(tracker.conflict_type(2, 3), ConflictType::WriteRead);
    }
    
    #[test]
    fn test_find_non_conflicting() {
        let mut tracker = ConflictTracker::new();
        
        // TX 1 and TX 2 conflict
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        
        let mut a2 = AccessList::new();
        a2.add_write(100, 1);
        
        // TX 3 doesn't conflict with anyone
        let mut a3 = AccessList::new();
        a3.add_write(100, 2);
        
        tracker.register(1, a1);
        tracker.register(2, a2);
        tracker.register(3, a3);
        
        let non_conflicting = tracker.find_non_conflicting(&[1, 2, 3]);
        
        // Should include TX 1 (first) and TX 3, but not TX 2 (conflicts with 1)
        assert_eq!(non_conflicting.len(), 2);
        assert!(non_conflicting.contains(&1));
        assert!(non_conflicting.contains(&3));
        assert!(!non_conflicting.contains(&2));
    }
    
    #[test]
    fn test_clear() {
        let mut tracker = ConflictTracker::new();
        
        let mut a1 = AccessList::new();
        a1.add_write(100, 1);
        tracker.register(1, a1);
        
        assert_eq!(tracker.len(), 1);
        
        tracker.clear();
        
        assert!(tracker.is_empty());
        assert_eq!(tracker.conflict_count(), 0);
    }
}