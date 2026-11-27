//! euthymia-admit — verification and admission CLI for SFT JSONL batches.
//!
//!   euthymia-admit --scores scores.jsonl --batch <name> <input.jsonl> [...]
//!
//! Rows are never mutated in place. Admitted output lands under
//! `data/admitted/`, rejected output under `data/rejected/`, and the
//! machine-readable report records every row decision and input hash.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use euthymia_verify::{FileScorer, admit_batch, require_exists};

#[derive(Parser)]
#[command(
    name = "euthymia-admit",
    about = "Admit or reject euthymia SFT records through schema, privacy, critical-failure, and rubric gates"
)]
struct Cli {
    /// JSONL score file keyed by scenario_id
    #[arg(long)]
    scores: PathBuf,
    /// Batch name used for admitted/rejected output filenames
    #[arg(long)]
    batch: String,
    /// Output root containing admitted/ and rejected/ (default: data)
    #[arg(long, default_value = "data")]
    output_root: PathBuf,
    /// Admission report path (default: <output-root>/admission_report.json)
    #[arg(long)]
    report: Option<PathBuf>,
    /// Input candidate JSONL files; input verification fields are ignored
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    require_exists(&cli.scores, "score file")?;
    for input in &cli.inputs {
        require_exists(input, "input file")?;
    }

    let scorer = FileScorer::load(&cli.scores)?;
    let report = cli
        .report
        .unwrap_or_else(|| cli.output_root.join("admission_report.json"));
    let outputs = admit_batch(&cli.inputs, &scorer, &cli.batch, &cli.output_root, &report)?;

    println!("admitted: {}", outputs.admitted.display());
    println!("rejected: {}", outputs.rejected.display());
    println!("report: {}", outputs.report.display());
    Ok(())
}
