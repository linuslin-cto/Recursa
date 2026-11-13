//! Deterministic local adapter for exercising the real adaptation workflow.
//!
//! Deterministic local demo: buggy positive-only ledger sum as the base harness,
//! plus a scripted proposer that drops the filter so signed totals match.

use recursa_core::proposer::{Proposer, ProposerError, Reply};
use recursa_core::spec::{
    BenchPrompts, BenchSpec, BenchmarkAdapter, ChannelSpec, SpecError,
};
use recursa_core::tree::{self, SurfaceSpec, Tree};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub const BASE_SCRIPT: &str = r#"import csv
import sys
from decimal import Decimal

with open(sys.argv[1]) as source:
    amounts = [Decimal(row['amount']) for row in csv.DictReader(source)]
total = sum((amount for amount in amounts if amount > 0), Decimal(0))
print(f'Total: {total:.2f}')
"#;

/// Interpret the demo summarize script without requiring a Python runtime.
fn run_summarize_logic(script: &str, ledger_csv: &str) -> Result<String, String> {
    let positive_only = script.contains("if amount > 0");
    let mut total = rust_decimal_total(ledger_csv, positive_only)?;
    // Format like Python Decimal:.2f
    let sign = if total < 0.0 {
        total = -total;
        "-"
    } else {
        ""
    };
    Ok(format!("Total: {sign}{total:.2}\n"))
}

fn rust_decimal_total(ledger_csv: &str, positive_only: bool) -> Result<f64, String> {
    let mut lines = ledger_csv.lines();
    let header = lines.next().unwrap_or("");
    if !header.to_ascii_lowercase().contains("amount") {
        return Err("ledger.csv missing amount header".into());
    }
    let mut total = 0.0;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let amount: f64 = line
            .parse()
            .map_err(|_| format!("bad amount {line:?}"))?;
        if positive_only && amount <= 0.0 {
            continue;
        }
        total += amount;
    }
    // Quantize to cents to match Decimal banker's behaviour for these fixtures.
    Ok((total * 100.0).round() / 100.0)
}

pub struct DemoAdapter {
    pub root: PathBuf,
    pub spec: BenchSpec,
    surface_spec: SurfaceSpec,
    pub execution_count: AtomicUsize,
    score_hook: Arc<Mutex<Option<Box<dyn Fn(&Path) -> Option<HashMap<String, serde_json::Value>> + Send>>>>,
    execute_hook: Arc<Mutex<Option<Box<dyn Fn(&str) -> Result<(), String> + Send>>>>,
}

impl DemoAdapter {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let channel = ChannelSpec::new("opencode", "local/deterministic").expect("channel");
        let mut channels = HashMap::new();
        channels.insert("proposer".into(), channel.clone());
        channels.insert("selector".into(), channel);
        let spec = BenchSpec {
            name: "demo".into(),
            runs_root: root.join("runs"),
            install_mode: "workspace_config".into(),
            prompts: BenchPrompts::simple("reconcile signed amounts in a transaction ledger"),
            channels,
            leak_globs: vec!["reference_answer.txt".into()],
            allow_subagents: true,
            reserved_scripts: Vec::new(),
        };
        let surface_spec = SurfaceSpec::opencode();
        let mut base: Tree = IndexMap::new();
        for name in &surface_spec.always_on {
            base.insert(name.clone(), String::new());
        }
        base.insert(
            "systemprompt.md".into(),
            "Reconcile the ledger using python scripts/summarize.py.\n".into(),
        );
        base.insert("scripts/summarize.py".into(), BASE_SCRIPT.into());
        let base_dir = root.join("base");
        tree::write_tree(&surface_spec, &base, &base_dir).expect("write base");

        for (task_id, amounts) in [
            ("ledger-1", "100.25\n-20.10\n5.00\n"),
            ("ledger-2", "35.50\n-10.25\n-5.00\n"),
        ] {
            let source = root.join("sources").join(task_id);
            fs::create_dir_all(&source).ok();
            fs::write(source.join("ledger.csv"), format!("amount\n{amounts}")).ok();
        }

        Self {
            root,
            spec,
            surface_spec,
            execution_count: AtomicUsize::new(0),
            score_hook: Arc::new(Mutex::new(None)),
            execute_hook: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_score_hook<F>(&self, f: F)
    where
        F: Fn(&Path) -> Option<HashMap<String, serde_json::Value>> + Send + 'static,
    {
        *self.score_hook.lock().unwrap() = Some(Box::new(f));
    }

    pub fn clear_score_hook(&self) {
        *self.score_hook.lock().unwrap() = None;
    }

    pub fn set_execute_hook<F>(&self, f: F)
    where
        F: Fn(&str) -> Result<(), String> + Send + 'static,
    {
        *self.execute_hook.lock().unwrap() = Some(Box::new(f));
    }

    pub fn clear_execute_hook(&self) {
        *self.execute_hook.lock().unwrap() = None;
    }
}

impl BenchmarkAdapter for DemoAdapter {
    fn spec(&self) -> &BenchSpec {
        &self.spec
    }

    fn surface(&self) -> SurfaceSpec {
        self.surface_spec.clone()
    }

    fn tasks(&self, _split: &str) -> Vec<String> {
        vec!["ledger-1".into(), "ledger-2".into()]
    }

    fn domain_of(&self, _task_id: &str) -> String {
        "accounting".into()
    }

    fn task_prompt(&self, _task_id: &str) -> String {
        "Sum every signed amount in ledger.csv. Return Total: with two decimal places.".into()
    }

    fn task_files(&self, task_id: &str) -> PathBuf {
        self.root.join("sources").join(task_id)
    }

    fn base_harness(&self) -> PathBuf {
        self.root.join("base")
    }

    fn validate_tree(&self, harness: &Tree) -> Result<(), SpecError> {
        if !harness.contains_key("scripts/summarize.py") {
            return Err(SpecError::Msg(
                "The local executor requires scripts/summarize.py".into(),
            ));
        }
        Ok(())
    }

    fn execute(
        &self,
        task_id: &str,
        harness_dir: Option<&Path>,
        run_id: &str,
        _timeout_secs: Option<u64>,
    ) -> Result<PathBuf, SpecError> {
        if let Some(hook) = self.execute_hook.lock().unwrap().as_ref() {
            hook(task_id).map_err(SpecError::Msg)?;
        }
        self.execution_count.fetch_add(1, Ordering::SeqCst);
        let run_dir = self.root.join("executions").join(run_id);
        fs::create_dir_all(&run_dir).map_err(|e| SpecError::Msg(e.to_string()))?;
        let harness = harness_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.base_harness());
        let script = fs::read_to_string(harness.join("scripts/summarize.py"))
            .map_err(|e| SpecError::Msg(e.to_string()))?;
        let ledger = fs::read_to_string(self.task_files(task_id).join("ledger.csv"))
            .map_err(|e| SpecError::Msg(e.to_string()))?;
        let stdout = run_summarize_logic(&script, &ledger).map_err(SpecError::Msg)?;
        fs::write(run_dir.join("answer.txt"), &stdout).map_err(|e| SpecError::Msg(e.to_string()))?;
        fs::write(
            run_dir.join("trajectory.md"),
            format!("[ACTION] Run scripts/summarize.py on ledger.csv\n[OBSERVATION]\n{stdout}"),
        )
        .map_err(|e| SpecError::Msg(e.to_string()))?;
        Ok(run_dir)
    }

    fn delivered(&self, run_dir: &Path) -> bool {
        run_dir.join("answer.txt").is_file()
    }

    fn trajectory(&self, run_dir: &Path, _obs_trunc: usize) -> String {
        fs::read_to_string(run_dir.join("trajectory.md")).unwrap_or_default()
    }

    fn deliverable_text(&self, run_dir: &Path) -> String {
        fs::read_to_string(run_dir.join("answer.txt")).unwrap_or_default()
    }

    fn deliverable_files(&self, run_dir: &Path) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("answer.txt".into(), self.deliverable_text(run_dir));
        m
    }

    fn score(&self, run_dir: &Path) -> Option<HashMap<String, serde_json::Value>> {
        let hook = self.score_hook.lock().unwrap();
        if let Some(f) = hook.as_ref() {
            return f(run_dir);
        }
        None
    }
}

pub struct DemoProposer {
    pub calls: Arc<Mutex<Vec<Reply>>>,
}

impl DemoProposer {
    pub fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl Default for DemoProposer {
    fn default() -> Self {
        Self::new()
    }
}

impl Proposer for DemoProposer {
    fn call(
        &mut self,
        _mission: &str,
        context_dir: Option<&Path>,
        label: &str,
    ) -> Result<Reply, ProposerError> {
        let context = context_dir.ok_or_else(|| {
            ProposerError::new("demo proposer requires a context directory", 1, 0.0)
        })?;
        let mut workspace = None;
        let text;
        if label.starts_with("select.") {
            let ledger = fs::read_to_string(context.join("source/ledger.csv"))
                .map_err(|e| ProposerError::new(e.to_string(), 1, 0.0))?;
            let expected = rust_decimal_total(&ledger, false)
                .map_err(|e| ProposerError::new(e, 1, 0.0))?;
            let expected_line = format!("Total: {expected:.2}");
            let mut chosen = None;
            let cand_dir = context.join("candidates");
            let mut names: Vec<_> = fs::read_dir(&cand_dir)
                .map_err(|e| ProposerError::new(e.to_string(), 1, 0.0))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
                .collect();
            names.sort();
            for path in names {
                let body = fs::read_to_string(&path).unwrap_or_default();
                if body.contains(&expected_line) {
                    chosen = path.file_stem().map(|s| s.to_string_lossy().into_owned());
                    break;
                }
            }
            let chosen = chosen.ok_or_else(|| {
                ProposerError::new("no candidate matched signed total", 1, 0.0)
            })?;
            text = format!(
                "<why>The signed total matches the source ledger.</why>\n```choice\n{chosen}\n```"
            );
        } else if label.starts_with("experience.") {
            let changed = fs::read_to_string(context.join("harness/scripts/summarize.py"))
                .unwrap_or_default()
                != fs::read_to_string(context.join("base_harness/scripts/summarize.py"))
                    .unwrap_or_default();
            text = if changed {
                "<edits>\ncomponent: scripts/summarize.py\ncandidates: inv1\n\
                 change: dropped the positive-only filter from the amount sum\n\
                 observed: the probe total included the credits the base run omitted\n\
                 verdict: helped\n</edits>\n\n\
                 <experience>\ncue: When a ledger includes credits and debits\n\
                 lesson: Include both signs when summing in scripts/summarize.py.\n\
                 evidence: The scripts/summarize.py row -- removing the positive-only filter \
                 changed the probe total to include the credits omitted by the base trajectory.\n\
                 confidence: high for this observed arithmetic contrast\n</experience>"
                    .into()
            } else {
                "NO_ENTRY".into()
            };
        } else {
            let ws = context.join("harness");
            let script_path = ws.join("scripts/summarize.py");
            let body = fs::read_to_string(&script_path).unwrap_or_else(|_| BASE_SCRIPT.into());
            let fixed = body.replace(" if amount > 0", "");
            if let Some(parent) = script_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&script_path, fixed);
            workspace = Some(ws);
            text = "<notes>Keep signed amounts: the base total omitted credits in the source ledger.</notes>".into();
        }
        let reply = Reply {
            label: label.into(),
            text,
            ok: true,
            cost_usd: 0.0,
            attempts: 1,
            cached: false,
            workspace,
        };
        self.calls.lock().unwrap().push(reply.clone());
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_only_vs_signed() {
        let csv = "amount\n100.25\n-20.10\n5.00\n";
        assert_eq!(
            run_summarize_logic(BASE_SCRIPT, csv).unwrap(),
            "Total: 105.25\n"
        );
        let fixed = BASE_SCRIPT.replace(" if amount > 0", "");
        assert_eq!(run_summarize_logic(&fixed, csv).unwrap(), "Total: 85.15\n");
    }
}
