use crate::tx::Tx;

/// A block being built or already sealed
#[derive(Debug, Clone)]
pub struct Block {
    /// Block number/height
    pub number: u64,
    
    /// Transactions included in this block
    pub transactions: Vec<Tx>,
    
    /// Maximum gas this block can use
    pub gas_limit: u64,
    
    /// Current gas used by included transactions
    pub gas_used: u64,
    
    /// Total value (fees) from all transactions
    pub total_value: u64,
}

impl Block {
    /// Create a new empty block
    pub fn new(number: u64, gas_limit: u64) -> Self {
        Self {
            number,
            transactions: Vec::new(),
            gas_limit,
            gas_used: 0,
            total_value: 0,
        }
    }
    
    /// Remaining gas capacity
    pub fn remaining_gas(&self) -> u64 {
        self.gas_limit.saturating_sub(self.gas_used)
    }
    
    /// Check if a transaction fits in the remaining space
    pub fn can_fit(&self, tx: &Tx) -> bool {
        tx.gas_limit <= self.remaining_gas()
    }
    
    /// Add a transaction to the block
    /// Returns false if it doesn't fit
    pub fn add_tx(&mut self, tx: Tx) -> bool {
        if !self.can_fit(&tx) {
            return false;
        }
        
        self.gas_used += tx.gas_limit;
        self.total_value += tx.value();
        self.transactions.push(tx);
        true
    }
    
    /// Remove a transaction by id
    /// Returns the removed transaction if found
    pub fn remove_tx(&mut self, id: u64) -> Option<Tx> {
        if let Some(pos) = self.transactions.iter().position(|tx| tx.id == id) {
            let tx = self.transactions.remove(pos);
            self.gas_used -= tx.gas_limit;
            self.total_value -= tx.value();
            Some(tx)
        } else {
            None
        }
    }
    
    /// Number of transactions in the block
    pub fn tx_count(&self) -> usize {
        self.transactions.len()
    }
    
    /// Block fullness as a percentage
    pub fn fullness(&self) -> f64 {
        if self.gas_limit == 0 {
            return 0.0;
        }
        (self.gas_used as f64 / self.gas_limit as f64) * 100.0
    }
    
    /// Find the transaction with the lowest value density
    /// Returns (index, tx) if block is non-empty
    pub fn worst_tx(&self) -> Option<(usize, &Tx)> {
        self.transactions
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.value_density()
                    .partial_cmp(&b.value_density())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
    
    /// Clear the block, keeping the same number and gas limit
    pub fn clear(&mut self) {
        self.transactions.clear();
        self.gas_used = 0;
        self.total_value = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_tx(id: u64, gas: u64, fee: u64) -> Tx {
        Tx::new(id, gas, fee)
    }
    
    #[test]
    fn test_new_block() {
        let block = Block::new(1, 30_000_000);
        assert_eq!(block.number, 1);
        assert_eq!(block.gas_limit, 30_000_000);
        assert_eq!(block.gas_used, 0);
        assert_eq!(block.total_value, 0);
        assert_eq!(block.tx_count(), 0);
    }
    
    #[test]
    fn test_add_tx() {
        let mut block = Block::new(1, 100_000);
        let tx = make_tx(1, 21_000, 10);
        
        assert!(block.add_tx(tx));
        assert_eq!(block.gas_used, 21_000);
        assert_eq!(block.total_value, 210_000);
        assert_eq!(block.tx_count(), 1);
    }
    
    #[test]
    fn test_add_tx_no_fit() {
        let mut block = Block::new(1, 20_000);
        let tx = make_tx(1, 21_000, 10);
        
        assert!(!block.add_tx(tx));
        assert_eq!(block.tx_count(), 0);
    }
    
    #[test]
    fn test_remove_tx() {
        let mut block = Block::new(1, 100_000);
        block.add_tx(make_tx(1, 21_000, 10));
        block.add_tx(make_tx(2, 30_000, 20));
        
        let removed = block.remove_tx(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, 1);
        assert_eq!(block.gas_used, 30_000);
        assert_eq!(block.tx_count(), 1);
    }
    
    #[test]
    fn test_worst_tx() {
        let mut block = Block::new(1, 1_000_000);
        block.add_tx(make_tx(1, 21_000, 5));  // density = 5
        block.add_tx(make_tx(2, 21_000, 20)); // density = 20
        block.add_tx(make_tx(3, 21_000, 10)); // density = 10
        
        let (idx, worst) = block.worst_tx().unwrap();
        assert_eq!(worst.id, 1); // lowest density
        assert_eq!(idx, 0);
    }
    
    #[test]
    fn test_fullness() {
        let mut block = Block::new(1, 100_000);
        assert_eq!(block.fullness(), 0.0);
        
        block.add_tx(make_tx(1, 50_000, 10));
        assert_eq!(block.fullness(), 50.0);
        
        block.add_tx(make_tx(2, 50_000, 10));
        assert_eq!(block.fullness(), 100.0);
    }
    
    #[test]
    fn test_clear() {
        let mut block = Block::new(1, 100_000);
        block.add_tx(make_tx(1, 21_000, 10));
        block.add_tx(make_tx(2, 30_000, 20));
        
        block.clear();
        assert_eq!(block.tx_count(), 0);
        assert_eq!(block.gas_used, 0);
        assert_eq!(block.total_value, 0);
        assert_eq!(block.number, 1); // Preserved
    }
}