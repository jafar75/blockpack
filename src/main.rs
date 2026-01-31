use blockpack::config::Config;
use blockpack::tx::Tx;
use blockpack::builder::{Builder, UpdateResult};

fn main() {
    let config = Config::default();
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!();
    
    // Demonstrate incremental building
    // Simulate a stream where transactions arrive over time
    // Earlier txs tend to be worse, better ones come later
    
    let gas_limit = 200_000;
    let mut builder = Builder::new(gas_limit);
    
    println!("Simulating transaction stream (gas limit: {})", gas_limit);
    println!();
    
    // Stream of transactions - worse ones first, better ones later
    let stream = vec![
        Tx::new(1, 50_000, 10),   // density 10
        Tx::new(2, 60_000, 12),   // density 12
        Tx::new(3, 40_000, 15),   // density 15
        Tx::new(4, 50_000, 8),    // density 8
        Tx::new(5, 45_000, 25),   // density 25 - better!
        Tx::new(6, 55_000, 30),   // density 30 - even better!
        Tx::new(7, 70_000, 35),   // density 35 - best so far
        Tx::new(8, 30_000, 20),   // density 20
        Tx::new(9, 80_000, 40),   // density 40 - excellent
        Tx::new(10, 50_000, 5),   // density 5 - too late, too bad
    ];
    
    println!("{:>4} {:>8} {:>6} {:>8} {:>10} {:>12} {:>10}",
        "TX", "Gas", "Fee", "Density", "Result", "Block Value", "Block Gas");
    println!("{:-<75}", "");
    
    for tx in stream {
        let id = tx.id;
        let gas = tx.gas_limit;
        let fee = tx.priority_fee_gwei;
        let density = tx.value_density();
        
        let result = builder.process_tx(tx);
        
        let result_str = match &result {
            UpdateResult::Added => "Added".to_string(),
            UpdateResult::Replaced(evicted) => format!("Replaced({})", evicted),
            UpdateResult::Rejected => "Rejected".to_string(),
            UpdateResult::Duplicate => "Duplicate".to_string(),
        };
        
        let candidate = builder.candidate().unwrap();
        
        println!("{:>4} {:>8} {:>6} {:>8.1} {:>10} {:>12} {:>10}",
            id, gas, fee, density, result_str, 
            candidate.total_value, candidate.gas_used);
    }
    
    println!("{:-<75}", "");
    
    // Show final stats
    let stats = builder.stats();
    println!();
    println!("Incremental stats:");
    println!("  Received: {}", stats.txs_received);
    println!("  Added: {}", stats.txs_added);
    println!("  Replaced: {}", stats.txs_replaced);
    println!("  Rejected: {}", stats.txs_rejected);
    println!("  Acceptance rate: {:.1}%", stats.acceptance_rate() * 100.0);
    
    // Show final block
    println!();
    println!("Final candidate block:");
    let candidate = builder.candidate().unwrap();
    println!("  Transactions: {}", candidate.tx_count());
    println!("  Gas: {} / {} ({:.1}%)", 
        candidate.gas_used, candidate.gas_limit, candidate.fullness());
    println!("  Total value: {}", candidate.total_value);
    
    // Sanity check
    assert!(candidate.gas_used <= candidate.gas_limit, 
        "BUG: Block exceeds gas limit!");
    
    println!();
    println!("Included transactions (sorted by density):");
    let mut txs: Vec<_> = candidate.transactions.iter().collect();
    txs.sort_by(|a, b| b.value_density().partial_cmp(&a.value_density()).unwrap());
    
    for tx in txs {
        println!("  TX {:>2}: gas {:>6}, fee {:>3}, density {:>5.1}, value {:>10}",
            tx.id, tx.gas_limit, tx.priority_fee_gwei, 
            tx.value_density(), tx.value());
    }
    
    // Seal and show result
    println!();
    let block = builder.seal().unwrap();
    println!("Sealed block #{}", block.number);
    println!("Builder ready for block #{}", builder.next_block_number());
    
    println!();
    println!("TODO: Wire up stream producer");
}