//! Offline self-evolution check (see docs/EVALUATION.md).
//!
//! Task-level: refined harness beats the positive-only base on signed totals.
//! Harnessing-level: domain library carries ledger-1 into ledger-2's context.
//! Scoring: base score 0% → refined 100% (signed-total metric).

use recursa_core::grader::{Grader, LedgerTotalGrader};
use recursa_core::report::{self, Scored};
use recursa_core::schedule;
use recursa_core::BenchmarkAdapter;
use recursa_demo::{expected_signed_total, DemoAdapter, DemoProposer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn run_stream(
    adapter: &DemoAdapter,
    proposer: &mut DemoProposer,
    k: usize,
    root: &Path,
) -> Value {
    schedule::run_domain(
        "accounting",
        adapter.tasks("all"),
        k,
        adapter,
        proposer,
        &adapter.spec.runs_root,
        &root.join("library").join(k.to_string()),
    )
}

#[test]
fn task_and_harnessing_level_self_evolution() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    let mut proposer = DemoProposer::new();
    let k = 1usize;

    // --- Task-level: base harness is wrong on signed ledger-1 ---
    let base_run = adapter
        .execute(
            "ledger-1",
            Some(&adapter.base_harness()),
            "self-evolve/base-ledger-1",
            None,
        )
        .unwrap();
    assert_eq!(
        adapter.deliverable_text(&base_run),
        "Total: 105.25\n",
        "base harness must keep the positive-only defect"
    );

    let result = run_stream(&adapter, &mut proposer, k, root);
    assert_eq!(result["done"].as_u64().unwrap(), 2);
    assert_eq!(result["failed"].as_u64().unwrap(), 0);

    let signed = [("ledger-1", "Total: 85.15\n"), ("ledger-2", "Total: 20.25\n")];
    for (task_id, expected) in signed {
        let summary_path = adapter
            .spec
            .runs_root
            .join(format!("demo-oc-seq{k}"))
            .join(task_id)
            .join("summary.json");
        let summary: Value =
            serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
        for mode in ["agg", "seq"] {
            let chosen = PathBuf::from(
                summary["selections"][mode]["chosen_run_dir"]
                    .as_str()
                    .unwrap(),
            );
            assert!(
                summary["selections"][mode]["ok"].as_bool().unwrap_or(false),
                "{task_id}/{mode} selection must succeed"
            );
            assert_eq!(
                adapter.deliverable_text(&chosen),
                expected,
                "{task_id}/{mode}: refined deliverable must use signed totals"
            );
        }
    }
    // Task-level contrast on ledger-1: refined ≠ defective base.
    let l1: Value = serde_json::from_str(
        &fs::read_to_string(
            adapter
                .spec
                .runs_root
                .join(format!("demo-oc-agg{k}/ledger-1/summary.json")),
        )
        .unwrap(),
    )
    .unwrap();
    let chosen_l1 = PathBuf::from(l1["selections"]["agg"]["chosen_run_dir"].as_str().unwrap());
    assert_ne!(
        adapter.deliverable_text(&chosen_l1),
        adapter.deliverable_text(&base_run)
    );

    // --- Harnessing-level: ledger-1 is in the domain library for ledger-2 ---
    let lib_prefix = root
        .join("library")
        .join(k.to_string())
        .join("agg")
        .join("accounting");
    assert!(
        lib_prefix.join("INDEX.md").is_file(),
        "domain library INDEX must exist after adoption"
    );
    assert!(
        lib_prefix.join("harnesses/ledger-1").is_dir(),
        "adopted harness for ledger-1 must be stored"
    );

    let ledger2_library_ctx = adapter
        .spec
        .runs_root
        .join(format!("demo-oc-agg{k}"))
        .join("ledger-2/context/inv1/library/harnesses");
    let transferred: Vec<_> = fs::read_dir(&ledger2_library_ctx)
        .unwrap_or_else(|_| panic!("missing library context at {}", ledger2_library_ctx.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        transferred.iter().any(|n| n == "ledger-1"),
        "harnessing-level recursion must expose ledger-1 to ledger-2; got {transferred:?}"
    );

    // --- Score: base vs refined (evaluation scheme) ---
    let mut base_scored = Vec::new();
    let mut refined_scored = Vec::new();
    for task_id in ["ledger-1", "ledger-2"] {
        let expected = expected_signed_total(task_id).unwrap();
        let grader = LedgerTotalGrader {
            expected: expected.into(),
        };
        let base_dir = root
            .join("executions")
            .join(format!("demo-oc-agg{k}/{task_id}/base1"));
        let bg = grader.grade(&base_dir).unwrap();
        assert_eq!(bg.n_passed, 0, "{task_id} base must fail signed metric");
        base_scored.push(Scored {
            task_id: task_id.into(),
            mode: "agg".into(),
            k,
            label: "base1".into(),
            run_id: format!("demo-oc-agg{k}/{task_id}/base1"),
            chosen_is_base: true,
            graded: true,
            n_passed: Some(bg.n_passed),
            n_criteria: Some(bg.n_criteria),
            all_pass: bg.all_pass,
            error: String::new(),
        });

        let summary: Value = serde_json::from_str(
            &fs::read_to_string(
                adapter
                    .spec
                    .runs_root
                    .join(format!("demo-oc-agg{k}/{task_id}/summary.json")),
            )
            .unwrap(),
        )
        .unwrap();
        let chosen = PathBuf::from(
            summary["selections"]["agg"]["chosen_run_dir"]
                .as_str()
                .unwrap(),
        );
        let rg = grader.grade(&chosen).unwrap();
        assert!(rg.all_pass, "{task_id} refined must pass signed metric");
        refined_scored.push(Scored {
            task_id: task_id.into(),
            mode: "agg".into(),
            k,
            label: summary["selections"]["agg"]["chosen_label"]
                .as_str()
                .unwrap()
                .into(),
            run_id: summary["selections"]["agg"]["chosen_run_id"]
                .as_str()
                .unwrap()
                .into(),
            chosen_is_base: false,
            graded: true,
            n_passed: Some(rg.n_passed),
            n_criteria: Some(rg.n_criteria),
            all_pass: rg.all_pass,
            error: String::new(),
        });
    }
    let empty_fb: BTreeMap<(String, usize), usize> = BTreeMap::new();
    let empty_usd: BTreeMap<(String, usize), f64> = BTreeMap::new();
    let base_sum = report::summarise(&base_scored, &empty_fb, &empty_usd);
    let ref_sum = report::summarise(&refined_scored, &empty_fb, &empty_usd);
    let gains = report::gains(&base_sum, &ref_sum);
    assert_eq!(base_sum[0].score, Some(0.0));
    assert_eq!(ref_sum[0].score, Some(100.0));
    assert_eq!(gains[0].delta_pp, Some(100.0));
}
