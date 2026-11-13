//! Namespacing, stream order, and drivers: one task, one domain, a sweep.

use crate::library::Library;
use crate::proposer::Proposer;
use crate::recursion::{self, ModeResult};
use crate::runmark;
use crate::selection::{self, Selection};
use crate::spec::{BenchmarkAdapter, RunView};
use crate::tree::{self, Tree};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const MODES: &[&str] = &["agg", "seq"];

/// Test / automation hooks. Empty in production. Thread-local so parallel tests do not collide.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// If set, every recursion mode fails with this error (after bases).
    pub force_mode_error: Option<String>,
    /// Task ids for which `Library::add` should fail.
    pub library_fail_tasks: Vec<String>,
}

thread_local! {
    static RUN_OPTIONS: RefCell<RunOptions> = const { RefCell::new(RunOptions {
        force_mode_error: None,
        library_fail_tasks: Vec::new(),
    }) };
}

pub fn with_run_options<R>(opts: RunOptions, f: impl FnOnce() -> R) -> R {
    RUN_OPTIONS.with(|cell| {
        let previous = cell.replace(opts);
        let out = f();
        cell.replace(previous);
        out
    })
}

fn current_options() -> RunOptions {
    RUN_OPTIONS.with(|cell| cell.borrow().clone())
}

pub fn mode_token(name: &str) -> Result<&'static str, String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "agg" | "parallel" => Ok("agg"),
        "seq" | "sequential" => Ok("seq"),
        other => Err(format!(
            "unknown mode {other:?}; expected parallel | sequential (or {MODES:?})"
        )),
    }
}

#[derive(Debug, Clone)]
pub struct Namespace {
    pub benchmark: String,
    pub substrate: String,
    pub mode: String,
    pub k: usize,
    pub seed: Option<u64>,
    pub tag: String,
}

impl Namespace {
    pub fn as_str(&self) -> String {
        ns(
            &self.benchmark,
            &self.substrate,
            &self.mode,
            self.k,
            self.seed,
            &self.tag,
        )
    }

    pub fn run_id(&self, task_id: &str, label: &str) -> String {
        format!("{}/{task_id}/{label}", self.as_str())
    }

    pub fn task_dir(&self, root: &Path, task_id: &str) -> PathBuf {
        root.join(self.as_str())
            .join(task_id.replace('/', "__"))
    }
}

/// Continual-learning stream: `(domain, task_id)` pairs.
///
/// `seed=None` keeps `reference_domain_order` then sorted tasks. An integer seed
/// shuffles domains and per-domain tasks with independent LCG streams so dropping
/// one domain does not reshuffle the others.
pub fn stream_order(
    tasks_by_domain: &HashMap<String, Vec<String>>,
    seed: Option<u64>,
    reference_domain_order: &[String],
) -> Result<Vec<(String, String)>, String> {
    let mut domains: Vec<String> = tasks_by_domain.keys().cloned().collect();
    domains.sort();
    let ordered_domains = if let Some(seed) = seed {
        let mut d = domains;
        shuffle_with_seed(&mut d, mix_seed(seed, "__domains__"));
        d
    } else {
        let ref_set: std::collections::BTreeSet<_> =
            reference_domain_order.iter().cloned().collect();
        let missing: Vec<_> = domains
            .iter()
            .filter(|d| !ref_set.contains(*d))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "reference_domain_order does not cover domain(s) {missing:?}"
            ));
        }
        reference_domain_order
            .iter()
            .filter(|d| tasks_by_domain.contains_key(*d))
            .cloned()
            .collect()
    };
    let mut out = Vec::new();
    for domain in ordered_domains {
        let mut tasks = tasks_by_domain.get(&domain).cloned().unwrap_or_default();
        tasks.sort();
        if let Some(seed) = seed {
            shuffle_with_seed(&mut tasks, mix_seed(seed, &domain));
        }
        for task_id in tasks {
            out.push((domain.clone(), task_id));
        }
    }
    Ok(out)
}

fn mix_seed(seed: u64, label: &str) -> u64 {
    let mut h = seed;
    for b in label.as_bytes() {
        h = h
            .wrapping_mul(1315423911)
            .wrapping_add(u64::from(*b))
            .wrapping_add(1);
    }
    h
}

fn shuffle_with_seed<T>(items: &mut [T], seed: u64) {
    let mut state = seed;
    for i in (1..items.len()).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        let j = (state as usize) % (i + 1);
        items.swap(i, j);
    }
}

pub fn ns(
    benchmark: &str,
    substrate: &str,
    mode: &str,
    k: usize,
    seed: Option<u64>,
    tag: &str,
) -> String {
    assert!(
        MODES.contains(&mode),
        "unknown mode {mode:?}; the method has exactly two: {MODES:?}"
    );
    assert!(
        !substrate.is_empty(),
        "namespace needs a substrate name ('oc' | 'cx')"
    );
    let mut out = format!("{benchmark}-{substrate}-{mode}{k}");
    if let Some(s) = seed {
        out.push_str(&format!("-s{s}"));
    }
    if !tag.is_empty() {
        out.push('-');
        out.push_str(tag);
    }
    out
}

fn load_base_tree(adapter: &dyn BenchmarkAdapter) -> Tree {
    let surface = adapter.surface();
    tree::read_tree(&surface, &adapter.base_harness())
        .map(|(t, _)| t)
        .unwrap_or_default()
}

fn write_summary(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, serde_json::to_string_pretty(value).unwrap_or_default());
}

fn read_summary(path: &Path) -> Option<Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn selection_still_valid(adapter: &dyn BenchmarkAdapter, sel: &Value) -> bool {
    let Some(run_dir) = sel.get("chosen_run_dir").and_then(|v| v.as_str()) else {
        return false;
    };
    let path = Path::new(run_dir);
    sel.get("ok").and_then(|v| v.as_bool()).unwrap_or(false)
        && !sel
            .get("chosen_label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        && adapter.delivered(path)
}

fn harness_matches_selection(work: &Path, sel: &Value) -> bool {
    let Some(label) = sel.get("chosen_label").and_then(|v| v.as_str()) else {
        return false;
    };
    work.join("harnesses").join(label).is_dir()
}

fn task_inputs(adapter: &dyn BenchmarkAdapter, task_id: &str) -> Option<String> {
    let prompt = adapter.task_prompt(task_id);
    let files = adapter.task_files(task_id);
    Some(runmark::inputs_fingerprint(
        &prompt,
        if files.exists() { Some(files.as_path()) } else { None },
    ))
}

fn empty_selection(task_id: &str, mode: &str, k: usize, error: &str, usd: f64) -> Selection {
    Selection {
        task_id: task_id.into(),
        mode: mode.into(),
        k,
        error: error.into(),
        usd,
        ok: false,
        chosen_label: String::new(),
        chosen_run_dir: String::new(),
        selected_at: chrono::Utc::now().to_rfc3339(),
        evidence: "deliverable".into(),
        ..Default::default()
    }
}

fn failed_mode(mode: &str, task_id: &str, k: usize, error: &str) -> ModeResult {
    ModeResult {
        mode: mode.into(),
        task_id: task_id.into(),
        k,
        stages: Vec::new(),
        trees: HashMap::new(),
        rollouts: HashMap::new(),
        finals: Vec::new(),
        ledger: None,
        similarity: None,
        errors: vec![error.into()],
    }
}

fn persist_all_summaries(
    adapter: &dyn BenchmarkAdapter,
    task_id: &str,
    k: usize,
    runs_root: &Path,
    summary: &Value,
) {
    for mode in MODES {
        let ns = Namespace {
            benchmark: adapter.spec().name.clone(),
            substrate: "oc".into(),
            mode: (*mode).into(),
            k,
            seed: None,
            tag: String::new(),
        };
        write_summary(
            &ns.task_dir(runs_root, task_id).join("summary.json"),
            summary,
        );
    }
}

pub fn run_task(
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    task_id: &str,
    k: usize,
    runs_root: &Path,
    library_root: &Path,
) -> Value {
    run_task_with_selector(adapter, prop, None, task_id, k, runs_root, library_root, MODES)
}

pub fn run_task_with_selector(
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    mut selector: Option<&mut dyn Proposer>,
    task_id: &str,
    k: usize,
    runs_root: &Path,
    library_root: &Path,
    requested_modes: &[&str],
) -> Value {
    let opts = current_options();
    let substrate = "oc";
    let base_tree = load_base_tree(adapter);
    let mut mode_results: HashMap<String, ModeResult> = HashMap::new();
    let mut selections: HashMap<String, Selection> = HashMap::new();
    let mut status = "ok".to_string();
    let mut errors: Vec<String> = Vec::new();
    let modes: Vec<&str> = if requested_modes.is_empty() {
        MODES.to_vec()
    } else {
        requested_modes
            .iter()
            .copied()
            .filter(|m| MODES.contains(m))
            .collect()
    };

    let primary_ns = Namespace {
        benchmark: adapter.spec().name.clone(),
        substrate: substrate.into(),
        mode: "agg".into(),
        k,
        seed: None,
        tag: String::new(),
    };
    let work_agg = primary_ns.task_dir(runs_root, task_id);
    let summary_path = work_agg.join("summary.json");
    let inputs = task_inputs(adapter, task_id);
    let previous_summary = read_summary(&summary_path);
    if let Some(prev) = previous_summary.as_ref() {
        let inputs_changed = match (&inputs, prev.get("inputs").and_then(|v| v.as_str())) {
            (Some(now), Some(old)) => now != old,
            _ => false,
        };
        if !inputs_changed {
            let prev_modes = prev.get("modes").cloned().unwrap_or(json!({}));
            let sels = prev.get("selections").cloned().unwrap_or(json!({}));
            let mut complete = true;
            for mode in &modes {
                let ok = prev_modes
                    .get(*mode)
                    .and_then(|m| m.get("ok"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let sel = sels.get(*mode).cloned().unwrap_or(json!({}));
                let work = Namespace {
                    benchmark: adapter.spec().name.clone(),
                    substrate: substrate.into(),
                    mode: (*mode).into(),
                    k,
                    seed: None,
                    tag: String::new(),
                }
                .task_dir(runs_root, task_id);
                if !(ok
                    && selection_still_valid(adapter, &sel)
                    && harness_matches_selection(&work, &sel))
                {
                    complete = false;
                    break;
                }
            }
            // Resume when every *requested* mode is healthy — even if overall status was partial.
            if complete {
                return json!({
                    "task_id": task_id,
                    "status": "skipped",
                    "summary": prev,
                });
            }
        }
        // If inputs changed, do not reuse prior mode/selection records.
    }

    // Shared base rollouts (Python: one base set for all modes under agg namespace).
    let _ = fs::create_dir_all(&work_agg);
    let mut base = Vec::new();
    for i in 1..=k {
        let label = format!("base{i}");
        let run_id = primary_ns.run_id(task_id, &label);
        match adapter.execute(task_id, Some(&adapter.base_harness()), &run_id, None) {
            Ok(rd) => {
                let exists = rd.exists();
                base.push(RunView {
                    label: label.clone(),
                    run_id,
                    run_dir: rd.clone(),
                    ok: exists,
                    delivered: exists && adapter.delivered(&rd),
                    error: String::new(),
                });
            }
            Err(e) => {
                errors.push(e.to_string());
                for mode in MODES {
                    mode_results.insert(
                        (*mode).into(),
                        failed_mode(mode, task_id, k, "base-failed"),
                    );
                    selections.insert(
                        (*mode).into(),
                        empty_selection(task_id, mode, k, "base-failed", 0.0),
                    );
                }
                let failed = json!({
                    "task_id": task_id,
                    "status": "base-failed",
                    "k": k,
                    "modes": {
                        "agg": {"ok": false, "error": "base-failed"},
                        "seq": {"ok": false, "error": "base-failed"},
                    },
                    "selections": {
                        "agg": selections.get("agg"),
                        "seq": selections.get("seq"),
                    },
                    "errors": errors,
                });
                persist_all_summaries(adapter, task_id, k, runs_root, &failed);
                return json!({
                    "task_id": task_id,
                    "status": "base-failed",
                    "summary": failed,
                });
            }
        }
    }
    if !base.iter().any(|b| b.ok) {
        let err = "no base rollout produced anything";
        for mode in MODES {
            mode_results.insert((*mode).into(), failed_mode(mode, task_id, k, err));
            selections.insert(
                (*mode).into(),
                empty_selection(task_id, mode, k, err, 0.0),
            );
        }
        let failed = json!({
            "task_id": task_id,
            "status": "base-failed",
            "k": k,
            "modes": {
                "agg": {"ok": false, "error": err},
                "seq": {"ok": false, "error": err},
            },
            "selections": {
                "agg": selections.get("agg"),
                "seq": selections.get("seq"),
            },
            "errors": [err],
        });
        persist_all_summaries(adapter, task_id, k, runs_root, &failed);
        return json!({
            "task_id": task_id,
            "status": "base-failed",
            "summary": failed,
        });
    }

    for mode in &modes {
        let ns = Namespace {
            benchmark: adapter.spec().name.clone(),
            substrate: substrate.into(),
            mode: (*mode).into(),
            k,
            seed: None,
            tag: String::new(),
        };
        let work = ns.task_dir(runs_root, task_id);
        let _ = fs::create_dir_all(&work);
        let run_id_of = |label: &str| ns.run_id(task_id, label);

        if let Some(err) = opts.force_mode_error.clone() {
            let msg = format!("RuntimeError: {err}");
            errors.push(format!("{mode}: {msg}"));
            mode_results.insert((*mode).into(), failed_mode(mode, task_id, k, &msg));
            selections.insert(
                (*mode).into(),
                empty_selection(task_id, mode, k, &msg, 0.0),
            );
            status = "partial".into();
            continue;
        }

        let domain = adapter.domain_of(task_id);
        let library = Library::new(library_root.join(mode), &domain, adapter.surface());
        let library_ctx = library.context_dir();

        let result = recursion::run_mode(
            mode,
            task_id,
            k,
            &base,
            adapter,
            prop,
            &work,
            &run_id_of,
            &base_tree,
            library_ctx.as_deref(),
        );

        for stage in &result.stages {
            if stage.execution_failed {
                errors.push(format!(
                    "{mode}: {}: proposer execution failed: {}",
                    stage.label, stage.error
                ));
            }
        }

        if !result.ok() {
            status = "partial".into();
        }

        let select_prop: &mut dyn Proposer = match selector.as_mut() {
            Some(s) => *s,
            None => prop,
        };
        let mut sel = selection::run_selection(
            adapter,
            select_prop,
            task_id,
            mode,
            k,
            &work,
            &result.rollouts,
        );
        if sel.ok {
            selection::reflect_adoption(
                adapter,
                select_prop,
                task_id,
                &mut sel,
                &result.rollouts,
                &result.stages,
                &work,
            );
            if let Some(tree) = result.trees.get(&sel.chosen_label) {
                let should_fail = opts.library_fail_tasks.iter().any(|t| t == task_id);
                let add_result = if should_fail {
                    Err("library write failed".into())
                } else {
                    library.add(task_id, tree, &sel.experience, &sel.why)
                };
                if let Err(e) = add_result {
                    status = "partial".into();
                    errors.push(format!("{mode}: {e}"));
                    let usd = sel.usd;
                    mode_results.insert(
                        (*mode).into(),
                        failed_mode(mode, task_id, k, &e),
                    );
                    selections.insert(
                        (*mode).into(),
                        empty_selection(task_id, mode, k, &e, usd),
                    );
                    continue;
                }
            }
        } else if status == "ok" {
            status = "partial".into();
        }

        mode_results.insert((*mode).into(), result);
        selections.insert((*mode).into(), sel);
    }

    // Preserve unrequested mode records only when task inputs are unchanged.
    let mut modes_json = json!({});
    let mut sels_json = json!({});
    let inputs_changed = match (&inputs, previous_summary.as_ref().and_then(|p| p.get("inputs")).and_then(|v| v.as_str())) {
        (Some(now), Some(old)) => now.as_str() != old,
        _ => false,
    };
    if let Some(prev) = previous_summary.as_ref() {
        if !inputs_changed {
            if let Some(obj) = prev.get("modes").and_then(|v| v.as_object()) {
                for (k, v) in obj {
                    modes_json[k] = v.clone();
                }
            }
            if let Some(obj) = prev.get("selections").and_then(|v| v.as_object()) {
                for (k, v) in obj {
                    sels_json[k] = v.clone();
                }
            }
            if let Some(arr) = prev.get("errors").and_then(|v| v.as_array()) {
                let mut kept = Vec::new();
                for e in arr {
                    let s = e.as_str().unwrap_or("");
                    let drop = modes.iter().any(|m| s.starts_with(&format!("{m}:")));
                    if !drop {
                        kept.push(s.to_string());
                    }
                }
                kept.append(&mut errors);
                errors = kept;
            }
        }
    }
    for mode in &modes {
        if let Some(r) = mode_results.get(*mode) {
            modes_json[*mode] = r.to_json();
        }
        if let Some(s) = selections.get(*mode) {
            sels_json[*mode] = serde_json::to_value(s).unwrap_or(json!({}));
        }
    }
    // Overall status: ok only if every present selection is ok.
    let all_sel_ok = ["agg", "seq"].iter().all(|m| {
        sels_json
            .get(*m)
            .and_then(|s| s.get("ok"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    });
    if status == "ok" && !all_sel_ok {
        status = "partial".into();
    }

    let summary = json!({
        "task_id": task_id,
        "status": status,
        "k": k,
        "inputs": inputs,
        "modes": modes_json,
        "selections": sels_json,
        "errors": errors,
    });
    persist_all_summaries(adapter, task_id, k, runs_root, &summary);

    json!({
        "task_id": task_id,
        "status": status,
        "summary": summary,
    })
}

pub fn run_domain(
    domain: &str,
    tasks: Vec<String>,
    k: usize,
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    runs_root: &Path,
    library_root: &Path,
) -> Value {
    run_domain_with_selector(domain, tasks, k, adapter, prop, None, runs_root, library_root, MODES)
}

pub fn run_domain_with_selector(
    domain: &str,
    tasks: Vec<String>,
    k: usize,
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    mut selector: Option<&mut dyn Proposer>,
    runs_root: &Path,
    library_root: &Path,
    requested_modes: &[&str],
) -> Value {
    let _ = domain;
    let mut rows = Vec::new();
    let mut done = 0usize;
    let mut failed = 0usize;
    for task_id in tasks {
        let row = match selector.as_mut() {
            Some(sel) => run_task_with_selector(
                adapter,
                prop,
                Some(*sel),
                &task_id,
                k,
                runs_root,
                library_root,
                requested_modes,
            ),
            None => run_task_with_selector(
                adapter,
                prop,
                None,
                &task_id,
                k,
                runs_root,
                library_root,
                requested_modes,
            ),
        };
        let status = row
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("failed")
            .to_string();
        match status.as_str() {
            "ok" | "skipped" => done += 1,
            _ => failed += 1,
        }
        rows.push(json!({"task_id": task_id, "status": status}));
    }
    json!({
        "done": done,
        "failed": failed,
        "tasks": rows,
    })
}
