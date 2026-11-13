//! Focused regressions for execution, evidence, and adoption contracts.
//! Ported from Python `tests/test_workflow.py`.

use recursa_core::context;
use recursa_core::judge;
use recursa_core::library::Library;
use recursa_core::proposer::{Proposer, ProposerError, Reply};
use recursa_core::recursion;
use recursa_core::schedule;
use recursa_core::spec::resolve_channel_model;
use recursa_core::tree;
use recursa_core::BenchmarkAdapter;
use recursa_demo::{DemoAdapter, DemoProposer, BASE_SCRIPT};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn run_stream(
    adapter: &DemoAdapter,
    proposer: &mut dyn Proposer,
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
fn recursion_budget_and_resume() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    let mut proposer = DemoProposer::new();

    assert!(resolve_channel_model(None, None).unwrap().model.is_empty());
    assert!(recursa_core::proposer::ChannelProposer::new(
        resolve_channel_model(None, None).unwrap()
    )
    .is_err());
    assert!(resolve_channel_model(Some("opencode"), Some("")).is_err());

    for k in [1usize, 2] {
        let result = run_stream(&adapter, &mut proposer, k, root);
        assert_eq!(
            (
                result["done"].as_u64().unwrap(),
                result["failed"].as_u64().unwrap()
            ),
            (2, 0),
            "k={k}"
        );
        for task_id in adapter.tasks("all") {
            let summary_path = adapter
                .spec
                .runs_root
                .join(format!("demo-oc-agg{k}"))
                .join(&task_id)
                .join("summary.json");
            let summary: Value =
                serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
            for mode in ["agg", "seq"] {
                let ledger = &summary["modes"][mode]["ledger"];
                assert_eq!(
                    (ledger["R"].as_u64().unwrap(), ledger["P"].as_u64().unwrap()),
                    ((4 * k) as u64, (3 * k) as u64),
                    "task={task_id} mode={mode}"
                );
                let selection = &summary["selections"][mode];
                let expected = if task_id == "ledger-1" {
                    "Total: 85.15\n"
                } else {
                    "Total: 20.25\n"
                };
                let chosen = PathBuf::from(selection["chosen_run_dir"].as_str().unwrap());
                assert_eq!(adapter.deliverable_text(&chosen), expected);
            }
        }
        let executions = adapter
            .execution_count
            .load(std::sync::atomic::Ordering::SeqCst);
        let calls = proposer.call_count();
        let resumed = run_stream(&adapter, &mut proposer, k, root);
        assert_eq!(
            adapter
                .execution_count
                .load(std::sync::atomic::Ordering::SeqCst),
            executions
        );
        assert_eq!(proposer.call_count(), calls);
        let statuses: Vec<_> = resumed["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["status"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(statuses, vec!["skipped", "skipped"]);
    }
}

#[test]
fn complete_evidence_and_library_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    fs::write(
        adapter.task_files("ledger-1").join("reference_answer.txt"),
        "\"pass_rate\": 1",
    )
    .unwrap();
    let mut proposer = DemoProposer::new();
    run_stream(&adapter, &mut proposer, 2, root);

    let work = adapter.spec.runs_root.join("demo-oc-seq2/ledger-1");
    let reflection = work.join("reflection/seq2");
    let required = [
        "source/ledger.csv",
        "rollouts/base1.md",
        "rollouts/c2.md",
        "pool/inv1/harness.diff",
        "decisions/c1.md",
        "adoption.json",
        "base_harness/scripts/summarize.py",
        "harness/scripts/summarize.py",
    ];
    let missing: Vec<_> = required
        .iter()
        .filter(|name| !reflection.join(name).is_file())
        .collect();
    assert!(missing.is_empty(), "missing reflection files: {missing:?}");
    assert!(work.join("context/inv2/pool/c1/rollout.md").is_file());
    let leaked: Vec<_> = walkdir::WalkDir::new(&work)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() == "reference_answer.txt")
        .collect();
    assert!(leaked.is_empty());

    let next_context = work
        .parent()
        .unwrap()
        .join("ledger-2/context/inv1/library/harnesses");
    let names: Vec<_> = fs::read_dir(&next_context)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["ledger-1".to_string()]);

    let mut leak_tree = tree::Tree::new();
    leak_tree.insert("EXPERIENCE.md".into(), "\"pass_rate\": 1".into());
    assert!(context::assert_tree_has_no_grader_verdicts(&leak_tree).is_err());
}

#[test]
fn invalid_proposal_retries_with_feedback_and_accumulates_cost() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    let feedback = Arc::new(Mutex::new(String::new()));
    let feedback2 = feedback.clone();

    struct RepairProposer {
        inner: DemoProposer,
        feedback: Arc<Mutex<String>>,
    }
    impl Proposer for RepairProposer {
        fn call(
            &mut self,
            mission: &str,
            context_dir: Option<&Path>,
            label: &str,
        ) -> Result<Reply, ProposerError> {
            let context = context_dir.unwrap();
            let script = context.join("harness/scripts/summarize.py");
            if label.ends_with(".a1") {
                let _ = fs::create_dir_all(script.parent().unwrap());
                fs::write(&script, "def broken(:\n").unwrap();
                return Ok(Reply {
                    label: label.into(),
                    text: "invalid edit".into(),
                    ok: true,
                    cost_usd: 1.0,
                    attempts: 1,
                    cached: false,
                    workspace: Some(script.parent().unwrap().parent().unwrap().to_path_buf()),
                });
            }
            *self.feedback.lock().unwrap() = mission.to_string();
            fs::write(&script, BASE_SCRIPT).unwrap();
            let mut reply = self.inner.call(mission, context_dir, label)?;
            reply.cost_usd = 2.0;
            Ok(reply)
        }
    }

    let mut proposer = RepairProposer {
        inner: DemoProposer::new(),
        feedback: feedback2,
    };
    let fallback = tree::read_tree(&adapter.surface(), &adapter.base_harness())
        .unwrap()
        .0;
    let stage = recursion::run_stage(
        "inv1",
        "INV",
        "Repair the script",
        &fallback,
        &adapter,
        &mut proposer,
        &root.join("context"),
        "ledger-1",
        &[],
        Some(&adapter.base_harness()),
        None,
        &[],
        &HashMap::new(),
    );
    assert_eq!(
        (stage.fell_back, stage.attempts, stage.usd),
        (false, 2, 3.0)
    );
    assert!(feedback
        .lock()
        .unwrap()
        .contains("PREVIOUS ATTEMPT WAS REJECTED"));
}

#[test]
fn no_entry_stays_out_of_library_on_resume() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);

    struct NoExperience {
        inner: DemoProposer,
    }
    impl Proposer for NoExperience {
        fn call(
            &mut self,
            mission: &str,
            context_dir: Option<&Path>,
            label: &str,
        ) -> Result<Reply, ProposerError> {
            if label.starts_with("experience.") {
                if label.ends_with("seq1") {
                    return Err(ProposerError::new("reflection failed", 2, 2.0));
                }
                return Ok(Reply {
                    label: label.into(),
                    text: "NO_ENTRY".into(),
                    ok: true,
                    cost_usd: 1.0,
                    attempts: 1,
                    cached: false,
                    workspace: None,
                });
            }
            self.inner.call(mission, context_dir, label)
        }
    }

    let mut proposer = NoExperience {
        inner: DemoProposer::new(),
    };
    run_stream(&adapter, &mut proposer, 1, root);
    run_stream(&adapter, &mut proposer, 1, root);
    for mode in ["agg", "seq"] {
        let library = Library::new(
            root.join("library/1").join(mode),
            "accounting",
            adapter.surface(),
        );
        assert!(library.entries().is_empty());
    }
    let summary_path = adapter
        .spec
        .runs_root
        .join("demo-oc-agg1/ledger-1/summary.json");
    let selections: Value =
        serde_json::from_str(&fs::read_to_string(summary_path).unwrap()).unwrap();
    let selections = &selections["selections"];
    assert_eq!(
        (
            selections["agg"]["usd"].as_f64().unwrap(),
            selections["seq"]["usd"].as_f64().unwrap()
        ),
        (1.0, 2.0)
    );
}

#[test]
fn failed_selection_is_never_judged() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    struct FailedSelector {
        inner: DemoProposer,
    }
    impl Proposer for FailedSelector {
        fn call(
            &mut self,
            mission: &str,
            context_dir: Option<&Path>,
            label: &str,
        ) -> Result<Reply, ProposerError> {
            if label.starts_with("select.") {
                return Err(ProposerError::new("selection failed", 2, 3.0));
            }
            self.inner.call(mission, context_dir, label)
        }
    }
    let mut proposer = FailedSelector {
        inner: DemoProposer::new(),
    };
    run_stream(&adapter, &mut proposer, 2, root);
    let summary: Value = serde_json::from_str(
        &fs::read_to_string(
            adapter
                .spec
                .runs_root
                .join("demo-oc-agg2/ledger-1/summary.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (
            summary["selections"]["agg"]["usd"].as_f64().unwrap(),
            summary["selections"]["seq"]["usd"].as_f64().unwrap()
        ),
        (3.0, 3.0)
    );
    adapter.set_score_hook(|_| {
        let mut m = HashMap::new();
        m.insert("n_passed".into(), serde_json::json!(1));
        m.insert("n_criteria".into(), serde_json::json!(1));
        Some(m)
    });
    let scored = Arc::new(Mutex::new(false));
    let scored2 = scored.clone();
    adapter.set_score_hook(move |_| {
        *scored2.lock().unwrap() = true;
        let mut m = HashMap::new();
        m.insert("n_passed".into(), serde_json::json!(1));
        m.insert("n_criteria".into(), serde_json::json!(1));
        Some(m)
    });
    let rows = judge::judge_selections(
        &adapter,
        &[
            summary["selections"]["agg"].clone(),
            summary["selections"]["seq"].clone(),
        ],
    );
    assert!(!*scored.lock().unwrap());
    assert!(!rows[0].graded);
    assert_eq!(rows[0].n_passed, None);
}

#[test]
fn failed_rerun_clears_stale_selections_in_summary() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    let mut proposer = DemoProposer::new();
    run_stream(&adapter, &mut proposer, 1, root);
    let summary_path = adapter
        .spec
        .runs_root
        .join("demo-oc-agg1/ledger-1/summary.json");
    let initial: Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
    assert!(initial["selections"]["agg"]["ok"].as_bool().unwrap());
    assert!(initial["selections"]["seq"]["ok"].as_bool().unwrap());

    let chosen = PathBuf::from(
        initial["selections"]["agg"]["chosen_run_dir"]
            .as_str()
            .unwrap(),
    );
    fs::remove_file(chosen.join("answer.txt")).unwrap();

    let failed_mode = schedule::with_run_options(
        schedule::RunOptions {
            force_mode_error: Some("recursion failed on rerun".into()),
            ..Default::default()
        },
        || run_stream(&adapter, &mut proposer, 1, root),
    );
    let statuses: Vec<_> = failed_mode["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(statuses, vec!["partial", "skipped"]);
    let after_mode_err: Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
    for mode in ["agg", "seq"] {
        assert!(!after_mode_err["modes"][mode]["ok"].as_bool().unwrap());
        assert!(!after_mode_err["selections"][mode]["ok"].as_bool().unwrap());
        assert_eq!(
            after_mode_err["selections"][mode]["chosen_label"]
                .as_str()
                .unwrap(),
            ""
        );
    }
    let rows = judge::judge_selections(
        &adapter,
        &[
            after_mode_err["selections"]["agg"].clone(),
            after_mode_err["selections"]["seq"].clone(),
        ],
    );
    assert!(rows.iter().all(|r| !r.graded));

    struct PaidReflection {
        inner: DemoProposer,
    }
    impl Proposer for PaidReflection {
        fn call(
            &mut self,
            mission: &str,
            context_dir: Option<&Path>,
            label: &str,
        ) -> Result<Reply, ProposerError> {
            let mut reply = self.inner.call(mission, context_dir, label)?;
            if label.starts_with("experience.") {
                reply.cost_usd = 4.0;
            }
            Ok(reply)
        }
    }
    let mut paid = PaidReflection {
        inner: DemoProposer::new(),
    };
    let failed_lib = schedule::with_run_options(
        schedule::RunOptions {
            library_fail_tasks: vec!["ledger-1".into()],
            ..Default::default()
        },
        || run_stream(&adapter, &mut paid, 1, root),
    );
    let statuses: Vec<_> = failed_lib["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(statuses, vec!["partial", "skipped"]);
    let after_lib_err: Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
    for mode in ["agg", "seq"] {
        assert!(!after_lib_err["selections"][mode]["ok"].as_bool().unwrap());
        assert_eq!(
            after_lib_err["selections"][mode]["usd"].as_f64().unwrap(),
            4.0
        );
    }

    let repaired = run_stream(&adapter, &mut proposer, 1, root);
    let statuses: Vec<_> = repaired["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(statuses, vec!["ok", "skipped"]);
    let restored: Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
    fs::remove_file(PathBuf::from(
        restored["selections"]["agg"]["chosen_run_dir"]
            .as_str()
            .unwrap(),
    )
    .join("answer.txt"))
    .unwrap();
    adapter.set_execute_hook(|_| Err("executor down".into()));
    let failed_base = run_stream(&adapter, &mut proposer, 1, root);
    adapter.clear_execute_hook();
    let statuses: Vec<_> = failed_base["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(statuses, vec!["base-failed", "skipped"]);
    let after_base_err: Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).unwrap()).unwrap();
    assert_eq!(
        after_base_err["status"].as_str().unwrap(),
        "base-failed"
    );
    for mode in ["agg", "seq"] {
        assert!(!after_base_err["modes"][mode]["ok"].as_bool().unwrap());
        assert!(!after_base_err["selections"][mode]["ok"].as_bool().unwrap());
    }
}

#[test]
fn failed_rejudge_keeps_grade_only_for_same_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let adapter = DemoAdapter::new(root);
    let mut proposer = DemoProposer::new();
    run_stream(&adapter, &mut proposer, 1, root);
    let ns_dir = adapter.spec.runs_root.join("demo-oc-agg1");

    let first = judge::collect_selections(&ns_dir, &["agg"]);
    assert!(first
        .iter()
        .all(|s| s.get("selected_at").and_then(|v| v.as_str()).unwrap_or("").len() > 0));
    let first_by_task: HashMap<String, Value> = first
        .iter()
        .filter_map(|s| {
            s.get("task_id")
                .and_then(|v| v.as_str())
                .map(|t| (t.to_string(), s.clone()))
        })
        .collect();

    adapter.set_score_hook(|_| {
        let mut m = HashMap::new();
        m.insert("n_passed".into(), serde_json::json!(1));
        m.insert("n_criteria".into(), serde_json::json!(1));
        Some(m)
    });
    let graded = judge::judge_selections(&adapter, &first);

    // Simulate judge endpoint unavailable: return None so graded=false without selection error.
    adapter.set_score_hook(|_| None);
    let failed = judge::judge_selections(&adapter, &first);
    let merged = judge::merge(&graded, &failed);
    assert!(merged.iter().all(|r| r.graded));

    fs::remove_file(
        PathBuf::from(
            first_by_task["ledger-1"]["chosen_run_dir"]
                .as_str()
                .unwrap(),
        )
        .join("answer.txt"),
    )
    .unwrap();
    adapter.clear_score_hook();
    run_stream(&adapter, &mut proposer, 1, root);
    let again = judge::collect_selections(&ns_dir, &["agg"]);
    let again_by_task: HashMap<String, Value> = again
        .iter()
        .filter_map(|s| {
            s.get("task_id")
                .and_then(|v| v.as_str())
                .map(|t| (t.to_string(), s.clone()))
        })
        .collect();
    assert_eq!(
        again_by_task["ledger-1"]["chosen_run_id"],
        first_by_task["ledger-1"]["chosen_run_id"]
    );
    assert_ne!(
        again_by_task["ledger-1"]["selected_at"],
        first_by_task["ledger-1"]["selected_at"]
    );
    assert_eq!(
        again_by_task["ledger-2"]["selected_at"],
        first_by_task["ledger-2"]["selected_at"]
    );

    adapter.set_score_hook(|_| None);
    let failed = judge::judge_selections(&adapter, &again);
    let merged = judge::merge(&graded, &failed);
    let by_task: HashMap<_, _> = merged
        .into_iter()
        .map(|r| (r.task_id.clone(), r.graded))
        .collect();
    assert_eq!(by_task.get("ledger-1"), Some(&false));
    assert_eq!(by_task.get("ledger-2"), Some(&true));
}
