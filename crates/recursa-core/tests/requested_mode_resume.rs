//! Independent recursion modes must resume independently of unrequested failures.
//! Ported from Python `tests/test_requested_mode_resume.py`.

use recursa_core::proposer::{Proposer, ProposerError, Reply};
use recursa_core::schedule;
use recursa_demo::{DemoAdapter, DemoProposer};
use serde_json::Value;
use std::fs;
use std::path::Path;

struct SequentialFailure {
    inner: DemoProposer,
}

impl Proposer for SequentialFailure {
    fn call(
        &mut self,
        mission: &str,
        context_dir: Option<&Path>,
        label: &str,
    ) -> Result<Reply, ProposerError> {
        if label == "select.seq2" {
            return Err(ProposerError::new(
                "temporary sequential selector failure",
                1,
                0.0,
            ));
        }
        self.inner.call(mission, context_dir, label)
    }
}

fn run(
    adapter: &DemoAdapter,
    proposer: &mut dyn Proposer,
    root: &Path,
    modes: &[&str],
) -> Value {
    schedule::run_domain_with_selector(
        "accounting",
        vec!["ledger-1".into()],
        2,
        adapter,
        proposer,
        None,
        &adapter.spec.runs_root,
        &root.join("library"),
        modes,
    )
}

#[test]
fn valid_parallel_mode_resumes_when_sequential_selection_failed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    let mut proposer = SequentialFailure {
        inner: DemoProposer::new(),
    };

    assert_eq!(
        run(&adapter, &mut proposer, root, &["agg", "seq"])["tasks"][0]["status"],
        "partial"
    );
    let summary_path = adapter
        .spec
        .runs_root
        .join("demo-oc-agg2/ledger-1/summary.json");
    let record: Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
    assert!(record["selections"]["agg"]["ok"].as_bool().unwrap());
    assert!(!record["selections"]["seq"]["ok"].as_bool().unwrap());

    let counts = (
        adapter
            .execution_count
            .load(std::sync::atomic::Ordering::SeqCst),
        proposer.inner.call_count(),
    );
    for _ in 0..2 {
        assert_eq!(
            run(&adapter, &mut proposer, root, &["agg"])["tasks"][0]["status"],
            "skipped"
        );
        assert_eq!(
            (
                adapter
                    .execution_count
                    .load(std::sync::atomic::Ordering::SeqCst),
                proposer.inner.call_count()
            ),
            counts
        );
    }
    assert_eq!(
        run(&adapter, &mut proposer, root, &["seq"])["tasks"][0]["status"],
        "partial"
    );
    assert!(
        adapter
            .execution_count
            .load(std::sync::atomic::Ordering::SeqCst)
            > counts.0
    );
}
