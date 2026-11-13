//! Wire `Grader` into judge_selections_with_grader.

use recursa_core::grader::LedgerTotalGrader;
use recursa_core::judge;
use recursa_core::schedule;
use recursa_demo::{DemoAdapter, DemoProposer};
use serde_json::json;

#[test]
fn grader_scores_adopted_demo_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    let mut proposer = DemoProposer::new();
    schedule::run_domain(
        "accounting",
        vec!["ledger-1".into()],
        1,
        &adapter,
        &mut proposer,
        &adapter.spec.runs_root,
        &root.join("library"),
    );
    let summary: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            adapter
                .spec
                .runs_root
                .join("demo-oc-agg1/ledger-1/summary.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let sel = summary["selections"]["agg"].clone();
    let grader = LedgerTotalGrader {
        expected: "Total: 85.15\n".into(),
    };
    let rows = judge::judge_selections_with_grader(&adapter, &grader, &[sel]);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].graded);
    assert_eq!(rows[0].n_passed, Some(1));
    assert_eq!(rows[0].n_criteria, Some(1));

    let bad = judge::judge_selections_with_grader(
        &adapter,
        &LedgerTotalGrader {
            expected: "Total: 0.00\n".into(),
        },
        &[summary["selections"]["agg"].clone()],
    );
    assert_eq!(bad[0].n_passed, Some(0));
    let _ = json!(null);
}
