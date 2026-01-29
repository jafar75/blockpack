# blockpack

A streaming knapsack solver for block building simulation, written in Rust.

## Overview

`blockpack` simulates the core problem faced by blockchain block builders: selecting transactions from a continuous stream to maximize value (fees) within a constrained capacity (gas limit).

This is essentially the **streaming knapsack problem** with real-world constraints inspired by Ethereum block building.

## The Problem

Block builders receive transactions continuously from mempools and must:

- Pack transactions into blocks with limited gas capacity
- Maximize total priority fees extracted
- Make decisions online as transactions arrive
- Produce valid blocks within time constraints

## Features

- **Streaming simulation**: Transactions arrive via channels with configurable arrival rates (Poisson distributed)
- **Greedy packing**: Fast baseline algorithm sorting by fee density
- **Incremental improvement**: Online replacement heuristic as better transactions arrive
- **Configurable parameters**: Block size, arrival rates, fee distributions

## Project Structure

```
src/
├── main.rs       # Entry point, wires everything together
├── tx.rs         # Transaction type
├── pool.rs       # Pending transaction pool
├── builder.rs    # Block building algorithms
├── stream.rs     # Stream simulation (producer)
└── config.rs     # Configuration parameters
```

## Usage

```bash
# Run the simulation
cargo run

# Run with release optimizations
cargo run --release

# Run tests
cargo test

# Run benchmarks
cargo bench
```

## Configuration

Parameters can be adjusted in `config.rs`:

| Parameter | Description | Default |
|-----------|-------------|---------|
| `block_gas_limit` | Maximum gas per block | 30,000,000 |
| `target_block_time_ms` | Time between blocks | 12,000 |
| `tx_arrival_rate` | Average txs per second | 50.0 |
| `min_tx_gas` | Minimum gas per tx | 21,000 |
| `max_tx_gas` | Maximum gas per tx | 500,000 |

## Algorithms

### Greedy Packing

Sort transactions by priority fee per gas (value density), pack in order until block is full. Simple, fast, and provides a reasonable baseline.

### Incremental Replacement

When a new transaction arrives:
1. If it fits in the current block, add it
2. If not, check if replacing the lowest-value transaction improves total value
3. Swap if beneficial

This maintains a valid block at all times while improving as better transactions arrive.

## Roadmap

- [x] Core types and project structure
- [ ] Basic transaction pool
- [ ] Greedy builder
- [ ] Streaming simulation
- [ ] Metrics and stats
- [ ] Nonce constraints
- [ ] Transaction conflicts
- [ ] Bundle support

## Background

This project explores the intersection of:

- **Streaming algorithms**: Making decisions with incomplete information
- **Knapsack problems**: NP-hard optimization under constraints
- **MEV infrastructure**: Real-world block building systems

Modern block builders like Titan, Flashbots, and others solve variations of this problem at scale, incorporating additional complexities like MEV extraction, bundle atomicity, and state simulation.

## License

MIT