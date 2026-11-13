//! Recursa CLI — demo and core recursion entrypoints.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use recursa_core::schedule;
use recursa_core::BenchmarkAdapter;
use recursa_demo::{DemoAdapter, DemoProposer};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "recursa", about = "Recursa: recursive agent harnessing (Rust)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the deterministic local demo (no model credentials).
    Demo {
        #[arg(long, default_value = "runs/demo")]
        output: PathBuf,
        #[arg(long, default_value_t = 2)]
        k: usize,
    },
    /// Print framework identity and budget identities for a given k.
    Info {
        #[arg(long, default_value_t = 2)]
        k: usize,
    },
    /// Materialise a fixture task catalog for a benchmark stub (jb|lab|wb).
    Catalog {
        #[arg(long)]
        bench: String,
        #[arg(long, default_value = "datasets")]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Demo { output, k } => {
            if k == 0 {
                bail!("--k must be >= 1");
            }
            std::fs::create_dir_all(&output)?;
            let adapter = DemoAdapter::new(&output);
            let mut proposer = DemoProposer::new();
            let result = schedule::run_domain(
                "accounting",
                adapter.tasks("all"),
                k,
                &adapter,
                &mut proposer,
                &adapter.spec.runs_root,
                &output.join("library").join(k.to_string()),
            );
            println!("{}", serde_json::to_string_pretty(&result)?);
            let done = result.get("done").and_then(|v| v.as_u64()).unwrap_or(0);
            let failed = result.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
            if failed > 0 {
                bail!("demo finished with {failed} failed task(s); done={done}");
            }
            eprintln!(
                "demo ok: done={done} k={k} executions={}",
                adapter
                    .execution_count
                    .load(std::sync::atomic::Ordering::SeqCst)
            );
        }
        Commands::Info { k } => {
            println!(
                "recursa 0.1.0\nR = 4k = {}\nP = 3k = {}\nmodes = [agg/parallel, seq/sequential]",
                recursa_core::expected_rollouts(k),
                recursa_core::expected_sessions(k)
            );
            println!("substrates = [oc/opencode, cx/codex]");
            println!("benchmarks:");
            for stub in recursa_core::benchmarks::BenchmarkStub::all() {
                println!(
                    "  - {} ({}) domains={}",
                    stub.id.as_str(),
                    stub.display,
                    stub.domains.join(",")
                );
            }
        }
        Commands::Catalog { bench, output } => {
            let id = recursa_core::benchmarks::BenchmarkId::from_token(&bench)
                .ok_or_else(|| anyhow::anyhow!("unknown bench {bench}; expected jb|lab|wb"))?;
            let stub = match id {
                recursa_core::benchmarks::BenchmarkId::JobBench => {
                    recursa_core::benchmarks::BenchmarkStub::jobbench()
                }
                recursa_core::benchmarks::BenchmarkId::Lab => {
                    recursa_core::benchmarks::BenchmarkStub::lab()
                }
                recursa_core::benchmarks::BenchmarkId::WorkBuddy => {
                    recursa_core::benchmarks::BenchmarkStub::workbuddy()
                }
                recursa_core::benchmarks::BenchmarkId::Demo => {
                    bail!("demo uses recursa demo, not catalog")
                }
            };
            let root = recursa_core::datasets::dataset_root(&output, stub.id.as_str());
            let index = recursa_core::datasets::materialise_stub_dataset(&root, &stub)
                .map_err(|e| anyhow::anyhow!(e))?;
            println!(
                "catalog {} -> {} ({} tasks)",
                stub.id.as_str(),
                root.display(),
                index.tasks.len()
            );
            for t in &index.tasks {
                println!("  {} [{}]", t.id, t.domain);
            }
        }
    }
    Ok(())
}
