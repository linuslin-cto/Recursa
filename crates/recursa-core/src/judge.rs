//! Grade adopted selections without re-running recursion.

use crate::grader::Grader;
use crate::spec::BenchmarkAdapter;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeRow {
    pub task_id: String,
    pub mode: String,
    pub k: usize,
    pub run_id: String,
    pub selected_at: String,
    pub graded: bool,
    pub n_passed: Option<u64>,
    pub n_criteria: Option<u64>,
    pub error: String,
}

fn as_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().map(|i| i as u64))
}

fn row_meta(sel: &Value) -> (String, String, usize, String, String, bool, String, String) {
    let task_id = sel
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mode = sel
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let k = sel.get("k").and_then(as_u64).unwrap_or(0) as usize;
    let run_id = sel
        .get("chosen_run_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let selected_at = sel
        .get("selected_at")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ok = sel.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let chosen = sel
        .get("chosen_label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let run_dir = sel
        .get("chosen_run_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (task_id, mode, k, run_id, selected_at, ok, chosen, run_dir)
}

pub fn judge_selections(adapter: &dyn BenchmarkAdapter, selections: &[Value]) -> Vec<JudgeRow> {
    let mut rows = Vec::new();
    for sel in selections {
        let (task_id, mode, k, run_id, selected_at, ok, chosen, run_dir) = row_meta(sel);
        if !ok || chosen.is_empty() || run_dir.is_empty() || !adapter.delivered(Path::new(&run_dir))
        {
            rows.push(JudgeRow {
                task_id,
                mode,
                k,
                run_id,
                selected_at,
                graded: false,
                n_passed: None,
                n_criteria: None,
                error: "selection not judgeable".into(),
            });
            continue;
        }
        match adapter.score(Path::new(&run_dir)) {
            Some(score) => rows.push(JudgeRow {
                task_id,
                mode,
                k,
                run_id,
                selected_at,
                graded: true,
                n_passed: score.get("n_passed").and_then(as_u64),
                n_criteria: score.get("n_criteria").and_then(as_u64),
                error: String::new(),
            }),
            None => rows.push(JudgeRow {
                task_id,
                mode,
                k,
                run_id,
                selected_at,
                graded: false,
                n_passed: None,
                n_criteria: None,
                error: "adapter returned no score".into(),
            }),
        }
    }
    rows
}

/// Grade via an explicit `Grader` (scores.json, ledger totals, …).
pub fn judge_selections_with_grader(
    adapter: &dyn BenchmarkAdapter,
    grader: &dyn Grader,
    selections: &[Value],
) -> Vec<JudgeRow> {
    let mut rows = Vec::new();
    for sel in selections {
        let (task_id, mode, k, run_id, selected_at, ok, chosen, run_dir) = row_meta(sel);
        if !ok || chosen.is_empty() || run_dir.is_empty() || !adapter.delivered(Path::new(&run_dir))
        {
            rows.push(JudgeRow {
                task_id,
                mode,
                k,
                run_id,
                selected_at,
                graded: false,
                n_passed: None,
                n_criteria: None,
                error: "selection not judgeable".into(),
            });
            continue;
        }
        match grader.grade(Path::new(&run_dir)) {
            Ok(g) => rows.push(JudgeRow {
                task_id,
                mode,
                k,
                run_id,
                selected_at,
                graded: true,
                n_passed: Some(g.n_passed),
                n_criteria: Some(g.n_criteria),
                error: String::new(),
            }),
            Err(e) => rows.push(JudgeRow {
                task_id,
                mode,
                k,
                run_id,
                selected_at,
                graded: false,
                n_passed: None,
                n_criteria: None,
                error: e,
            }),
        }
    }
    rows
}

/// Per (task, mode, k): same selection keeps a prior grade when re-judge fails.
pub fn merge(old: &[JudgeRow], new: &[JudgeRow]) -> Vec<JudgeRow> {
    let mut by_key: std::collections::BTreeMap<(String, String, usize), JudgeRow> = old
        .iter()
        .map(|r| ((r.task_id.clone(), r.mode.clone(), r.k), r.clone()))
        .collect();
    for r in new {
        let key = (r.task_id.clone(), r.mode.clone(), r.k);
        if let Some(prev) = by_key.get(&key) {
            if prev.run_id == r.run_id
                && prev.selected_at == r.selected_at
                && prev.graded
                && !r.graded
                && !r.error.starts_with("selection")
            {
                continue;
            }
        }
        by_key.insert(key, r.clone());
    }
    by_key.into_values().collect()
}

pub fn collect_selections(ns_dir: &Path, modes: &[&str]) -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(ns_dir) else {
        return out;
    };
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("summary.json"))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    for summary_path in paths {
        let Ok(text) = fs::read_to_string(&summary_path) else {
            continue;
        };
        let Ok(summary) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        for mode in modes {
            if let Some(sel) = summary
                .pointer(&format!("/selections/{mode}"))
                .cloned()
            {
                out.push(sel);
            }
        }
    }
    out
}
