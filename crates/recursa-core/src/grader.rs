//! Grader seam for benchmark judges (stub + local numeric checks).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grade {
    pub n_passed: u64,
    pub n_criteria: u64,
    pub all_pass: bool,
    pub detail: HashMap<String, Value>,
}

impl Grade {
    pub fn from_counts(n_passed: u64, n_criteria: u64) -> Self {
        Self {
            n_passed,
            n_criteria,
            all_pass: n_criteria > 0 && n_passed == n_criteria,
            detail: HashMap::new(),
        }
    }
}

pub trait Grader: Send + Sync {
    fn grade(&self, run_dir: &Path) -> Result<Grade, String>;
}

/// Reads `scores.json` written by an upstream judge / fixture.
pub struct ScoresFileGrader;

impl Grader for ScoresFileGrader {
    fn grade(&self, run_dir: &Path) -> Result<Grade, String> {
        let path = run_dir.join("scores.json");
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let n_passed = v
            .get("n_passed")
            .and_then(|x| x.as_u64())
            .ok_or_else(|| "scores.json missing n_passed".to_string())?;
        let n_criteria = v
            .get("n_criteria")
            .and_then(|x| x.as_u64())
            .ok_or_else(|| "scores.json missing n_criteria".to_string())?;
        let mut g = Grade::from_counts(n_passed, n_criteria);
        if let Some(obj) = v.as_object() {
            for (k, val) in obj {
                g.detail.insert(k.clone(), val.clone());
            }
        }
        Ok(g)
    }
}

/// Demo ledger grader: pass if `answer.txt` equals the expected Total line.
pub struct LedgerTotalGrader {
    pub expected: String,
}

impl Grader for LedgerTotalGrader {
    fn grade(&self, run_dir: &Path) -> Result<Grade, String> {
        let got = std::fs::read_to_string(run_dir.join("answer.txt")).map_err(|e| e.to_string())?;
        let pass = got.trim() == self.expected.trim();
        Ok(Grade::from_counts(u64::from(pass), 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_file_and_ledger_graders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("scores.json"),
            r#"{"n_passed":2,"n_criteria":2,"all_pass":true}"#,
        )
        .unwrap();
        let g = ScoresFileGrader.grade(dir.path()).unwrap();
        assert!(g.all_pass);

        std::fs::write(dir.path().join("answer.txt"), "Total: 85.15\n").unwrap();
        let g = LedgerTotalGrader {
            expected: "Total: 85.15\n".into(),
        }
        .grade(dir.path())
        .unwrap();
        assert!(g.all_pass);
    }
}
