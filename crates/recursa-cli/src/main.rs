//! Recursa CLI — demo, catalog, and offline score report.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use recursa_core::grader::{Grader, LedgerTotalGrader};
use recursa_core::judge::{self, JudgeRow};
use recursa_core::report::{self, Scored};
use recursa_core::schedule;
use recursa_core::BenchmarkAdapter;
use recursa_demo::{expected_signed_total, DemoAdapter, DemoProposer};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
    /// Judge + report demo runs: base vs refined scores (self-evolution gain).
    Score {
        #[arg(long, default_value = "runs/demo")]
        output: PathBuf,
        #[arg(long, default_value_t = 1)]
        k: usize,
        /// Skip re-running the demo (score existing artifacts only).
        #[arg(long, default_value_t = false)]
        skip_run: bool,
    },
    /// Run schedule over a catalog dataset (jb|lab|wb dry stub).
    Run {
        #[arg(long)]
        bench: String,
        #[arg(long, default_value = "datasets")]
        dataset: PathBuf,
        #[arg(long, default_value = "runs/catalog")]
        output: PathBuf,
        #[arg(long, default_value_t = 1)]
        k: usize,
        #[arg(long, default_value_t = 0)]
        max_tasks: usize,
        #[arg(long)]
        seed: Option<u64>,
    },
}

fn grade_demo_selection(task_id: &str, run_dir: &Path) -> Result<JudgeRow, String> {
    let expected = expected_signed_total(task_id)
        .ok_or_else(|| format!("no expected total for {task_id}"))?
        .to_string();
    let g = LedgerTotalGrader { expected }.grade(run_dir)?;
    Ok(JudgeRow {
        task_id: task_id.into(),
        mode: String::new(),
        k: 0,
        run_id: String::new(),
        selected_at: String::new(),
        graded: true,
        n_passed: Some(g.n_passed),
        n_criteria: Some(g.n_criteria),
        error: String::new(),
    })
}

fn score_demo(output: &Path, k: usize) -> Result<()> {
    let adapter = DemoAdapter::new(output);
    let ns_dir = adapter.spec.runs_root.join(format!("demo-oc-agg{k}"));
    if !ns_dir.is_dir() {
        bail!("missing runs at {}; pass --run (default) or run `recursa demo` first", ns_dir.display());
    }

    let sels = judge::collect_selections(&ns_dir, &["agg", "seq"]);
    let mut refined = Vec::new();
    for sel in &sels {
        let task_id = sel.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
        let mode = sel.get("mode").and_then(|v| v.as_str()).unwrap_or("");
        let label = sel.get("chosen_label").and_then(|v| v.as_str()).unwrap_or("");
        let run_id = sel.get("chosen_run_id").and_then(|v| v.as_str()).unwrap_or("");
        let run_dir = sel
            .get("chosen_run_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let chosen_is_base = sel
            .get("chosen_is_base")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let ok = sel.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok || run_dir.is_empty() {
            refined.push(Scored {
                task_id: task_id.into(),
                mode: mode.into(),
                k,
                label: label.into(),
                run_id: run_id.into(),
                chosen_is_base,
                graded: false,
                n_passed: None,
                n_criteria: None,
                all_pass: false,
                error: "selection not judgeable".into(),
            });
            continue;
        }
        match grade_demo_selection(task_id, Path::new(run_dir)) {
            Ok(mut row) => {
                row.mode = mode.into();
                row.k = k;
                row.run_id = run_id.into();
                refined.push(Scored::from_judge_row(&row, label, chosen_is_base));
            }
            Err(e) => refined.push(Scored {
                task_id: task_id.into(),
                mode: mode.into(),
                k,
                label: label.into(),
                run_id: run_id.into(),
                chosen_is_base,
                graded: false,
                n_passed: None,
                n_criteria: None,
                all_pass: false,
                error: e,
            }),
        }
    }

    // Base reference executions (shared under agg namespace).
    let mut base_rows = Vec::new();
    for task_id in adapter.tasks("all") {
        let run_id = format!("demo-oc-agg{k}/{task_id}/base1");
        let run_dir = output.join("executions").join(&run_id);
        for mode in ["agg", "seq"] {
            match grade_demo_selection(&task_id, &run_dir) {
                Ok(mut row) => {
                    row.mode = mode.into();
                    row.k = k;
                    row.run_id = run_id.clone();
                    base_rows.push(Scored::from_judge_row(&row, "base1", true));
                }
                Err(e) => base_rows.push(Scored {
                    task_id: task_id.clone(),
                    mode: mode.into(),
                    k,
                    label: "base1".into(),
                    run_id: run_id.clone(),
                    chosen_is_base: true,
                    graded: false,
                    n_passed: None,
                    n_criteria: None,
                    all_pass: false,
                    error: e,
                }),
            }
        }
    }

    let empty_fb = BTreeMap::new();
    let empty_usd = BTreeMap::new();
    let refined_sum = report::summarise(&refined, &empty_fb, &empty_usd);
    let base_sum = report::summarise(&base_rows, &empty_fb, &empty_usd);
    let gain = report::gains(&base_sum, &refined_sum);

    println!(
        "{}",
        report::render(
            &refined_sum,
            &format!("Recursa demo-oc refined k={k}"),
            "demo"
        )
    );
    println!();
    println!("{}", report::render_gains(&gain));
    println!();
    println!("Paper reference gains (orientation only; model-backed benches):");
    for (name, pp) in report::paper_reference_gains() {
        println!("  - {name}: up to +{pp} pp");
    }

    let judged_path = ns_dir.join("judged.json");
    let payload = json!({
        "refined": refined,
        "base": base_rows,
        "summaries": refined_sum,
        "base_summaries": base_sum,
        "gains": gain,
    });
    std::fs::write(&judged_path, serde_json::to_string_pretty(&payload)?)?;
    eprintln!("wrote {}", judged_path.display());

    // Self-evolution gate: refined must beat base on every reported mode.
    for g in &gain {
        let d = g.delta_pp.unwrap_or(-1.0);
        if d <= 0.0 {
            bail!(
                "self-evolution failed for {} k={}: Δ={d:?} (base={:?} refined={:?})",
                g.mode,
                g.k,
                g.base_score,
                g.refined_score
            );
        }
    }
    Ok(())
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
        Commands::Score { output, k, skip_run } => {
            if k == 0 {
                bail!("--k must be >= 1");
            }
            if !skip_run {
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
                let failed = result.get("failed").and_then(|v| v.as_u64()).unwrap_or(0);
                if failed > 0 {
                    bail!("demo failed before score: {failed}");
                }
            }
            score_demo(&output, k)?;
        }
        Commands::Run {
            bench,
            dataset,
            output,
            k,
            max_tasks,
            seed,
        } => {
            if k == 0 {
                bail!("--k must be >= 1");
            }
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
                    bail!("demo uses recursa demo, not run")
                }
            };
            let data_root = recursa_core::datasets::dataset_root(&dataset, stub.id.as_str());
            std::fs::create_dir_all(&output)?;
            let adapter = recursa_core::catalog::CatalogAdapter::open(&output, &data_root, stub)
                .map_err(|e| anyhow::anyhow!(e))?;
            let mut proposer = recursa_core::catalog::CatalogProposer::new();
            let by = adapter.tasks_by_domain();
            let stream = schedule::stream_order(&by, seed.or(Some(1)), &[])
                .map_err(|e| anyhow::anyhow!(e))?;
            let mut done = 0usize;
            let mut failed = 0usize;
            let mut rows = Vec::new();
            let limit = if max_tasks == 0 {
                stream.len()
            } else {
                max_tasks
            };
            for (domain, task_id) in stream.into_iter().take(limit) {
                let row = schedule::run_task(
                    &adapter,
                    &mut proposer,
                    &task_id,
                    k,
                    &adapter.spec.runs_root,
                    &output.join("library").join(k.to_string()),
                );
                let status = row
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("failed")
                    .to_string();
                if status == "ok" || status == "skipped" {
                    done += 1;
                } else {
                    failed += 1;
                }
                rows.push(json!({"domain": domain, "task_id": task_id, "status": status}));
            }
            let result = json!({"done": done, "failed": failed, "tasks": rows});
            println!("{}", serde_json::to_string_pretty(&result)?);
            if failed > 0 {
                bail!("catalog run finished with {failed} failed task(s); done={done}");
            }
            eprintln!(
                "run ok: bench={} done={done} k={k} executions={}",
                adapter.spec.name,
                adapter
                    .execution_count
                    .load(std::sync::atomic::Ordering::SeqCst)
            );
        }
    }
    Ok(())
}
