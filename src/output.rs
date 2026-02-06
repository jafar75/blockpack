//! Output formatting for simulation results
//!
//! Supports JSON, CSV, and human-readable formats.

use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::block::Block;
use crate::metrics::MetricsReport;
use crate::sim::SimulationResult;
use crate::tx::Tx;

/// Serializable transaction record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    pub id: u64,
    pub gas_limit: u64,
    pub priority_fee_gwei: u64,
    pub value: u64,
}

impl From<&Tx> for TxRecord {
    fn from(tx: &Tx) -> Self {
        Self {
            id: tx.id,
            gas_limit: tx.gas_limit,
            priority_fee_gwei: tx.priority_fee_gwei,
            value: tx.value(),
        }
    }
}

/// Serializable block record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRecord {
    pub number: u64,
    pub tx_count: usize,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub total_value: u64,
    pub fullness_pct: f64,
    pub transactions: Vec<TxRecord>,
}

impl From<&Block> for BlockRecord {
    fn from(block: &Block) -> Self {
        Self {
            number: block.number,
            tx_count: block.tx_count(),
            gas_used: block.gas_used,
            gas_limit: block.gas_limit,
            total_value: block.total_value,
            fullness_pct: block.fullness(),
            transactions: block.transactions.iter().map(TxRecord::from).collect(),
        }
    }
}

/// Serializable simulation summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSummary {
    pub blocks_produced: usize,
    pub txs_generated: u64,
    pub txs_included: u64,
    pub inclusion_rate_pct: f64,
    pub total_value: u64,
    pub total_gas: u64,
    pub avg_block_fullness_pct: f64,
    pub sim_duration_secs: f64,
    pub real_duration_secs: f64,
    pub speedup: f64,
}

impl SimulationSummary {
    pub fn from_result(result: &SimulationResult, gas_limit: u64) -> Self {
        Self {
            blocks_produced: result.blocks.len(),
            txs_generated: result.txs_generated,
            txs_included: result.txs_included,
            inclusion_rate_pct: result.inclusion_rate() * 100.0,
            total_value: result.total_value(),
            total_gas: result.total_gas(),
            avg_block_fullness_pct: result.avg_block_fullness(gas_limit),
            sim_duration_secs: result.sim_duration.as_secs_f64(),
            real_duration_secs: result.real_duration.as_secs_f64(),
            speedup: result.speedup(),
        }
    }
}

/// Full simulation output for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationOutput {
    pub summary: SimulationSummary,
    pub blocks: Vec<BlockRecord>,
}

impl SimulationOutput {
    pub fn from_result(result: &SimulationResult, gas_limit: u64) -> Self {
        Self {
            summary: SimulationSummary::from_result(result, gas_limit),
            blocks: result.blocks.iter().map(BlockRecord::from).collect(),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_json_compact(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn write_json<W: Write>(&self, writer: W) -> Result<(), serde_json::Error> {
        serde_json::to_writer_pretty(writer, self)
    }
}

/// CSV writer for block data
pub struct CsvWriter<W: Write> {
    writer: W,
    header_written: bool,
}

impl<W: Write> CsvWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            header_written: false,
        }
    }

    pub fn write_blocks(&mut self, blocks: &[Block]) -> std::io::Result<()> {
        if !self.header_written {
            writeln!(self.writer, "block_number,tx_count,gas_used,gas_limit,total_value,fullness_pct")?;
            self.header_written = true;
        }

        for block in blocks {
            writeln!(
                self.writer,
                "{},{},{},{},{},{:.2}",
                block.number,
                block.tx_count(),
                block.gas_used,
                block.gas_limit,
                block.total_value,
                block.fullness()
            )?;
        }

        Ok(())
    }

    pub fn write_transactions(&mut self, txs: &[Tx]) -> std::io::Result<()> {
        if !self.header_written {
            writeln!(self.writer, "tx_id,gas_limit,priority_fee_gwei,value")?;
            self.header_written = true;
        }

        for tx in txs {
            writeln!(
                self.writer,
                "{},{},{},{}",
                tx.id,
                tx.gas_limit,
                tx.priority_fee_gwei,
                tx.value()
            )?;
        }

        Ok(())
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

/// Write simulation results to CSV
pub fn write_simulation_csv<W: Write>(
    result: &SimulationResult,
    blocks_writer: W,
) -> std::io::Result<()> {
    let mut csv = CsvWriter::new(blocks_writer);
    csv.write_blocks(&result.blocks)
}

/// Serializable metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsOutput {
    pub uptime_secs: f64,
    pub tx_received: u64,
    pub tx_added: u64,
    pub tx_replaced: u64,
    pub tx_rejected: u64,
    pub tx_rate: f64,
    pub acceptance_rate_pct: f64,
    pub blocks_built: u64,
    pub block_rate: f64,
    pub total_gas_used: u64,
    pub total_value: u64,
    pub avg_block_fullness_pct: f64,
    pub avg_block_value: f64,
    pub avg_block_txs: f64,
    pub avg_tx_gas: f64,
    pub avg_tx_fee: f64,
    pub p50_fee: f64,
    pub p95_fee: f64,
    pub p99_fee: f64,
}

impl From<&MetricsReport> for MetricsOutput {
    fn from(report: &MetricsReport) -> Self {
        Self {
            uptime_secs: report.uptime.as_secs_f64(),
            tx_received: report.tx_received,
            tx_added: report.tx_added,
            tx_replaced: report.tx_replaced,
            tx_rejected: report.tx_rejected,
            tx_rate: report.tx_rate,
            acceptance_rate_pct: report.acceptance_rate * 100.0,
            blocks_built: report.blocks_built,
            block_rate: report.block_rate,
            total_gas_used: report.total_gas_used,
            total_value: report.total_value,
            avg_block_fullness_pct: report.avg_block_fullness,
            avg_block_value: report.avg_block_value,
            avg_block_txs: report.avg_block_txs,
            avg_tx_gas: report.avg_tx_gas,
            avg_tx_fee: report.avg_tx_fee,
            p50_fee: report.p50_fee,
            p95_fee: report.p95_fee,
            p99_fee: report.p99_fee,
        }
    }
}

impl MetricsOutput {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Output format enum for CLI
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Human,
    Json,
    JsonCompact,
    Csv,
}

impl OutputFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "human" | "text" | "pretty" => Some(Self::Human),
            "json" => Some(Self::Json),
            "json-compact" | "jsonc" => Some(Self::JsonCompact),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_block(num: u64, txs: Vec<Tx>) -> Block {
        let mut block = Block::new(num, 1_000_000);
        for tx in txs {
            block.add_tx(tx);
        }
        block
    }

    #[test]
    fn test_tx_record() {
        let tx = Tx::new(1, 21_000, 50);
        let record = TxRecord::from(&tx);
        
        assert_eq!(record.id, 1);
        assert_eq!(record.gas_limit, 21_000);
        assert_eq!(record.priority_fee_gwei, 50);
        assert_eq!(record.value, 21_000 * 50);
    }

    #[test]
    fn test_block_record() {
        let block = make_block(1, vec![
            Tx::new(1, 21_000, 50),
            Tx::new(2, 50_000, 30),
        ]);
        let record = BlockRecord::from(&block);
        
        assert_eq!(record.number, 1);
        assert_eq!(record.tx_count, 2);
        assert_eq!(record.transactions.len(), 2);
    }

    #[test]
    fn test_json_output() {
        let block = make_block(1, vec![Tx::new(1, 21_000, 50)]);
        let record = BlockRecord::from(&block);
        
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"number\":1"));
        assert!(json.contains("\"tx_count\":1"));
    }

    #[test]
    fn test_csv_writer() {
        let blocks = vec![
            make_block(1, vec![Tx::new(1, 21_000, 50)]),
            make_block(2, vec![Tx::new(2, 50_000, 30)]),
        ];
        
        let mut output = Vec::new();
        {
            let mut csv = CsvWriter::new(&mut output);
            csv.write_blocks(&blocks).unwrap();
        }
        
        let csv_str = String::from_utf8(output).unwrap();
        assert!(csv_str.contains("block_number,tx_count"));
        assert!(csv_str.contains("1,1,21000"));
        assert!(csv_str.contains("2,1,50000"));
    }

    #[test]
    fn test_output_format_parsing() {
        assert_eq!(OutputFormat::from_str("json"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::from_str("JSON"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::from_str("csv"), Some(OutputFormat::Csv));
        assert_eq!(OutputFormat::from_str("human"), Some(OutputFormat::Human));
        assert_eq!(OutputFormat::from_str("invalid"), None);
    }
}