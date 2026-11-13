//! Offline regressions for improvement outages and successful fallback resumes.
//! Ported from Python `tests/test_proposer_failures.py`.

use recursa_core::proposer::{Proposer, ProposerError, Reply};
use recursa_core::schedule;
use recursa_core::BenchmarkAdapter;
use recursa_demo::{DemoAdapter, DemoProposer};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

struct FirstCandidateSelector {
    inner: DemoProposer,
}

impl Proposer for FirstCandidateSelector {
    fn call(
        &mut self,
        mission: &str,
        context_dir: Option<&Path>,
        label: &str,
    ) -> Result<Reply, ProposerError> {
        if label.starts_with("select.") {
            let context = context_dir.unwrap();
            let mut names: Vec<_> = fs::read_dir(context.join("candidates"))
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
                .collect();
            names.sort();
            let chosen = names[0].file_stem().unwrap().to_string_lossy().into_owned();
            let reply = Reply {
                label: label.into(),
                text: format!("```choice\n{chosen}\n```"),
                ok: true,
                cost_usd: 0.0,
                attempts: 1,
                cached: false,
                workspace: None,
            };
            self.inner.calls.lock().unwrap().push(reply.clone());
            return Ok(reply);
        }
        self.inner.call(mission, context_dir, label)
    }
}

struct OutageProposer;

impl Proposer for OutageProposer {
    fn call(
        &mut self,
        _mission: &str,
        _context_dir: Option<&Path>,
        _label: &str,
    ) -> Result<Reply, ProposerError> {
        Err(ProposerError::new("offline outage", 2, 0.25))
    }
}

fn run_stream(
    adapter: &DemoAdapter,
    prop: &mut dyn Proposer,
    selector: &mut dyn Proposer,
    k: usize,
    root: &Path,
) -> Value {
    schedule::run_domain_with_selector(
        "accounting",
        adapter.tasks("all"),
        k,
        adapter,
        prop,
        Some(selector),
        &adapter.spec.runs_root,
        &root.join("library").join(k.to_string()),
        schedule::MODES,
    )
}

fn summary_path(adapter: &DemoAdapter, k: usize, task_id: &str) -> PathBuf {
    adapter
        .spec
        .runs_root
        .join(format!("demo-oc-agg{k}"))
        .join(task_id)
        .join("summary.json")
}

#[test]
fn outage_is_partial_then_healthy_rerun_succeeds_and_skips() {
    for k in [1usize, 2] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let adapter = DemoAdapter::new(root);
        let mut selector = FirstCandidateSelector {
            inner: DemoProposer::new(),
        };
        let executions = adapter
            .execution_count
            .load(std::sync::atomic::Ordering::SeqCst);
        let mut outage = OutageProposer;
        let failed = run_stream(&adapter, &mut outage, &mut selector, k, root);
        assert_eq!(
            (
                failed["done"].as_u64().unwrap(),
                failed["failed"].as_u64().unwrap()
            ),
            (0, 2)
        );
        let statuses: Vec<_> = failed["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["status"].as_str().unwrap())
            .collect();
        assert_eq!(statuses, ["partial", "partial"]);

        for task_id in adapter.tasks("all") {
            let summary: Value =
                serde_json::from_str(&fs::read_to_string(summary_path(&adapter, k, &task_id)).unwrap())
                    .unwrap();
            assert_eq!(summary["status"], "partial");
            assert_eq!(summary["errors"].as_array().unwrap().len(), 6 * k);
            for mode in ["agg", "seq"] {
                let record = &summary["modes"][mode];
                assert!(!record["ok"].as_bool().unwrap());
                assert!(summary["selections"][mode]["ok"].as_bool().unwrap());
                for stage in record["stages"].as_array().unwrap() {
                    assert!(stage["execution_failed"].as_bool().unwrap());
                    assert!(stage["fell_back"].as_bool().unwrap());
                    assert!(!stage["cached"].as_bool().unwrap());
                    assert_eq!(
                        (
                            stage["attempts"].as_u64().unwrap(),
                            stage["usd"].as_f64().unwrap()
                        ),
                        (2, 0.25)
                    );
                    let label = stage["label"].as_str().unwrap();
                    let needle = format!(
                        "{mode}: {label}: proposer execution failed: offline outage"
                    );
                    assert!(summary["errors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e.as_str() == Some(needle.as_str())));
                }
                let ledger = &record["ledger"];
                assert_eq!(
                    (ledger["R"].as_u64().unwrap(), ledger["P"].as_u64().unwrap()),
                    ((4 * k) as u64, (3 * k) as u64)
                );
                assert_eq!(ledger["session_attempts"].as_u64().unwrap(), (6 * k) as u64);
                assert!((ledger["session_usd"].as_f64().unwrap() - 0.75 * k as f64).abs() < 1e-9);
                let chosen = PathBuf::from(
                    summary["selections"][mode]["chosen_run_dir"]
                        .as_str()
                        .unwrap(),
                );
                assert!(adapter.delivered(&chosen));
            }
        }
        assert_eq!(
            adapter
                .execution_count
                .load(std::sync::atomic::Ordering::SeqCst)
                - executions,
            14 * k
        );

        let mut proposer = DemoProposer::new();
        let executions = adapter
            .execution_count
            .load(std::sync::atomic::Ordering::SeqCst);
        let healthy = schedule::run_domain(
            "accounting",
            adapter.tasks("all"),
            k,
            &adapter,
            &mut proposer,
            &adapter.spec.runs_root,
            &root.join("library").join(k.to_string()),
        );
        let statuses: Vec<_> = healthy["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["status"].as_str().unwrap())
            .collect();
        assert_eq!(statuses, ["ok", "ok"]);
        assert_eq!(
            adapter
                .execution_count
                .load(std::sync::atomic::Ordering::SeqCst)
                - executions,
            14 * k
        );
        for task_id in adapter.tasks("all") {
            let summary: Value =
                serde_json::from_str(&fs::read_to_string(summary_path(&adapter, k, &task_id)).unwrap())
                    .unwrap();
            assert_eq!(summary["status"], "ok");
            assert!(summary["errors"].as_array().unwrap().is_empty());
            for mode in ["agg", "seq"] {
                let record = &summary["modes"][mode];
                assert!(record["ok"].as_bool().unwrap());
                assert!(record["stages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| !s["execution_failed"].as_bool().unwrap()));
                assert_eq!(
                    record["ledger"]["session_attempts"].as_u64().unwrap(),
                    (3 * k) as u64
                );
                assert_eq!(record["ledger"]["session_usd"].as_f64().unwrap(), 0.0);
            }
        }

        let executions = adapter
            .execution_count
            .load(std::sync::atomic::Ordering::SeqCst);
        let calls = proposer.call_count();
        let resumed = schedule::run_domain(
            "accounting",
            adapter.tasks("all"),
            k,
            &adapter,
            &mut proposer,
            &adapter.spec.runs_root,
            &root.join("library").join(k.to_string()),
        );
        let statuses: Vec<_> = resumed["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["status"].as_str().unwrap())
            .collect();
        assert_eq!(statuses, ["skipped", "skipped"]);
        assert_eq!(
            (
                adapter
                    .execution_count
                    .load(std::sync::atomic::Ordering::SeqCst),
                proposer.call_count()
            ),
            (executions, calls)
        );
    }
}
