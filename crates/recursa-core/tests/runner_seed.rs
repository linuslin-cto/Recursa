//! Seeded namespace identity (Rust portion of `test_runner_seed.py` fixtures).

use recursa_core::schedule::{ns, stream_order, Namespace};
use std::collections::HashMap;

#[test]
fn namespace_embeds_seed_and_task_slug() {
    let n = Namespace {
        benchmark: "lab".into(),
        substrate: "oc".into(),
        mode: "agg".into(),
        k: 1,
        seed: Some(7),
        tag: String::new(),
    };
    assert_eq!(n.as_str(), "lab-oc-agg1-s7");
    assert_eq!(
        n.task_dir(std::path::Path::new("/runs"), "contracts/task-1"),
        std::path::Path::new("/runs/lab-oc-agg1-s7/contracts__task-1")
    );
    assert_eq!(
        ns("lab", "oc", "agg", 1, Some(7), ""),
        "lab-oc-agg1-s7"
    );
    assert_eq!(
        ns("demo", "oc", "seq", 2, None, "smoke"),
        "demo-oc-seq2-smoke"
    );
}

#[test]
fn stream_order_stable_under_seed_and_domain_drop() {
    let mut by = HashMap::new();
    by.insert("a".into(), vec!["a2".into(), "a1".into()]);
    by.insert("b".into(), vec!["b1".into()]);
    let full = stream_order(&by, Some(42), &[]).unwrap();
    by.remove("b");
    let dropped = stream_order(&by, Some(42), &[]).unwrap();
    let full_a: Vec<_> = full.into_iter().filter(|(d, _)| d == "a").collect();
    assert_eq!(full_a, dropped);
    let reference = stream_order(
        &by,
        None,
        &["a".into()],
    )
    .unwrap();
    assert_eq!(reference, vec![("a".into(), "a1".into()), ("a".into(), "a2".into())]);
}
