//! Nonce tracking and transaction ordering
//!
//! Ensures transactions from the same sender are included in correct nonce order.

use std::collections::{HashMap, BTreeMap};
use crate::tx::{Tx, TxId, Address};

/// Tracks nonces for accounts and manages transaction dependencies
#[derive(Debug, Default)]
pub struct NonceTracker {
    /// Current confirmed nonce for each address (from chain state)
    confirmed_nonces: HashMap<Address, u64>,
    
    /// Pending transactions per address, keyed by nonce
    pending: HashMap<Address, BTreeMap<u64, Tx>>,
}

impl NonceTracker {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set the confirmed nonce for an address (from chain state)
    pub fn set_confirmed_nonce(&mut self, address: Address, nonce: u64) {
        self.confirmed_nonces.insert(address, nonce);
        
        // Prune any pending txs with nonce < confirmed
        if let Some(pending) = self.pending.get_mut(&address) {
            pending.retain(|&tx_nonce, _| tx_nonce >= nonce);
        }
    }
    
    /// Get the confirmed nonce for an address
    pub fn confirmed_nonce(&self, address: Address) -> u64 {
        *self.confirmed_nonces.get(&address).unwrap_or(&0)
    }
    
    /// Get the next expected nonce for an address
    pub fn next_nonce(&self, address: Address) -> u64 {
        self.confirmed_nonces.get(&address).copied().unwrap_or(0)
    }
    
    /// Add a transaction to pending
    /// Returns Ok if added, Err with reason if rejected
    pub fn add_tx(&mut self, tx: Tx) -> Result<(), NonceError> {
        let confirmed = self.confirmed_nonce(tx.sender);
        
        // Reject if nonce is too old
        if tx.nonce < confirmed {
            return Err(NonceError::NonceTooLow {
                got: tx.nonce,
                expected: confirmed,
            });
        }
        
        let pending = self.pending.entry(tx.sender).or_default();
        
        // Check if this nonce already has a tx
        if let Some(existing) = pending.get(&tx.nonce) {
            // Allow replacement if new tx has higher fee
            if tx.priority_fee_gwei <= existing.priority_fee_gwei {
                return Err(NonceError::ReplacementUnderpriced {
                    existing_fee: existing.priority_fee_gwei,
                    new_fee: tx.priority_fee_gwei,
                });
            }
        }
        
        pending.insert(tx.nonce, tx);
        Ok(())
    }
    
    /// Remove a transaction
    pub fn remove_tx(&mut self, sender: Address, nonce: u64) -> Option<Tx> {
        self.pending.get_mut(&sender)?.remove(&nonce)
    }
    
    /// Get executable transactions (those with no gaps in nonce sequence)
    pub fn get_executable(&self, address: Address) -> Vec<&Tx> {
        let confirmed = self.confirmed_nonce(address);
        let mut result = Vec::new();
        
        if let Some(pending) = self.pending.get(&address) {
            let mut expected_nonce = confirmed;
            
            for (&nonce, tx) in pending.iter() {
                if nonce == expected_nonce {
                    result.push(tx);
                    expected_nonce += 1;
                } else {
                    // Gap in nonces, stop
                    break;
                }
            }
        }
        
        result
    }
    
    /// Get all executable transactions across all addresses
    pub fn get_all_executable(&self) -> Vec<&Tx> {
        let mut result = Vec::new();
        
        for &address in self.pending.keys() {
            result.extend(self.get_executable(address));
        }
        
        result
    }
    
    /// Get all executable transactions sorted by value density
    pub fn get_executable_sorted(&self) -> Vec<&Tx> {
        let mut txs = self.get_all_executable();
        txs.sort_by(|a, b| {
            b.value_density()
                .partial_cmp(&a.value_density())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        txs
    }
    
    /// Check if a transaction is executable (no nonce gaps before it)
    pub fn is_executable(&self, tx: &Tx) -> bool {
        let confirmed = self.confirmed_nonce(tx.sender);
        
        if tx.nonce < confirmed {
            return false; // Already executed
        }
        
        if tx.nonce == confirmed {
            return true; // Next expected nonce
        }
        
        // Check for gaps
        if let Some(pending) = self.pending.get(&tx.sender) {
            for nonce in confirmed..tx.nonce {
                if !pending.contains_key(&nonce) {
                    return false; // Gap found
                }
            }
            true
        } else {
            false
        }
    }
    
    /// Mark a transaction as included (update confirmed nonce)
    pub fn mark_included(&mut self, tx: &Tx) {
        let new_nonce = tx.nonce + 1;
        let current = self.confirmed_nonce(tx.sender);
        
        if new_nonce > current {
            self.set_confirmed_nonce(tx.sender, new_nonce);
        }
        
        // Remove the included tx from pending
        self.remove_tx(tx.sender, tx.nonce);
    }
    
    /// Get pending transaction count for an address
    pub fn pending_count(&self, address: Address) -> usize {
        self.pending.get(&address).map(|p| p.len()).unwrap_or(0)
    }
    
    /// Get total pending transaction count
    pub fn total_pending(&self) -> usize {
        self.pending.values().map(|p| p.len()).sum()
    }
    
    /// Clear all pending transactions for an address
    pub fn clear_address(&mut self, address: Address) {
        self.pending.remove(&address);
    }
    
    /// Clear all pending transactions
    pub fn clear(&mut self) {
        self.pending.clear();
    }
    
    /// Get queued (non-executable) transactions for an address
    pub fn get_queued(&self, address: Address) -> Vec<&Tx> {
        let confirmed = self.confirmed_nonce(address);
        let executable = self.get_executable(address);
        let executable_count = executable.len();
        
        if let Some(pending) = self.pending.get(&address) {
            pending.values()
                .skip(executable_count)
                .collect()
        } else {
            Vec::new()
        }
    }
}

/// Errors related to nonce handling
#[derive(Debug, Clone, PartialEq)]
pub enum NonceError {
    /// Transaction nonce is below the confirmed nonce
    NonceTooLow { got: u64, expected: u64 },
    
    /// Replacement transaction has insufficient fee
    ReplacementUnderpriced { existing_fee: u64, new_fee: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_tx(id: TxId, sender: Address, nonce: u64, fee: u64) -> Tx {
        Tx::with_sender(id, sender, nonce, 21_000, fee)
    }
    
    #[test]
    fn test_add_and_get_executable() {
        let mut tracker = NonceTracker::new();
        
        // Add txs in order
        tracker.add_tx(make_tx(1, 100, 0, 10)).unwrap();
        tracker.add_tx(make_tx(2, 100, 1, 20)).unwrap();
        tracker.add_tx(make_tx(3, 100, 2, 15)).unwrap();
        
        let executable = tracker.get_executable(100);
        assert_eq!(executable.len(), 3);
        assert_eq!(executable[0].nonce, 0);
        assert_eq!(executable[1].nonce, 1);
        assert_eq!(executable[2].nonce, 2);
    }
    
    #[test]
    fn test_nonce_gap() {
        let mut tracker = NonceTracker::new();
        
        // Add tx with nonce 0 and 2 (gap at 1)
        tracker.add_tx(make_tx(1, 100, 0, 10)).unwrap();
        tracker.add_tx(make_tx(2, 100, 2, 20)).unwrap(); // Gap!
        
        let executable = tracker.get_executable(100);
        assert_eq!(executable.len(), 1); // Only nonce 0
        
        // Fill the gap
        tracker.add_tx(make_tx(3, 100, 1, 15)).unwrap();
        
        let executable = tracker.get_executable(100);
        assert_eq!(executable.len(), 3); // All now executable
    }
    
    #[test]
    fn test_nonce_too_low() {
        let mut tracker = NonceTracker::new();
        tracker.set_confirmed_nonce(100, 5);
        
        let result = tracker.add_tx(make_tx(1, 100, 3, 10));
        assert!(matches!(result, Err(NonceError::NonceTooLow { .. })));
    }
    
    #[test]
    fn test_replacement() {
        let mut tracker = NonceTracker::new();
        
        tracker.add_tx(make_tx(1, 100, 0, 10)).unwrap();
        
        // Replace with higher fee
        tracker.add_tx(make_tx(2, 100, 0, 20)).unwrap();
        
        let executable = tracker.get_executable(100);
        assert_eq!(executable.len(), 1);
        assert_eq!(executable[0].id, 2);
        assert_eq!(executable[0].priority_fee_gwei, 20);
    }
    
    #[test]
    fn test_replacement_underpriced() {
        let mut tracker = NonceTracker::new();
        
        tracker.add_tx(make_tx(1, 100, 0, 20)).unwrap();
        
        // Try to replace with lower fee
        let result = tracker.add_tx(make_tx(2, 100, 0, 10));
        assert!(matches!(result, Err(NonceError::ReplacementUnderpriced { .. })));
    }
    
    #[test]
    fn test_mark_included() {
        let mut tracker = NonceTracker::new();
        
        let tx = make_tx(1, 100, 0, 10);
        tracker.add_tx(tx.clone()).unwrap();
        
        tracker.mark_included(&tx);
        
        assert_eq!(tracker.confirmed_nonce(100), 1);
        assert_eq!(tracker.pending_count(100), 0);
    }
    
    #[test]
    fn test_multiple_senders() {
        let mut tracker = NonceTracker::new();
        
        // Sender 100
        tracker.add_tx(make_tx(1, 100, 0, 10)).unwrap();
        tracker.add_tx(make_tx(2, 100, 1, 20)).unwrap();
        
        // Sender 200
        tracker.add_tx(make_tx(3, 200, 0, 30)).unwrap();
        
        let all_executable = tracker.get_all_executable();
        assert_eq!(all_executable.len(), 3);
    }
    
    #[test]
    fn test_executable_sorted() {
        let mut tracker = NonceTracker::new();
        
        tracker.add_tx(make_tx(1, 100, 0, 10)).unwrap();
        tracker.add_tx(make_tx(2, 200, 0, 30)).unwrap();
        tracker.add_tx(make_tx(3, 300, 0, 20)).unwrap();
        
        let sorted = tracker.get_executable_sorted();
        assert_eq!(sorted[0].priority_fee_gwei, 30);
        assert_eq!(sorted[1].priority_fee_gwei, 20);
        assert_eq!(sorted[2].priority_fee_gwei, 10);
    }
    
    #[test]
    fn test_is_executable() {
        let mut tracker = NonceTracker::new();
        
        let tx0 = make_tx(1, 100, 0, 10);
        let tx1 = make_tx(2, 100, 1, 20);
        let tx3 = make_tx(3, 100, 3, 30); // Gap at 2
        
        tracker.add_tx(tx0.clone()).unwrap();
        tracker.add_tx(tx1.clone()).unwrap();
        tracker.add_tx(tx3.clone()).unwrap();
        
        assert!(tracker.is_executable(&tx0));
        assert!(tracker.is_executable(&tx1));
        assert!(!tracker.is_executable(&tx3)); // Not executable due to gap
    }
    
    #[test]
    fn test_get_queued() {
        let mut tracker = NonceTracker::new();
        
        tracker.add_tx(make_tx(1, 100, 0, 10)).unwrap();
        tracker.add_tx(make_tx(2, 100, 2, 20)).unwrap(); // Gap at 1
        tracker.add_tx(make_tx(3, 100, 3, 30)).unwrap();
        
        let queued = tracker.get_queued(100);
        assert_eq!(queued.len(), 2); // Nonces 2 and 3 are queued
    }
}