pub mod config;
pub mod tx;
pub mod block;
pub mod pool;
pub mod builder;
pub mod stream;
pub mod r#loop;

// Re-export main types for convenience
pub use config::Config;
pub use tx::{Tx, TxId};
pub use block::Block;
pub use pool::Pool;
pub use builder::{Builder, BuildResult, UpdateResult};
pub use r#loop::{
    BuilderCommand, BuilderEvent, BuilderLoopConfig,
    LoopStats, PollingBuilder, run_builder_loop, build_from_batch,
};