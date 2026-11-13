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

fn default_weight() -> u64 {
    1
}

/// One rubric criterion: deliverable must contain a substring (case-sensitive).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricCriterion {
    pub id: String,
    pub must_contain: String,
    #[serde(default = "default_weight")]
    pub weight: u64,
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
                    weight: 2,
                },
                RubricCriterion {
                    id: "mentions_task".into(),
                    must_contain: task_id.into(),
                    weight: 1,
                },
            ],
        }
    }

    pub fn grade_text(&self, text: &str) -> Grade {
        let mut detail = HashMap::new();
        let mut n_passed = 0u64;
        let mut n_criteria = 0u64;
        let mut all_pass = true;
        for c in &self.criteria {
            let w = c.weight.max(1);
            n_criteria += w;
            let pass = text.contains(&c.must_contain);
            if pass {
                n_passed += w;
            } else {
                all_pass = false;
            }
            detail.insert(
                c.id.clone(),
                serde_json::json!({"pass": pass, "weight": w}),
            );
        }
        if n_criteria == 0 {
            all_pass = false;
        }
        Grade {
            n_passed,
            n_criteria,
            all_pass: all_pass && n_criteria > 0 && n_passed == n_criteria,
            detail,
        }
    }
}

/// Load LAB-style `criteria` from task.json (never shown to the improver).
pub fn rubric_from_lab_task_json(text: &str) -> Result<Rubric, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Some(arr) = v.get("criteria").and_then(|x| x.as_array()) else {
        return Err("task.json missing criteria[]".into());
    };
    let mut criteria = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let id = item
            .get("id")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("lab{i}"));
        let weight = item
            .get("weight")
            .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f.round() as u64)))
            .unwrap_or(1)
            .max(1);
        let must = item
            .get("must_contain")
            .or_else(|| item.get("keyword"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                item.get("description")
                    .and_then(|x| x.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && s.len() <= 64)
            })
            .ok_or_else(|| format!("criteria[{i}] needs must_contain/keyword"))?;
        criteria.push(RubricCriterion {
            id,
            must_contain: must,
            weight,
        });
    }
    if criteria.is_empty() {
        return Err("criteria[] empty".into());
    }
    Ok(Rubric { criteria })
}

/// Strip grading fields before exposing task.json to improvers.
pub fn strip_lab_task_for_improver(text: &str) -> String {
    let Ok(mut v) = serde_json::from_str::<Value>(text) else {
        return text.to_string();
    };
    if let Some(obj) = v.as_object_mut() {
        obj.remove("criteria");
        obj.remove("rubrics");
        obj.remove("reference_answer");
    }
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| text.to_string())
}

/// Parse Recursa `rubrics.json` or JobBench-style `RUBRICS.json`.
///
/// Supported shapes:
/// - `{"criteria":[{"id","must_contain"}, ...]}`
/// - `{"rubrics":[{"criterion": str|list, "weight": number, "must_contain"?: str}, ...]}`
pub fn load_rubric_json(text: &str) -> Result<Rubric, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if let Ok(r) = serde_json::from_value::<Rubric>(v.clone()) {
        if !r.criteria.is_empty() {
            return Ok(r);
        }
    }
    let Some(arr) = v.get("rubrics").and_then(|x| x.as_array()) else {
        return Err("unrecognized rubric JSON (need criteria[] or rubrics[])".into());
    };
    let mut criteria = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let id = item
            .get("id")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("c{i}"));
        let weight = item
            .get("weight")
            .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f.round() as u64)))
            .unwrap_or(1)
            .max(1);
        if let Some(mc) = item.get("must_contain").and_then(|x| x.as_str()) {
            if !mc.is_empty() {
                criteria.push(RubricCriterion {
                    id,
                    must_contain: mc.into(),
                    weight,
                });
                continue;
            }
        }
        if let Some(kws) = item.get("keywords").and_then(|x| x.as_array()) {
            for (j, kw) in kws.iter().enumerate() {
                if let Some(s) = kw.as_str() {
                    if !s.is_empty() {
                        criteria.push(RubricCriterion {
                            id: format!("{id}.{j}"),
                            must_contain: s.into(),
                            weight,
                        });
                    }
                }
            }
            continue;
        }
        // Fallback: treat criterion text as a required substring when short enough.
        let crit = item.get("criterion");
        let texts: Vec<String> = match crit {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect(),
            _ => Vec::new(),
        };
        for (j, t) in texts.into_iter().enumerate() {
            let needle = t.trim();
            if !needle.is_empty() && needle.len() <= 64 {
                criteria.push(RubricCriterion {
                    id: format!("{id}.{j}"),
                    must_contain: needle.into(),
                    weight,
                });
            }
        }
    }
    if criteria.is_empty() {
        return Err("rubrics[] produced no must_contain criteria".into());
    }
    Ok(Rubric { criteria })
}

/// Grades `answer.txt` (or `answer.md`) against `rubrics.json` / `RUBRICS.json`.
pub struct RubricFileGrader;

impl Grader for RubricFileGrader {
    fn grade(&self, run_dir: &Path) -> Result<Grade, String> {
        let rubric_path = [
            run_dir.join("rubrics.json"),
            run_dir.join("RUBRICS.json"),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| "missing rubrics.json / RUBRICS.json".to_string())?;
        let text = std::fs::read_to_string(&rubric_path).map_err(|e| e.to_string())?;
        let rubric = load_rubric_json(&text)?;
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
        assert_eq!(g.n_passed, 3); // weights 2+1
        assert_eq!(g.n_criteria, 3);
        assert!(g.all_pass);

        std::fs::write(dir.path().join("answer.txt"), "BASE: contracts/task-1\n").unwrap();
        let g = RubricFileGrader.grade(dir.path()).unwrap();
        assert_eq!(g.n_passed, 1); // only mentions_task weight 1
        assert!(!g.all_pass);
    }

    #[test]
    fn load_jobbench_style_rubrics() {
        let jb = r#"{
          "rubrics": [
            {"criterion": "OK:", "weight": 2.0},
            {"criterion": ["contracts/task-1"], "weight": 1.0, "id": "task"}
          ]
        }"#;
        let r = load_rubric_json(jb).unwrap();
        assert_eq!(r.criteria.len(), 2);
        let g = r.grade_text("OK: contracts/task-1\n");
        assert_eq!((g.n_passed, g.n_criteria), (3, 3));
        assert!(g.all_pass);
        let g = r.grade_text("BASE: contracts/task-1\n");
        assert_eq!(g.n_passed, 1);
    }

    #[test]
    fn lab_criteria_stripped_from_improver_view() {
        let raw = r#"{"id":"t","criteria":[{"id":"a","must_contain":"OK:","weight":2}]}"#;
        let r = rubric_from_lab_task_json(raw).unwrap();
        assert_eq!(r.criteria[0].weight, 2);
        let stripped = strip_lab_task_for_improver(raw);
        assert!(!stripped.contains("criteria"));
        assert!(stripped.contains("\"id\""));
    }
}
