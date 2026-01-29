use std::time::Instant;

/// Unique identifier for a transaction
pub type TxId = u64;

/// A transaction in the mempool
#[derive(Debug, Clone)]
pub struct Tx {
    /// Unique identifier
    pub id: TxId,
    
    /// Gas required to execute this transaction
    pub gas_limit: u64,
    
    /// Priority fee in gwei (tip to block builder)
    pub priority_fee_gwei: u64,
    
    /// When this transaction arrived in the pool
    pub arrival_time: Instant,
}

impl Tx {
    /// Create a new transaction
    pub fn new(id: TxId, gas_limit: u64, priority_fee_gwei: u64) -> Self {
        Self {
            id,
            gas_limit,
            priority_fee_gwei,
            arrival_time: Instant::now(),
        }
    }
    
    /// Create a transaction with explicit arrival time
    pub fn with_arrival_time(
        id: TxId,
        gas_limit: u64,
        priority_fee_gwei: u64,
        arrival_time: Instant,
    ) -> Self {
        Self {
            id,
            gas_limit,
            priority_fee_gwei,
            arrival_time,
        }
    }
    
    /// Total value of this transaction (priority_fee * gas)
    /// This is what the builder earns by including it
    pub fn value(&self) -> u64 {
        self.priority_fee_gwei * self.gas_limit
    }
    
    /// Value density (value per unit of gas)
    /// Used for greedy sorting - higher is better
    pub fn value_density(&self) -> f64 {
        self.priority_fee_gwei as f64
    }
}

impl PartialEq for Tx {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Tx {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tx_value() {
        let tx = Tx::new(1, 21_000, 10);
        assert_eq!(tx.value(), 210_000);
    }
    
    #[test]
    fn test_tx_value_density() {
        let tx = Tx::new(1, 21_000, 10);
        assert_eq!(tx.value_density(), 10.0);
    }
    
    #[test]
    fn test_tx_equality() {
        let tx1 = Tx::new(1, 21_000, 10);
        let tx2 = Tx::new(1, 50_000, 20); // Same ID, different params
        let tx3 = Tx::new(2, 21_000, 10);
        
        assert_eq!(tx1, tx2); // Same ID = equal
        assert_ne!(tx1, tx3); // Different ID = not equal
    }
}