//! Judged rows → printed score report (base vs refined, pick provenance).
//!
//! Ungraded rows are excluded from the denominator (never counted as zero).

use crate::judge::JudgeRow;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scored {
    pub task_id: String,
    pub mode: String,
    pub k: usize,
    pub label: String,
    pub run_id: String,
    pub chosen_is_base: bool,
    pub graded: bool,
    pub n_passed: Option<u64>,
    pub n_criteria: Option<u64>,
    pub all_pass: bool,
    pub error: String,
}

impl Scored {
    pub fn from_judge_row(row: &JudgeRow, label: &str, chosen_is_base: bool) -> Self {
        let graded = row.graded
            && row.n_passed.is_some()
            && row.n_criteria.map(|c| c > 0).unwrap_or(false);
        let all_pass = graded
            && row.n_passed.zip(row.n_criteria).is_some_and(|(p, c)| p == c && c > 0);
        Self {
            task_id: row.task_id.clone(),
            mode: row.mode.clone(),
            k: row.k,
            label: label.into(),
            run_id: row.run_id.clone(),
            chosen_is_base,
            graded,
            n_passed: row.n_passed,
            n_criteria: row.n_criteria,
            all_pass,
            error: row.error.clone(),
        }
    }

    pub fn frac(&self) -> Option<f64> {
        match (self.n_passed, self.n_criteria) {
            (Some(p), Some(c)) if c > 0 => Some(p as f64 / c as f64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub mode: String,
    pub k: usize,
    pub n: usize,
    pub n_ungraded: usize,
    pub score: Option<f64>,
    pub all_pass: Option<f64>,
    pub kept_base: usize,
    pub chose_final: usize,
    pub chose_candidate: usize,
    pub fell_back: usize,
    pub usd: f64,
}

impl Summary {
    pub fn name(&self) -> String {
        let label = match self.mode.as_str() {
            "agg" => "parallel",
            "seq" => "sequential",
            other => other,
        };
        format!("{label} k={}", self.k)
    }

    pub fn line(&self, bench: &str) -> String {
        if self.n == 0 {
            return format!("  {}: no graded task", self.name());
        }
        let figures = if bench == "lab" {
            format!(
                "partial {:6.2}% | pass {:5.1}%",
                self.score.unwrap_or(0.0),
                self.all_pass.unwrap_or(0.0)
            )
        } else {
            format!("score {:6.2}%", self.score.unwrap_or(0.0))
        };
        let pick = format!(
            " | picked output: final {} / candidate {} / base {}",
            self.chose_final, self.chose_candidate, self.kept_base
        );
        let fb = if self.fell_back > 0 {
            format!(" | fell back {}", self.fell_back)
        } else {
            String::new()
        };
        let ug = if self.n_ungraded > 0 {
            format!(" | ungraded {}", self.n_ungraded)
        } else {
            String::new()
        };
        format!("  {}: n={:>3} | {figures}{pick}{fb}{ug}", self.name(), self.n)
    }
}

fn pick_kind(mode: &str, k: usize, label: &str) -> &'static str {
    if label.starts_with("base") {
        return "base";
    }
    match mode {
        "agg" => {
            if label.starts_with("agg") {
                "final"
            } else {
                "candidate"
            }
        }
        "seq" => {
            let finals: Vec<_> = (1..=k).map(|i| format!("c{i}")).collect();
            if finals.iter().any(|f| f == label) {
                "final"
            } else {
                "candidate"
            }
        }
        _ => "candidate",
    }
}

pub fn summarise(
    rows: &[Scored],
    fell_back: &BTreeMap<(String, usize), usize>,
    usd: &BTreeMap<(String, usize), f64>,
) -> Vec<Summary> {
    let mut keys: Vec<(String, usize)> = rows.iter().map(|r| (r.mode.clone(), r.k)).collect();
    keys.sort();
    keys.dedup();
    let mut out = Vec::new();
    for (mode, k) in keys {
        let group: Vec<&Scored> = rows.iter().filter(|r| r.mode == mode && r.k == k).collect();
        let good: Vec<&Scored> = group.iter().copied().filter(|r| r.graded).collect();
        let mut s = Summary {
            mode: mode.clone(),
            k,
            n: good.len(),
            n_ungraded: group.len() - good.len(),
            score: None,
            all_pass: None,
            kept_base: good.iter().filter(|r| r.chosen_is_base).count(),
            chose_final: good
                .iter()
                .filter(|r| pick_kind(&r.mode, r.k, &r.label) == "final")
                .count(),
            chose_candidate: good
                .iter()
                .filter(|r| pick_kind(&r.mode, r.k, &r.label) == "candidate")
                .count(),
            fell_back: *fell_back.get(&(mode.clone(), k)).unwrap_or(&0),
            usd: (*usd.get(&(mode.clone(), k)).unwrap_or(&0.0) * 100.0).round() / 100.0,
        };
        if !good.is_empty() {
            let mean: f64 = good.iter().filter_map(|r| r.frac()).sum::<f64>() / good.len() as f64;
            s.score = Some((100.0 * mean * 100.0).round() / 100.0);
            s.all_pass = Some(
                (100.0 * good.iter().filter(|r| r.all_pass).count() as f64 / good.len() as f64
                    * 10.0)
                    .round()
                    / 10.0,
            );
        }
        out.push(s);
    }
    out
}

/// Base vs refined gain in percentage points (refined_score - base_score).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainReport {
    pub mode: String,
    pub k: usize,
    pub base_score: Option<f64>,
    pub refined_score: Option<f64>,
    pub delta_pp: Option<f64>,
    pub n_base: usize,
    pub n_refined: usize,
}

pub fn gains(base: &[Summary], refined: &[Summary]) -> Vec<GainReport> {
    let mut out = Vec::new();
    for r in refined {
        let b = base.iter().find(|x| x.mode == r.mode && x.k == r.k);
        let base_score = b.and_then(|x| x.score);
        let refined_score = r.score;
        let delta_pp = match (base_score, refined_score) {
            (Some(a), Some(b)) => Some(((b - a) * 100.0).round() / 100.0),
            _ => None,
        };
        out.push(GainReport {
            mode: r.mode.clone(),
            k: r.k,
            base_score,
            refined_score,
            delta_pp,
            n_base: b.map(|x| x.n).unwrap_or(0),
            n_refined: r.n,
        });
    }
    out
}

pub fn render(summaries: &[Summary], title: &str, bench: &str) -> String {
    let mut lines = Vec::new();
    if !title.is_empty() {
        lines.push(title.to_string());
        lines.push("=".repeat(title.len().max(40)));
    }
    let mut sorted = summaries.to_vec();
    sorted.sort_by_key(|s| (s.k, s.mode.clone()));
    for s in &sorted {
        lines.push(s.line(bench));
    }
    let at1: BTreeMap<_, _> = summaries
        .iter()
        .filter(|s| s.k == 1 && s.score.is_some())
        .map(|s| (s.mode.as_str(), s))
        .collect();
    if at1.contains_key("agg") && at1.contains_key("seq") {
        let gap = (at1["agg"].score.unwrap() - at1["seq"].score.unwrap()).abs();
        lines.push(String::new());
        lines.push(
            "  NOTE  at k=1 parallel and sequential are the same procedure; \
             the gap is a NOISE FLOOR, not a result."
                .into(),
        );
        lines.push(format!("        noise floor ≈ {gap:.2} pt"));
    }
    lines.join("\n")
}

pub fn render_gains(gains: &[GainReport]) -> String {
    let mut lines = vec![
        "Base vs refined (self-evolution gain)".into(),
        "====================================".into(),
    ];
    for g in gains {
        let mode = match g.mode.as_str() {
            "agg" => "parallel",
            "seq" => "sequential",
            other => other,
        };
        match (g.base_score, g.refined_score, g.delta_pp) {
            (Some(b), Some(r), Some(d)) => lines.push(format!(
                "  {mode} k={}: base {b:6.2}% → refined {r:6.2}% | Δ {d:+.2} pp (n_base={} n_ref={})",
                g.k, g.n_base, g.n_refined
            )),
            _ => lines.push(format!(
                "  {mode} k={}: incomplete grades (base_n={} refined_n={})",
                g.k, g.n_base, g.n_refined
            )),
        }
    }
    lines.join("\n")
}

/// Published paper reference gains (percentage points) for orientation — not offline targets.
pub fn paper_reference_gains() -> Vec<(&'static str, f64)> {
    vec![
        ("WorkBuddy-Bench (max reported)", 11.5),
        ("JobBench (max reported)", 4.5),
        ("LAB partial pass (max reported)", 11.8),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarise_and_gain() {
        let refined = vec![
            Scored {
                task_id: "a".into(),
                mode: "agg".into(),
                k: 1,
                label: "agg1".into(),
                run_id: "r1".into(),
                chosen_is_base: false,
                graded: true,
                n_passed: Some(1),
                n_criteria: Some(1),
                all_pass: true,
                error: String::new(),
            },
            Scored {
                task_id: "b".into(),
                mode: "agg".into(),
                k: 1,
                label: "agg1".into(),
                run_id: "r2".into(),
                chosen_is_base: false,
                graded: true,
                n_passed: Some(1),
                n_criteria: Some(1),
                all_pass: true,
                error: String::new(),
            },
        ];
        let base = vec![
            Scored {
                task_id: "a".into(),
                mode: "agg".into(),
                k: 1,
                label: "base1".into(),
                run_id: "b1".into(),
                chosen_is_base: true,
                graded: true,
                n_passed: Some(0),
                n_criteria: Some(1),
                all_pass: false,
                error: String::new(),
            },
            Scored {
                task_id: "b".into(),
                mode: "agg".into(),
                k: 1,
                label: "base1".into(),
                run_id: "b2".into(),
                chosen_is_base: true,
                graded: true,
                n_passed: Some(0),
                n_criteria: Some(1),
                all_pass: false,
                error: String::new(),
            },
        ];
        let empty_fb: BTreeMap<(String, usize), usize> = BTreeMap::new();
        let empty_usd: BTreeMap<(String, usize), f64> = BTreeMap::new();
        let rs = summarise(&refined, &empty_fb, &empty_usd);
        let bs = summarise(&base, &empty_fb, &empty_usd);
        assert!((rs[0].score.unwrap() - 100.0).abs() < 1e-9);
        assert!((bs[0].score.unwrap() - 0.0).abs() < 1e-9);
        let g = gains(&bs, &rs);
        assert!((g[0].delta_pp.unwrap() - 100.0).abs() < 1e-9);
    }
}
