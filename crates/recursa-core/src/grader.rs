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

/// One rubric criterion: deliverable must contain a substring (case-sensitive).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricCriterion {
    pub id: String,
    pub must_contain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rubric {
    pub criteria: Vec<RubricCriterion>,
}

impl Rubric {
    pub fn catalog_stub(task_id: &str) -> Self {
        Self {
            criteria: vec![
                RubricCriterion {
                    id: "refined_ok_prefix".into(),
                    must_contain: "OK:".into(),
                },
                RubricCriterion {
                    id: "mentions_task".into(),
                    must_contain: task_id.into(),
                },
            ],
        }
    }

    pub fn grade_text(&self, text: &str) -> Grade {
        let mut detail = HashMap::new();
        let mut n_passed = 0u64;
        for c in &self.criteria {
            let pass = text.contains(&c.must_contain);
            if pass {
                n_passed += 1;
            }
            detail.insert(c.id.clone(), Value::Bool(pass));
        }
        let n_criteria = self.criteria.len() as u64;
        Grade {
            n_passed,
            n_criteria,
            all_pass: n_criteria > 0 && n_passed == n_criteria,
            detail,
        }
    }
}

/// Grades `answer.txt` (or `answer.md`) against `rubrics.json` in the run dir.
pub struct RubricFileGrader;

impl Grader for RubricFileGrader {
    fn grade(&self, run_dir: &Path) -> Result<Grade, String> {
        let rubric_path = run_dir.join("rubrics.json");
        let text = std::fs::read_to_string(&rubric_path).map_err(|e| e.to_string())?;
        let rubric: Rubric = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if rubric.criteria.is_empty() {
            return Err("rubrics.json has no criteria".into());
        }
        let answer = std::fs::read_to_string(run_dir.join("answer.txt"))
            .or_else(|_| std::fs::read_to_string(run_dir.join("answer.md")))
            .map_err(|e| e.to_string())?;
        Ok(rubric.grade_text(&answer))
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

    #[test]
    fn rubric_grades_partial_credit() {
        let dir = tempfile::tempdir().unwrap();
        let rubric = Rubric::catalog_stub("contracts/task-1");
        std::fs::write(
            dir.path().join("rubrics.json"),
            serde_json::to_string_pretty(&rubric).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("answer.txt"),
            "OK: contracts/task-1\nmemo body\n",
        )
        .unwrap();
        let g = RubricFileGrader.grade(dir.path()).unwrap();
        assert_eq!(g.n_passed, 2);
        assert!(g.all_pass);

        std::fs::write(dir.path().join("answer.txt"), "BASE: contracts/task-1\n").unwrap();
        let g = RubricFileGrader.grade(dir.path()).unwrap();
        assert_eq!(g.n_passed, 1);
        assert!(!g.all_pass);
    }
}
