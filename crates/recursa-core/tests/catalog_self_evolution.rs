//! Catalog-path self-evolution (JobBench stub): score Δ + library carry-over.

use recursa_core::benchmarks::BenchmarkStub;
use recursa_core::catalog::{CatalogAdapter, CatalogProposer};
use recursa_core::datasets;
use recursa_core::report::{self, Scored};
use recursa_core::schedule;
use recursa_core::BenchmarkAdapter;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[test]
fn jb_catalog_score_gain_and_library_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("datasets/jb");
    let work = tmp.path().join("work");
    let stub = BenchmarkStub::jobbench();
    datasets::materialise_stub_dataset(&data, &stub).unwrap();

    let adapter = CatalogAdapter::open(&work, &data, stub).unwrap();
    let mut prop = CatalogProposer::new();
    let by = adapter.tasks_by_domain();
    assert!(
        by.get("accounting").map(|t| t.len()).unwrap_or(0) >= 2,
        "accounting must have ≥2 tasks for harnessing-level check"
    );

    // Stream accounting tasks first (seeded order still includes both).
    let accounting = by.get("accounting").cloned().unwrap();
    let k = 1usize;
    let result = schedule::run_domain(
        "accounting",
        accounting.clone(),
        k,
        &adapter,
        &mut prop,
        &adapter.spec.runs_root,
        &work.join("library").join(k.to_string()),
    );
    assert_eq!(result["done"].as_u64().unwrap(), accounting.len() as u64);
    assert_eq!(result["failed"].as_u64().unwrap(), 0);

    // Harnessing-level: task-1 harness is in the library for task-2.
    let lib = work
        .join("library")
        .join(k.to_string())
        .join("agg")
        .join("accounting");
    assert!(lib.join("INDEX.md").is_file());
    assert!(lib.join("harnesses/accounting__task-1").is_dir());

    let t2_lib = adapter
        .spec
        .runs_root
        .join(format!("jb-oc-agg{k}"))
        .join("accounting__task-2/context/inv1/library/harnesses");
    let names: Vec<_> = fs::read_dir(&t2_lib)
        .unwrap_or_else(|_| panic!("missing library context at {}", t2_lib.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().any(|n| n == "accounting__task-1"),
        "task-2 must see task-1 harness; got {names:?}"
    );

    // Score: base vs refined across accounting tasks.
    let mut base_scored = Vec::new();
    let mut refined_scored = Vec::new();
    for task_id in &accounting {
        let slug = task_id.replace('/', "__");
        let summary: Value = serde_json::from_str(
            &fs::read_to_string(
                adapter
                    .spec
                    .runs_root
                    .join(format!("jb-oc-agg{k}/{slug}/summary.json")),
            )
            .unwrap(),
        )
        .unwrap();
        let chosen = PathBuf::from(
            summary["selections"]["agg"]["chosen_run_dir"]
                .as_str()
                .unwrap(),
        );
        let base_dir = chosen.parent().unwrap().join("base1");

        let bg = adapter.score(&base_dir).expect("base score");
        let rg = adapter.score(&chosen).expect("refined score");
        assert_eq!(bg.get("all_pass"), Some(&serde_json::json!(false)));
        assert_eq!(rg.get("all_pass"), Some(&serde_json::json!(true)));

        base_scored.push(Scored {
            task_id: task_id.clone(),
            mode: "agg".into(),
            k,
            label: "base1".into(),
            run_id: format!("jb-oc-agg{k}/{task_id}/base1"),
            chosen_is_base: true,
            graded: true,
            n_passed: bg.get("n_passed").and_then(|v| v.as_u64()),
            n_criteria: bg.get("n_criteria").and_then(|v| v.as_u64()),
            all_pass: false,
            error: String::new(),
        });
        refined_scored.push(Scored {
            task_id: task_id.clone(),
            mode: "agg".into(),
            k,
            label: summary["selections"]["agg"]["chosen_label"]
                .as_str()
                .unwrap_or("agg1")
                .into(),
            run_id: summary["selections"]["agg"]["chosen_run_id"]
                .as_str()
                .unwrap_or("")
                .into(),
            chosen_is_base: false,
            graded: true,
            n_passed: rg.get("n_passed").and_then(|v| v.as_u64()),
            n_criteria: rg.get("n_criteria").and_then(|v| v.as_u64()),
            all_pass: true,
            error: String::new(),
        });
    }

    let empty_fb: BTreeMap<(String, usize), usize> = BTreeMap::new();
    let empty_usd: BTreeMap<(String, usize), f64> = BTreeMap::new();
    let base_sum = report::summarise(&base_scored, &empty_fb, &empty_usd);
    let ref_sum = report::summarise(&refined_scored, &empty_fb, &empty_usd);
    let gains = report::gains(&base_sum, &ref_sum);
    assert!(
        base_sum[0].score.unwrap_or(100.0) < ref_sum[0].score.unwrap_or(0.0),
        "refined must beat base"
    );
    assert!(
        gains[0].delta_pp.unwrap_or(0.0) > 0.0,
        "self-evolution Δ must be positive; got {:?}",
        gains[0].delta_pp
    );
}
