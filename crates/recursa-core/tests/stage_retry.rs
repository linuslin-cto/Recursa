//! A retried proposal starts from its previous attempt.
//! Ported from Python `tests/test_stage_retry.py`.

use recursa_core::proposer::{Proposer, ProposerError, Reply};
use recursa_core::recursion;
use recursa_core::tree;
use recursa_core::BenchmarkAdapter;
use recursa_demo::{DemoAdapter, BASE_SCRIPT};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

struct RetryProposer {
    defect: String,
    extra: String,
    seen: Arc<Mutex<Vec<String>>>,
    missions: Arc<Mutex<Vec<String>>>,
}

impl Proposer for RetryProposer {
    fn call(
        &mut self,
        mission: &str,
        context_dir: Option<&Path>,
        label: &str,
    ) -> Result<Reply, ProposerError> {
        let context = context_dir.unwrap();
        let harness = context.join("harness");
        let script = harness.join("scripts/summarize.py");
        let body = fs::read_to_string(&script).unwrap_or_else(|_| BASE_SCRIPT.into());
        self.seen.lock().unwrap().push(body.clone());
        self.missions.lock().unwrap().push(mission.to_string());

        if !mission.contains("PREVIOUS ATTEMPT WAS REJECTED") {
            let defect_path = harness.join(&self.defect);
            if let Some(parent) = defect_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(&defect_path, &self.extra).unwrap();
            fs::write(&script, body.replace(" if amount > 0", "")).unwrap();
        } else {
            let defect_path = harness.join(&self.defect);
            if defect_path.is_file() {
                let _ = fs::remove_file(&defect_path);
            }
        }
        Ok(Reply {
            label: label.into(),
            text: "<notes>Keep signed amounts.</notes>".into(),
            ok: true,
            cost_usd: 0.0,
            attempts: 1,
            cached: false,
            workspace: Some(harness),
        })
    }
}

#[test]
fn retry_keeps_the_rejected_attempts_edits() {
    for (defect, extra) in [
        ("scripts/check.py", "def broken(:\n"),
        (
            "scripts/summarize.py.bak",
            "# an editor's backup, off the editable surface\n",
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let adapter = DemoAdapter::new(tmp.path());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let missions = Arc::new(Mutex::new(Vec::new()));
        let mut prop = RetryProposer {
            defect: defect.into(),
            extra: extra.into(),
            seen: seen.clone(),
            missions: missions.clone(),
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
            &mut prop,
            &tmp.path().join("context"),
            "ledger-1",
            &[],
            Some(&adapter.base_harness()),
            None,
            &[],
            &HashMap::new(),
        );
        let missions = missions.lock().unwrap();
        let seen = seen.lock().unwrap();
        assert!(
            missions[1].contains(defect),
            "retry mission should mention {defect}"
        );
        assert!(!seen[1].contains(" if amount > 0"));
        assert_eq!(
            (stage.fell_back, stage.kept_base, stage.attempts),
            (false, false, 2)
        );
        assert!(!stage.tree["scripts/summarize.py"].contains(" if amount > 0"));
        assert!(!stage.tree.contains_key(defect));
    }
}
