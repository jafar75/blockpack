pub mod config;
pub mod tx;
pub mod block;
pub mod pool;
pub mod builder;
pub mod stream;
pub mod r#loop;
pub mod time;
pub mod sim;
pub mod metrics;
pub mod output;

// Re-export main types for convenience
pub use config::Config;
pub use tx::{Tx, TxId};
pub use block::Block;
pub use pool::{Pool, PoolStats, PoolPressure};
pub use builder::{Builder, BuildResult, UpdateResult, IncrementalStats};
pub use r#loop::{
    BuilderCommand, BuilderEvent, BuilderLoopConfig,
    LoopStats, PollingBuilder, run_builder_loop, build_from_batch,
};
pub use stream::{
    StreamConfig, StreamStats, TxGenerator,
    spawn_stream, run_stream, generate_batch, generate_simulated_stream,
};
pub use time::{
    Clock, RealClock, SimulatedClock, Simulation,
    TimedEvent, EventQueue, run_discrete_simulation,
};
pub use sim::{
    SimConfig, SimEvent, SimulationResult, BlockSimStats,
    MonteCarloResult, run_simulation, run_monte_carlo,
};
pub use metrics::{
    BuilderMetrics, MetricsReport, RollingStats, Histogram, RateTracker,
};
pub use output::{
    TxRecord, BlockRecord, SimulationSummary, SimulationOutput,
    MetricsOutput, CsvWriter, OutputFormat, write_simulation_csv,
};