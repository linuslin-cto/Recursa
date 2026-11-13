//! Parallel and sequential task-level recursion (Propose–Probe–Compose).

use crate::config::{PHASE_WIDTH, TRANSPORT_ATTEMPTS};
use crate::context;
use crate::ledger::Ledger;
use crate::proposer::{Proposer, ProposerError};
use crate::spec::{resolve_prompts, BenchmarkAdapter, RunView};
use crate::tree::{self, Tree};
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const STAGE_ATTEMPTS: usize = 2;

#[derive(Debug, Clone, Serialize)]
pub struct Stage {
    pub label: String,
    pub dim: String,
    pub tree: Tree,
    pub kept_base: bool,
    pub fell_back: bool,
    pub error: String,
    pub usd: f64,
    pub attempts: usize,
    pub cached: bool,
    pub notes: String,
    pub execution_failed: bool,
}

impl Stage {
    fn fallback(label: &str, dim: &str, tree: Tree, error: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            dim: dim.into(),
            tree,
            kept_base: false,
            fell_back: true,
            error: error.into(),
            usd: 0.0,
            attempts: 0,
            cached: false,
            notes: String::new(),
            execution_failed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModeResult {
    pub mode: String,
    pub task_id: String,
    pub k: usize,
    pub stages: Vec<Stage>,
    pub trees: HashMap<String, Tree>,
    pub rollouts: HashMap<String, RunView>,
    pub finals: Vec<String>,
    pub ledger: Option<Ledger>,
    pub similarity: Option<serde_json::Value>,
    pub errors: Vec<String>,
}

impl ModeResult {
    pub fn ok(&self) -> bool {
        !self.finals.is_empty()
            && self.errors.is_empty()
            && !self.stages.iter().any(|s| s.execution_failed)
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "mode": self.mode,
            "task_id": self.task_id,
            "k": self.k,
            "finals": self.finals,
            "ok": self.ok(),
            "fell_back": self.stages.iter().filter(|s| s.fell_back).map(|s| &s.label).collect::<Vec<_>>(),
            "kept_base": self.stages.iter().filter(|s| s.kept_base).map(|s| &s.label).collect::<Vec<_>>(),
            "stages": self.stages.iter().map(|s| serde_json::json!({
                "label": s.label,
                "dim": s.dim,
                "fell_back": s.fell_back,
                "kept_base": s.kept_base,
                "execution_failed": s.execution_failed,
                "attempts": s.attempts,
                "usd": s.usd,
                "cached": s.cached,
                "notes": s.notes,
                "error": s.error.chars().take(400).collect::<String>(),
            })).collect::<Vec<_>>(),
            "similarity": self.similarity,
            "ledger": self.ledger.as_ref().map(|l| l.to_json()),
            "errors": self.errors,
        })
    }
}

fn propose_job(dim: &str, k: usize, index: usize) -> String {
    format!(
        "Propose a {dim} harness edit (candidate {index}/{k}). \
         Edit `context/harness/` in place and keep scripts executable."
    )
}

fn aggregate_job(k: usize, index: usize) -> String {
    format!(
        "Compose final harness agg{index}/{k} from candidate evidence. \
         Keep helpful edits; drop harmful ones."
    )
}

fn compose_round_job(t: usize, k: usize) -> String {
    format!("Compose round {t}/{k} harness from INV/DOM probes and prior history.")
}

fn accept_tree(
    surface: &tree::SurfaceSpec,
    adapter: &dyn BenchmarkAdapter,
    new: &Tree,
    fallback: &Tree,
) -> (bool, String) {
    let mut new = new.clone();
    let _triggered = tree::autotrigger(surface, &mut new);
    let changed = tree::diff_summary(fallback, &new);
    let kept = changed["added"].is_empty()
        && changed["removed"].is_empty()
        && changed["modified"].is_empty();
    let reserved: Vec<&str> = tree::RESERVED_SCRIPTS
        .iter()
        .copied()
        .chain(adapter.spec().reserved_scripts.iter().map(|s| s.as_str()))
        .collect();
    if let Err(e) = tree::validate(surface, &new, adapter.spec().allow_subagents, &reserved) {
        return (kept, e.to_string());
    }
    if let Err(e) = adapter.validate_tree(&new) {
        return (kept, e.to_string());
    }
    if let Err(e) = context::assert_tree_has_no_grader_verdicts(&new) {
        return (kept, e);
    }
    (kept, String::new())
}

/// Run Propose stages; fork+rayon when every stage can get an independent proposer.
fn run_stage_batch(
    specs: &[(String, String, String)],
    fallback: &Tree,
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    ctx_dir: &Path,
    task_id: &str,
    rollouts: &[RunView],
    harness_dir: Option<&Path>,
    library_dir: Option<&Path>,
    pool: &[(String, PathBuf, Option<RunView>)],
    decisions: &HashMap<String, String>,
) -> Vec<Stage> {
    let forks: Vec<Option<Box<dyn Proposer>>> =
        (0..specs.len()).map(|_| prop.fork()).collect();
    if !forks.is_empty() && forks.iter().all(|f| f.is_some()) {
        use rayon::prelude::*;
        return specs
            .par_iter()
            .zip(forks)
            .map(|((label, dim, job), mut local)| {
                run_stage(
                    label,
                    dim,
                    job,
                    fallback,
                    adapter,
                    local.as_mut().unwrap().as_mut(),
                    ctx_dir,
                    task_id,
                    rollouts,
                    harness_dir,
                    library_dir,
                    pool,
                    decisions,
                )
            })
            .collect();
    }
    let mut out = Vec::with_capacity(specs.len());
    for (label, dim, job) in specs {
        out.push(run_stage(
            label,
            dim,
            job,
            fallback,
            adapter,
            prop,
            ctx_dir,
            task_id,
            rollouts,
            harness_dir,
            library_dir,
            pool,
            decisions,
        ));
    }
    out
}

pub fn run_stage(
    label: &str,
    dim: &str,
    job: &str,
    fallback: &Tree,
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    ctx_dir: &Path,
    task_id: &str,
    rollouts: &[RunView],
    harness_dir: Option<&Path>,
    library_dir: Option<&Path>,
    pool: &[(String, PathBuf, Option<RunView>)],
    decisions: &HashMap<String, String>,
) -> Stage {
    let surface = adapter.surface();
    let tree_ctx = match context::build(
        &ctx_dir.join(label),
        adapter,
        task_id,
        rollouts,
        harness_dir,
        library_dir,
        pool,
        decisions,
    ) {
        Ok(t) => t,
        Err(exc) => {
            return Stage::fallback(
                label,
                dim,
                fallback.clone(),
                format!("could not build evidence: {exc}"),
            );
        }
    };
    let mut err = String::new();
    let mut spent_usd = 0.0;
    let mut spent_calls = 0usize;
    let mut all_cached = true;
    let mut execution_failed = false;

    for attempt in 1..=STAGE_ATTEMPTS {
        let mission = if err.is_empty() {
            context::mission(&tree_ctx, job)
        } else {
            format!(
                "{}\n\n# YOUR PREVIOUS ATTEMPT WAS REJECTED\n\n{err}\n\n\
                 Fix it in `context/harness/` and reply again.",
                context::mission(&tree_ctx, job)
            )
        };
        let reply = match prop.call(
            &mission,
            Some(&tree_ctx.root),
            &format!("{label}.a{attempt}"),
        ) {
            Ok(r) => r,
            Err(ProposerError {
                message,
                attempts,
                cost_usd,
            }) => {
                err = message;
                execution_failed = true;
                spent_calls += if attempts == 0 {
                    TRANSPORT_ATTEMPTS
                } else {
                    attempts
                };
                spent_usd += cost_usd;
                all_cached = false;
                break;
            }
        };
        spent_usd += reply.cost_usd;
        spent_calls += if reply.cached {
            0
        } else {
            reply.attempts.max(1)
        };
        all_cached = all_cached && reply.cached;

        let Some(workspace) = reply.workspace.as_ref() else {
            err = "the harness directory did not come back from the session; edit the files in `context/harness/` in place".into();
            continue;
        };
        let (new, rejected) = match tree::read_tree(&surface, workspace) {
            Ok(v) => v,
            Err(e) => {
                err = e.to_string();
                continue;
            }
        };
        if new.is_empty() {
            err = "`context/harness/` came back empty -- edit the files, do not delete them all"
                .into();
            continue;
        }
        if !rejected.is_empty() {
            err = format!(
                "these files are off the editable surface and were not applied: {}",
                rejected.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
            );
            let _ = tree::write_tree(&surface, &new, &tree_ctx.root.join("harness"));
            continue;
        }
        let (kept, accept_err) = accept_tree(&surface, adapter, &new, fallback);
        if !accept_err.is_empty() {
            err = accept_err;
            let _ = tree::write_tree(&surface, &new, &tree_ctx.root.join("harness"));
            continue;
        }
        let notes_re = Regex::new(r"(?s)<notes>(.*?)</notes>").unwrap();
        let notes = notes_re
            .captures(&reply.text)
            .map(|c| c[1].trim().to_string())
            .unwrap_or_else(|| reply.text.trim().to_string());
        return Stage {
            label: label.into(),
            dim: dim.into(),
            tree: new,
            kept_base: kept,
            fell_back: false,
            error: String::new(),
            usd: spent_usd,
            attempts: spent_calls,
            cached: all_cached,
            notes,
            execution_failed: false,
        };
    }

    Stage {
        label: label.into(),
        dim: dim.into(),
        tree: fallback.clone(),
        kept_base: false,
        fell_back: true,
        error: err.chars().take(400).collect(),
        usd: spent_usd,
        attempts: spent_calls,
        cached: all_cached,
        notes: String::new(),
        execution_failed,
    }
}

fn record_sessions(ledger: &mut Ledger, stages: &[Stage]) {
    for stage in stages {
        ledger.session(&stage.label, stage.usd, stage.cached, stage.attempts);
    }
}

fn roll(
    adapter: &dyn BenchmarkAdapter,
    ledger: &mut Ledger,
    task_id: &str,
    label: &str,
    tree: Option<&Tree>,
    run_id_of: &dyn Fn(&str) -> String,
    work_dir: &Path,
    record: bool,
) -> RunView {
    let surface = adapter.surface();
    let hdir = tree.and_then(|t| {
        tree::write_tree(&surface, t, &work_dir.join("harnesses").join(label)).ok()
    });
    let run_id = run_id_of(label);
    let rv = match adapter.execute(task_id, hdir.as_deref(), &run_id, None) {
        Ok(rd) => {
            let exists = rd.exists();
            RunView {
                label: label.into(),
                run_id: run_id.clone(),
                run_dir: rd.clone(),
                ok: exists,
                delivered: exists && adapter.delivered(&rd),
                error: if exists {
                    String::new()
                } else {
                    "adapter returned a missing run directory".into()
                },
            }
        }
        Err(exc) => RunView {
            label: label.into(),
            run_id,
            run_dir: work_dir.join("missing"),
            ok: false,
            delivered: false,
            error: exc.to_string(),
        },
    };
    if record {
        ledger.rollout(label, 0.0, false);
    }
    rv
}

fn roll_many(
    adapter: &dyn BenchmarkAdapter,
    ledger: &mut Ledger,
    task_id: &str,
    items: &[(String, Tree)],
    run_id_of: &dyn Fn(&str) -> String,
    work_dir: &Path,
    width: usize,
) -> HashMap<String, RunView> {
    use rayon::prelude::*;
    if items.is_empty() {
        return HashMap::new();
    }
    let n = width.max(1).min(items.len());
    // Precompute run ids so workers do not need the non-Sync closure.
    let planned: Vec<(String, Tree, String)> = items
        .iter()
        .map(|(label, tree)| (label.clone(), tree.clone(), run_id_of(label)))
        .collect();
    let got: Vec<RunView> = if n == 1 {
        planned
            .iter()
            .map(|(label, tree, run_id)| {
                let id_fn = |_: &str| run_id.clone();
                roll(
                    adapter,
                    ledger,
                    task_id,
                    label,
                    Some(tree),
                    &id_fn,
                    work_dir,
                    false,
                )
            })
            .collect()
    } else {
        planned
            .par_iter()
            .map(|(label, tree, run_id)| {
                let mut local = Ledger::new("tmp", task_id, 1);
                let id_fn = |_: &str| run_id.clone();
                roll(
                    adapter,
                    &mut local,
                    task_id,
                    label,
                    Some(tree),
                    &id_fn,
                    work_dir,
                    false,
                )
            })
            .collect()
    };
    let mut out = HashMap::new();
    for ((label, _), rv) in items.iter().zip(got) {
        ledger.rollout(label, 0.0, false);
        out.insert(label.clone(), rv);
    }
    out
}

pub fn run_parallel(
    task_id: &str,
    k: usize,
    base: &[RunView],
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    work_dir: &Path,
    run_id_of: &dyn Fn(&str) -> String,
    base_tree: &Tree,
    library_dir: Option<&Path>,
) -> ModeResult {
    let surface = adapter.surface();
    let mut ledger = Ledger::new("agg", task_id, k);
    for r in base {
        ledger.rollout(&r.label, 0.0, false);
    }
    let mut result = ModeResult {
        mode: "agg".into(),
        task_id: task_id.into(),
        k,
        stages: Vec::new(),
        trees: HashMap::new(),
        rollouts: HashMap::new(),
        finals: Vec::new(),
        ledger: None,
        similarity: None,
        errors: Vec::new(),
    };
    for r in base {
        result.rollouts.insert(r.label.clone(), r.clone());
    }
    let ctx_dir = work_dir.join("context");
    let _prompts = resolve_prompts(adapter, task_id);
    let base_dir = tree::write_tree(&surface, base_tree, &work_dir.join("harnesses").join("base"))
        .expect("write base harness");

    let mut specs = Vec::new();
    for i in 1..=k {
        for d in ["INV", "DOM"] {
            specs.push((format!("{}{i}", d.to_lowercase()), d.to_string(), i));
        }
    }
    let stages = run_stage_batch(
        &specs
            .iter()
            .map(|(label, dim, index)| {
                (
                    label.clone(),
                    dim.clone(),
                    propose_job(dim, k, *index),
                )
            })
            .collect::<Vec<_>>(),
        base_tree,
        adapter,
        prop,
        &ctx_dir,
        task_id,
        base,
        Some(&base_dir),
        library_dir,
        &[],
        &HashMap::new(),
    );
    record_sessions(&mut ledger, &stages);
    for stage in &stages {
        result.trees.insert(stage.label.clone(), stage.tree.clone());
    }
    result.stages.extend(stages.clone());
    let items: Vec<_> = stages
        .iter()
        .map(|s| (s.label.clone(), s.tree.clone()))
        .collect();
    result.rollouts.extend(roll_many(
        adapter,
        &mut ledger,
        task_id,
        &items,
        run_id_of,
        work_dir,
        PHASE_WIDTH,
    ));

    let pool_ev: Vec<_> = stages
        .iter()
        .map(|stage| {
            (
                stage.label.clone(),
                work_dir.join("harnesses").join(&stage.label),
                result.rollouts.get(&stage.label).cloned(),
            )
        })
        .collect();
    let decisions: HashMap<_, _> = stages
        .iter()
        .map(|s| {
            (
                s.label.clone(),
                if s.notes.is_empty() {
                    s.error.clone()
                } else {
                    s.notes.clone()
                },
            )
        })
        .collect();

    let agg_specs: Vec<_> = (1..=k)
        .map(|i| (format!("agg{i}"), "AGG".to_string(), aggregate_job(k, i)))
        .collect();
    let aggs = run_stage_batch(
        &agg_specs,
        base_tree,
        adapter,
        prop,
        &ctx_dir,
        task_id,
        base,
        Some(&base_dir),
        library_dir,
        &pool_ev,
        &decisions,
    );
    record_sessions(&mut ledger, &aggs);
    for stage in &aggs {
        result.trees.insert(stage.label.clone(), stage.tree.clone());
        result.finals.push(stage.label.clone());
    }
    result.stages.extend(aggs.clone());
    let items: Vec<_> = aggs
        .iter()
        .map(|s| (s.label.clone(), s.tree.clone()))
        .collect();
    result.rollouts.extend(roll_many(
        adapter,
        &mut ledger,
        task_id,
        &items,
        run_id_of,
        work_dir,
        PHASE_WIDTH,
    ));

    let sim_map: std::collections::BTreeMap<_, _> = aggs
        .iter()
        .map(|s| (s.label.clone(), s.tree.clone()))
        .collect();
    result.similarity = Some(tree::diversity(&sim_map));
    ledger.assert_identity().expect("parallel ledger identity");
    result.ledger = Some(ledger);
    result
}

pub fn run_sequential(
    task_id: &str,
    k: usize,
    base: &[RunView],
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    work_dir: &Path,
    run_id_of: &dyn Fn(&str) -> String,
    base_tree: &Tree,
    library_dir: Option<&Path>,
) -> ModeResult {
    let surface = adapter.surface();
    let mut ledger = Ledger::new("seq", task_id, k);
    for r in base {
        ledger.rollout(&r.label, 0.0, false);
    }
    let mut result = ModeResult {
        mode: "seq".into(),
        task_id: task_id.into(),
        k,
        stages: Vec::new(),
        trees: HashMap::new(),
        rollouts: HashMap::new(),
        finals: Vec::new(),
        ledger: None,
        similarity: None,
        errors: Vec::new(),
    };
    for r in base {
        result.rollouts.insert(r.label.clone(), r.clone());
    }
    let ctx_dir = work_dir.join("context");
    let _prompts = resolve_prompts(adapter, task_id);

    let mut current_harness = base_tree.clone();
    let mut history: Vec<(String, PathBuf, Option<RunView>)> = Vec::new();

    for t in 1..=k {
        let current_label = if t > 1 {
            format!("c{}", t - 1)
        } else {
            "base".into()
        };
        let current_dir = tree::write_tree(
            &surface,
            &current_harness,
            &work_dir.join("harnesses").join(&current_label),
        )
        .expect("write current harness");

        let mut stages = Vec::new();
        for (label, dim) in [(format!("inv{t}"), "INV"), (format!("dom{t}"), "DOM")] {
            let decisions: HashMap<_, _> = result
                .stages
                .iter()
                .map(|s| {
                    (
                        s.label.clone(),
                        if s.notes.is_empty() {
                            s.error.clone()
                        } else {
                            s.notes.clone()
                        },
                    )
                })
                .collect();
            let stage = run_stage(
                &label,
                dim,
                &propose_job(dim, k, t),
                &current_harness,
                adapter,
                prop,
                &ctx_dir,
                task_id,
                base,
                Some(&current_dir),
                library_dir,
                &history,
                &decisions,
            );
            stages.push(stage);
        }
        record_sessions(&mut ledger, &stages);
        for stage in &stages {
            result.trees.insert(stage.label.clone(), stage.tree.clone());
        }
        result.stages.extend(stages.clone());
        let items: Vec<_> = stages
            .iter()
            .map(|s| (s.label.clone(), s.tree.clone()))
            .collect();
        result.rollouts.extend(roll_many(
            adapter,
            &mut ledger,
            task_id,
            &items,
            run_id_of,
            work_dir,
            PHASE_WIDTH,
        ));

        let mut round_pool = history.clone();
        for stage in &stages {
            round_pool.push((
                stage.label.clone(),
                work_dir.join("harnesses").join(&stage.label),
                result.rollouts.get(&stage.label).cloned(),
            ));
        }
        let ck = format!("c{t}");
        let comp_job = if k == 1 {
            aggregate_job(1, 1)
        } else {
            compose_round_job(t, k)
        };
        let decisions: HashMap<_, _> = result
            .stages
            .iter()
            .map(|s| {
                (
                    s.label.clone(),
                    if s.notes.is_empty() {
                        s.error.clone()
                    } else {
                        s.notes.clone()
                    },
                )
            })
            .collect();
        let comp = run_stage(
            &ck,
            "COMPOSE",
            &comp_job,
            &current_harness,
            adapter,
            prop,
            &ctx_dir,
            task_id,
            base,
            Some(&current_dir),
            library_dir,
            &round_pool,
            &decisions,
        );
        record_sessions(&mut ledger, std::slice::from_ref(&comp));
        result.trees.insert(ck.clone(), comp.tree.clone());
        let rolled = roll(
            adapter,
            &mut ledger,
            task_id,
            &ck,
            Some(&comp.tree),
            run_id_of,
            work_dir,
            true,
        );
        result.rollouts.insert(ck.clone(), rolled.clone());
        history = round_pool;
        history.push((ck.clone(), work_dir.join("harnesses").join(&ck), Some(rolled)));
        current_harness = comp.tree.clone();
        result.stages.push(comp);
    }

    result.finals = (1..=k).map(|i| format!("c{i}")).collect();
    ledger.assert_identity().expect("sequential ledger identity");
    result.ledger = Some(ledger);
    result
}

pub fn run_mode(
    mode: &str,
    task_id: &str,
    k: usize,
    base: &[RunView],
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    work_dir: &Path,
    run_id_of: &dyn Fn(&str) -> String,
    base_tree: &Tree,
    library_dir: Option<&Path>,
) -> ModeResult {
    match mode {
        "agg" => run_parallel(
            task_id,
            k,
            base,
            adapter,
            prop,
            work_dir,
            run_id_of,
            base_tree,
            library_dir,
        ),
        "seq" => run_sequential(
            task_id,
            k,
            base,
            adapter,
            prop,
            work_dir,
            run_id_of,
            base_tree,
            library_dir,
        ),
        _ => panic!("unknown mode {mode:?}; expected 'agg' or 'seq'"),
    }
}
