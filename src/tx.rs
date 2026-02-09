use std::time::Instant;

/// Unique identifier for a transaction
pub type TxId = u64;

/// Address type (simplified as u64 for simulation)
pub type Address = u64;

/// A transaction in the mempool
#[derive(Debug, Clone)]
pub struct Tx {
    /// Unique identifier
    pub id: TxId,
    
    /// Sender address
    pub sender: Address,
    
    /// Transaction nonce (sequence number for sender)
    pub nonce: u64,
    
    /// Gas required to execute this transaction
    pub gas_limit: u64,
    
    /// Max priority fee (tip) in gwei - goes to block builder
    pub priority_fee_gwei: u64,
    
    /// Max fee per gas in gwei (EIP-1559)
    /// If None, treated as legacy tx where max_fee = priority_fee
    pub max_fee_gwei: Option<u64>,
    
    /// When this transaction arrived in the pool
    pub arrival_time: Instant,
}

impl Tx {
    /// Create a new transaction (legacy style - priority fee only)
    pub fn new(id: TxId, gas_limit: u64, priority_fee_gwei: u64) -> Self {
        Self {
            id,
            sender: 0,
            nonce: 0,
            gas_limit,
            priority_fee_gwei,
            max_fee_gwei: None,
            arrival_time: Instant::now(),
        }
    }
    
    /// Create a transaction with sender and nonce
    pub fn with_sender(
        id: TxId,
        sender: Address,
        nonce: u64,
        gas_limit: u64,
        priority_fee_gwei: u64,
    ) -> Self {
        Self {
            id,
            sender,
            nonce,
            gas_limit,
            priority_fee_gwei,
            max_fee_gwei: None,
            arrival_time: Instant::now(),
        }
    }
    
    /// Create an EIP-1559 transaction with max fee
    pub fn new_eip1559(
        id: TxId,
        gas_limit: u64,
        priority_fee_gwei: u64,
        max_fee_gwei: u64,
    ) -> Self {
        Self {
            id,
            sender: 0,
            nonce: 0,
            gas_limit,
            priority_fee_gwei,
            max_fee_gwei: Some(max_fee_gwei),
            arrival_time: Instant::now(),
        }
    }
    
    /// Create a full EIP-1559 transaction with all fields
    pub fn new_full(
        id: TxId,
        sender: Address,
        nonce: u64,
        gas_limit: u64,
        priority_fee_gwei: u64,
        max_fee_gwei: Option<u64>,
    ) -> Self {
        Self {
            id,
            sender,
            nonce,
            gas_limit,
            priority_fee_gwei,
            max_fee_gwei,
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
            sender: 0,
            nonce: 0,
            gas_limit,
            priority_fee_gwei,
            max_fee_gwei: None,
            arrival_time,
        }
    }
    
    /// Calculate effective priority fee given current base fee
    pub fn effective_priority_fee(&self, base_fee_gwei: u64) -> u64 {
        match self.max_fee_gwei {
            Some(max_fee) => {
                if max_fee < base_fee_gwei {
                    0
                } else {
                    self.priority_fee_gwei.min(max_fee - base_fee_gwei)
                }
            }
            None => self.priority_fee_gwei,
        }
    }
    
    /// Check if transaction can be included at given base fee
    pub fn can_afford_base_fee(&self, base_fee_gwei: u64) -> bool {
        match self.max_fee_gwei {
            Some(max_fee) => max_fee >= base_fee_gwei,
            None => true,
        }
    }
    
    /// Total value of this transaction (priority_fee * gas)
    pub fn value(&self) -> u64 {
        self.priority_fee_gwei * self.gas_limit
    }
    
    /// Value at a specific base fee (EIP-1559 aware)
    pub fn value_at_base_fee(&self, base_fee_gwei: u64) -> u64 {
        self.effective_priority_fee(base_fee_gwei) * self.gas_limit
    }
    
    /// Value density (value per unit of gas)
    pub fn value_density(&self) -> f64 {
        self.priority_fee_gwei as f64
    }
    
    /// Value density at a specific base fee
    pub fn value_density_at_base_fee(&self, base_fee_gwei: u64) -> f64 {
        self.effective_priority_fee(base_fee_gwei) as f64
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
        let tx2 = Tx::new(1, 50_000, 20);
        let tx3 = Tx::new(2, 21_000, 10);
        
        assert_eq!(tx1, tx2);
        assert_ne!(tx1, tx3);
    }
    
    #[test]
    fn test_eip1559_effective_priority_fee() {
        let tx = Tx::new_eip1559(1, 21_000, 20, 100);
        assert_eq!(tx.effective_priority_fee(50), 20);
        assert_eq!(tx.effective_priority_fee(90), 10);
        assert_eq!(tx.effective_priority_fee(110), 0);
    }
    
    #[test]
    fn test_can_afford_base_fee() {
        let tx = Tx::new_eip1559(1, 21_000, 20, 100);
        
        assert!(tx.can_afford_base_fee(50));
        assert!(tx.can_afford_base_fee(100));
        assert!(!tx.can_afford_base_fee(101));
    }
    
    #[test]
    fn test_legacy_tx_base_fee() {
        let tx = Tx::new(1, 21_000, 50);
        
        assert_eq!(tx.effective_priority_fee(30), 50);
        assert!(tx.can_afford_base_fee(1000));
    }
    
    #[test]
    fn test_value_at_base_fee() {
        let tx = Tx::new_eip1559(1, 21_000, 20, 100);
        
        assert_eq!(tx.value_at_base_fee(50), 420_000);
        assert_eq!(tx.value_at_base_fee(90), 210_000);
    }
    
    #[test]
    fn test_with_sender() {
        let tx = Tx::with_sender(1, 100, 5, 21_000, 50);
        
        assert_eq!(tx.id, 1);
        assert_eq!(tx.sender, 100);
        assert_eq!(tx.nonce, 5);
        assert_eq!(tx.gas_limit, 21_000);
        assert_eq!(tx.priority_fee_gwei, 50);
    }
    
    #[test]
    fn test_new_full() {
        let tx = Tx::new_full(1, 200, 10, 50_000, 30, Some(100));
        
        assert_eq!(tx.id, 1);
        assert_eq!(tx.sender, 200);
        assert_eq!(tx.nonce, 10);
        assert_eq!(tx.gas_limit, 50_000);
        assert_eq!(tx.priority_fee_gwei, 30);
        assert_eq!(tx.max_fee_gwei, Some(100));
    }
}