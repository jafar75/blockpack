use std::time::Duration;

use blockpack::{
    Tx, Pool, Builder, PoolPressure,
    SimConfig, StreamConfig, BuilderMetrics, SimulationOutput,
    run_simulation, run_monte_carlo,
    NonceTracker, NonceError,
    AccessList, ConflictTracker, ConflictType,
    Bundle, BundlePool,
    TxPriority, ScheduledTx, LookaheadPlanner,
};

fn main() {
    println!("blockpack - streaming knapsack block builder");
    println!("============================================");
    println!();
    
    // Phase 5 demos
    demo_eip1559();
    println!();
    println!("{:=<70}", "");
    println!();
    
    demo_pool_pressure();
    println!();
    println!("{:=<70}", "");
    println!();
    
    // Phase 6 demos
    demo_nonce_tracking();
    println!();
    println!("{:=<70}", "");
    println!();
    
    demo_conflict_detection();
    println!();
    println!("{:=<70}", "");
    println!();
    
    demo_bundles();
    println!();
    println!("{:=<70}", "");
    println!();
    
    demo_lookahead();
    println!();
    println!("{:=<70}", "");
    println!();
    
    // Full simulation
    demo_simulation_with_metrics();
}

fn demo_eip1559() {
    println!("Demo: EIP-1559 Transaction Support");
    println!("{:-<70}", "");
    println!();
    
    let mut builder = Builder::new(200_000);
    builder.set_base_fee(30);
    
    println!("Base fee: {} gwei", builder.base_fee());
    println!();
    
    let txs = vec![
        ("Legacy", Tx::new(1, 50_000, 50)),
        ("EIP-1559 (fits)", Tx::new_eip1559(2, 50_000, 40, 100)),
        ("EIP-1559 (capped)", Tx::new_eip1559(3, 50_000, 30, 50)),
        ("EIP-1559 (can't afford)", Tx::new_eip1559(4, 50_000, 20, 25)),
    ];
    
    println!("{:<25} {:>10} {:>10} {:>10} {:>15}", 
        "Type", "Gas", "Priority", "MaxFee", "Result");
    println!("{:-<70}", "");
    
    for (name, tx) in txs {
        let gas = tx.gas_limit;
        let priority = tx.priority_fee_gwei;
        let max_fee = tx.max_fee_gwei.map(|f| f.to_string()).unwrap_or("-".to_string());
        let result = builder.process_tx(tx);
        println!("{:<25} {:>10} {:>10} {:>10} {:>15?}", 
            name, gas, priority, max_fee, result);
    }
    
    println!();
    let candidate = builder.candidate().unwrap();
    println!("Block: {} txs, {} gas, {} value",
        candidate.tx_count(), candidate.gas_used, candidate.total_value);
}

fn demo_pool_pressure() {
    println!("Demo: Pool Pressure Management");
    println!("{:-<70}", "");
    println!();
    
    let mut pool = Pool::new(20);
    println!("Pool capacity: 20 transactions");
    println!();
    
    println!("{:>5} {:>10} {:>12} {:>15}", "TXs", "Fill%", "Pressure", "Min Threshold");
    println!("{:-<50}", "");
    
    for i in 1..=25 {
        let fee = 10 + (i % 20);
        let _ = pool.insert(Tx::new(i, 21_000, fee));
        
        if i == 5 || i == 10 || i == 16 || i == 19 || i == 20 {
            let stats = pool.stats();
            println!("{:>5} {:>9.1}% {:>12?} {:>15.1}",
                stats.count, pool.fill_percent(), stats.pressure, stats.min_threshold);
        }
    }
    
    println!();
    let stats = pool.stats();
    println!("Final: {} inserted, {} evicted, {} rejected",
        stats.total_inserted, stats.total_evicted, stats.total_rejected);
}

fn demo_nonce_tracking() {
    println!("Demo: Nonce Tracking (Step 6.1)");
    println!("{:-<70}", "");
    println!();
    
    let mut tracker = NonceTracker::new();
    
    // Sender 100: add txs in order
    println!("Sender 100: Adding nonces 0, 1, 2 in order");
    tracker.add_tx(Tx::with_sender(1, 100, 0, 21_000, 10)).unwrap();
    tracker.add_tx(Tx::with_sender(2, 100, 1, 21_000, 20)).unwrap();
    tracker.add_tx(Tx::with_sender(3, 100, 2, 21_000, 15)).unwrap();
    
    let executable = tracker.get_executable(100);
    println!("  Executable txs: {} (nonces: {:?})", 
        executable.len(),
        executable.iter().map(|t| t.nonce).collect::<Vec<_>>());
    
    // Sender 200: add with gap
    println!();
    println!("Sender 200: Adding nonces 0 and 2 (gap at 1)");
    tracker.add_tx(Tx::with_sender(4, 200, 0, 21_000, 30)).unwrap();
    tracker.add_tx(Tx::with_sender(5, 200, 2, 21_000, 25)).unwrap();
    
    let executable = tracker.get_executable(200);
    let queued = tracker.get_queued(200);
    println!("  Executable: {} (only nonce 0)", executable.len());
    println!("  Queued: {} (nonce 2 waiting for 1)", queued.len());
    
    // Fill the gap
    println!();
    println!("Sender 200: Filling gap with nonce 1");
    tracker.add_tx(Tx::with_sender(6, 200, 1, 21_000, 20)).unwrap();
    
    let executable = tracker.get_executable(200);
    println!("  Executable now: {} (all unblocked)", executable.len());
    
    // Test replacement
    println!();
    println!("Testing nonce replacement:");
    let result = tracker.add_tx(Tx::with_sender(7, 100, 0, 21_000, 5)); // Lower fee
    println!("  Replace nonce 0 with lower fee: {:?}", result);
    
    let result = tracker.add_tx(Tx::with_sender(8, 100, 0, 21_000, 50)); // Higher fee
    println!("  Replace nonce 0 with higher fee: {:?}", result);
    
    // Test nonce too low
    println!();
    tracker.set_confirmed_nonce(100, 2);
    let result = tracker.add_tx(Tx::with_sender(9, 100, 1, 21_000, 100));
    println!("After confirming nonce 2, adding nonce 1: {:?}", result);
}

fn demo_conflict_detection() {
    println!("Demo: Conflict Detection (Step 6.2)");
    println!("{:-<70}", "");
    println!();
    
    let mut tracker = ConflictTracker::new();
    
    // TX 1: writes to contract 100, slot 1
    let mut access1 = AccessList::new();
    access1.add_write(100, 1);
    tracker.register(1, access1);
    println!("TX 1: writes to (contract=100, slot=1)");
    
    // TX 2: reads from contract 100, slot 1 (conflicts with TX 1)
    let mut access2 = AccessList::new();
    access2.add_read(100, 1);
    tracker.register(2, access2);
    println!("TX 2: reads from (contract=100, slot=1)");
    
    // TX 3: writes to contract 100, slot 2 (no conflict)
    let mut access3 = AccessList::new();
    access3.add_write(100, 2);
    tracker.register(3, access3);
    println!("TX 3: writes to (contract=100, slot=2)");
    
    // TX 4: writes to contract 100, slot 1 (conflicts with TX 1 and TX 2)
    let mut access4 = AccessList::new();
    access4.add_write(100, 1);
    tracker.register(4, access4);
    println!("TX 4: writes to (contract=100, slot=1)");
    
    println!();
    println!("Conflict analysis:");
    println!("  TX 1 <-> TX 2: {:?}", tracker.conflict_type(1, 2));
    println!("  TX 1 <-> TX 3: {:?}", tracker.conflict_type(1, 3));
    println!("  TX 1 <-> TX 4: {:?}", tracker.conflict_type(1, 4));
    println!("  TX 2 <-> TX 4: {:?}", tracker.conflict_type(2, 4));
    
    println!();
    println!("Conflicts for TX 1: {:?}", tracker.get_conflicts(1));
    println!("Conflicts for TX 3: {:?}", tracker.get_conflicts(3));
    
    println!();
    let non_conflicting = tracker.find_non_conflicting(&[1, 2, 3, 4]);
    println!("Non-conflicting subset from [1,2,3,4]: {:?}", non_conflicting);
}

fn demo_bundles() {
    println!("Demo: MEV Bundle Support (Step 6.3)");
    println!("{:-<70}", "");
    println!();
    
    let mut pool = BundlePool::new(10);
    
    // Create bundles
    let bundle1 = Bundle::new(1, vec![
        Tx::new(101, 50_000, 30),
        Tx::new(102, 50_000, 30),
    ]);
    println!("Bundle 1: {} txs, {} gas, {} value, density {:.1}",
        bundle1.tx_count(), bundle1.total_gas(), bundle1.total_value(), bundle1.value_density());
    
    let bundle2 = Bundle::for_block(2, vec![
        Tx::new(201, 100_000, 50),
    ], 100);
    println!("Bundle 2: {} txs, targets block 100, density {:.1}",
        bundle2.tx_count(), bundle2.value_density());
    
    let bundle3 = Bundle::new(3, vec![
        Tx::new(301, 75_000, 40),
        Tx::new(302, 25_000, 60),
    ]).with_timestamp_range(Some(1000), Some(2000));
    println!("Bundle 3: {} txs, timestamp range [1000, 2000], density {:.1}",
        bundle3.tx_count(), bundle3.value_density());
    
    // Add to pool
    pool.add(bundle1);
    pool.add(bundle2);
    pool.add(bundle3);
    
    println!();
    println!("Pool has {} bundles", pool.len());
    
    // Get bundles for specific block
    let for_block_100 = pool.get_for_block(100);
    println!("Bundles valid for block 100: {}", for_block_100.len());
    
    let for_block_50 = pool.get_for_block(50);
    println!("Bundles valid for block 50: {} (bundle 2 excluded)", for_block_50.len());
    
    // Sorted by density
    println!();
    println!("Bundles sorted by value density:");
    for bundle in pool.sorted_by_density() {
        println!("  Bundle {}: density {:.1}", bundle.id, bundle.value_density());
    }
    
    // Mark included and check stats
    pool.mark_included(1);
    let stats = pool.stats();
    println!();
    println!("After including bundle 1:");
    println!("  Remaining: {}, Included: {}", stats.count, stats.total_included);
}

fn demo_lookahead() {
    println!("Demo: Multi-block Lookahead (Step 6.4)");
    println!("{:-<70}", "");
    println!();
    
    let mut planner = LookaheadPlanner::new(1, 100_000, 4);
    println!("Planner: 4 block lookahead, 100k gas per block");
    println!();
    
    // Add transactions with different priorities
    println!("Adding transactions:");
    
    planner.add_tx(ScheduledTx::urgent(Tx::new(1, 30_000, 100)));
    println!("  TX 1: Urgent, 30k gas, fee 100");
    
    planner.add_tx(ScheduledTx::with_priority(Tx::new(2, 40_000, 50), TxPriority::High));
    println!("  TX 2: High priority, 40k gas, fee 50");
    
    planner.add_tx(ScheduledTx::new(Tx::new(3, 50_000, 30)));
    println!("  TX 3: Normal, 50k gas, fee 30");
    
    planner.add_tx(ScheduledTx::with_priority(Tx::new(4, 60_000, 20), TxPriority::Low));
    println!("  TX 4: Low priority, 60k gas, fee 20");
    
    planner.add_tx(ScheduledTx::with_deadline(Tx::new(5, 35_000, 40), 2));
    println!("  TX 5: Normal with deadline block 2, 35k gas, fee 40");
    
    // More txs to fill blocks
    planner.add_tx(ScheduledTx::new(Tx::new(6, 45_000, 25)));
    planner.add_tx(ScheduledTx::new(Tx::new(7, 55_000, 35)));
    println!("  TX 6-7: Normal priority fillers");
    
    // Run planning
    println!();
    println!("Running planner...");
    planner.plan();
    
    // Show results
    let stats = planner.stats();
    println!();
    println!("Plan results:");
    println!("  Total planned: {} txs", stats.total_planned_txs);
    println!("  Total value: {}", stats.total_planned_value);
    println!("  Pending (couldn't fit): {}", stats.pending_count);
    
    println!();
    println!("Block breakdown:");
    for block_stat in &stats.block_stats {
        let block = planner.get_block_plan(block_stat.block_number).unwrap();
        let tx_ids: Vec<_> = block.transactions.iter().map(|t| t.id).collect();
        println!("  Block {}: {} txs, {:.1}% full, value {}, txs: {:?}",
            block_stat.block_number,
            block_stat.tx_count,
            block_stat.fullness,
            block_stat.total_value,
            tx_ids);
    }
    
    // Advance window
    println!();
    println!("Advancing window (block 1 complete)...");
    let completed = planner.advance();
    println!("  Completed block {}: {} txs, {} value",
        completed.number, completed.tx_count(), completed.total_value);
    println!("  Window now starts at block {}", planner.get_next_block().number);
}

fn demo_simulation_with_metrics() {
    println!("Demo: Full Simulation with Metrics");
    println!("{:-<70}", "");
    println!();
    
    let config = SimConfig {
        block_gas_limit: 1_000_000,
        block_time: Duration::from_secs(12),
        sim_duration: Duration::from_secs(60),
        stream_config: StreamConfig {
            tx_rate: 50.0,
            min_gas: 21_000,
            max_gas: 300_000,
            min_fee: 5,
            max_fee: 150,
            log_normal_fees: true,
            fee_mu: 2.5,
            fee_sigma: 0.7,
            ..Default::default()
        },
        start_block: 1,
    };
    
    println!("Running 60s simulation at 50 tx/s...");
    let result = run_simulation(config.clone());
    
    println!("Completed in {:?} ({:.0}x speedup)", result.real_duration, result.speedup());
    println!();
    
    // Build metrics
    let mut metrics = BuilderMetrics::new();
    for stats in &result.block_stats {
        let avg_gas = if stats.tx_count > 0 { stats.gas_used / stats.tx_count as u64 } else { 0 };
        let avg_fee = if stats.gas_used > 0 { stats.total_value / stats.gas_used } else { 0 };
        for _ in 0..stats.tx_count {
            metrics.record_tx(avg_gas, avg_fee, true, false);
        }
        metrics.record_block(stats.tx_count, stats.gas_used, stats.total_value, config.block_gas_limit);
    }
    
    let report = metrics.report();
    println!("Blocks: {}, TXs included: {}", report.blocks_built, report.tx_received);
    println!("Avg block fullness: {:.1}%", report.avg_block_fullness);
    println!("Total value: {}", report.total_value);
    println!();
    
    // JSON summary
    println!("JSON Summary:");
    let output = SimulationOutput::from_result(&result, config.block_gas_limit);
    println!("{}", serde_json::to_string_pretty(&output.summary).unwrap());
}