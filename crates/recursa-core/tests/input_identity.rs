//! Changing task inputs must invalidate resume (runmark schedule integration).

use recursa_core::schedule;
use recursa_core::BenchmarkAdapter;
use recursa_demo::{DemoAdapter, DemoProposer};
use serde_json::Value;
use std::fs;
use std::sync::atomic::Ordering;

#[test]
fn changed_prompt_or_source_forces_rerun() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    let mut proposer = DemoProposer::new();

    let first = schedule::run_domain(
        "accounting",
        vec!["ledger-1".into()],
        1,
        &adapter,
        &mut proposer,
        &adapter.spec.runs_root,
        &root.join("library"),
    );
    assert_eq!(first["tasks"][0]["status"], "ok");
    let summary_path = adapter
        .spec
        .runs_root
        .join("demo-oc-agg1/ledger-1/summary.json");
    let summary: Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
    assert!(summary.get("inputs").and_then(|v| v.as_str()).is_some());

    let executions = adapter.execution_count.load(Ordering::SeqCst);
    let skipped = schedule::run_domain(
        "accounting",
        vec!["ledger-1".into()],
        1,
        &adapter,
        &mut proposer,
        &adapter.spec.runs_root,
        &root.join("library"),
    );
    assert_eq!(skipped["tasks"][0]["status"], "skipped");
    assert_eq!(adapter.execution_count.load(Ordering::SeqCst), executions);

    // Mutate source material → new inputs digest → must recompute.
    fs::write(
        adapter.task_files("ledger-1").join("ledger.csv"),
        "amount\n1.00\n-0.50\n",
    )
    .unwrap();
    let rerun = schedule::run_domain(
        "accounting",
        vec!["ledger-1".into()],
        1,
        &adapter,
        &mut proposer,
        &adapter.spec.runs_root,
        &root.join("library"),
    );
    assert_eq!(rerun["tasks"][0]["status"], "ok");
    assert!(adapter.execution_count.load(Ordering::SeqCst) > executions);
    let after: Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
    assert_ne!(summary["inputs"], after["inputs"]);
}
