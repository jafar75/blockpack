/// Configuration for the block builder simulation
#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum gas per block (Ethereum mainnet: 30M)
    pub block_gas_limit: u64,
    
    /// Target time between blocks in milliseconds
    pub target_block_time_ms: u64,
    
    /// Average transaction arrival rate (txs per second)
    pub tx_arrival_rate: f64,
    
    /// Minimum gas for a transaction
    pub min_tx_gas: u64,
    
    /// Maximum gas for a transaction
    pub max_tx_gas: u64,
    
    /// Minimum priority fee in gwei
    pub min_priority_fee_gwei: u64,
    
    /// Maximum priority fee in gwei
    pub max_priority_fee_gwei: u64,
    
    /// Maximum transactions to hold in pool
    pub max_pool_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            block_gas_limit: 30_000_000,
            target_block_time_ms: 12_000,
            tx_arrival_rate: 50.0,
            min_tx_gas: 21_000,
            max_tx_gas: 500_000,
            min_priority_fee_gwei: 1,
            max_priority_fee_gwei: 100,
            max_pool_size: 10_000,
        }
    }
}