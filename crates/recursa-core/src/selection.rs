//! Choose an anonymized final output, then reflect on refinement evidence.

use crate::context;
use crate::proposer::{Proposer, ProposerError};
use crate::recursion::Stage;
use crate::spec::{BenchmarkAdapter, RunView};
use crate::tree;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Selection {
    pub task_id: String,
    pub mode: String,
    pub k: usize,
    pub chosen_id: String,
    pub chosen_label: String,
    pub chosen_run_id: String,
    pub chosen_run_dir: String,
    pub chosen_is_base: bool,
    pub why: String,
    pub id_map: HashMap<String, String>,
    pub dropped: Vec<String>,
    pub n_candidates: usize,
    pub ok: bool,
    pub fallback: bool,
    pub error: String,
    pub usd: f64,
    pub experience: HashMap<String, String>,
    pub trivial: bool,
    pub evidence: String,
    pub selected_at: String,
}

fn anon_id(index: usize) -> String {
    let mut out = String::new();
    let mut n = index + 1;
    while n > 0 {
        let rem = (n - 1) % 26;
        out.insert(0, char::from(b'A' + rem as u8));
        n = (n - 1) / 26;
    }
    out
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

pub fn assign_ids(labels: &[String], task_id: &str, k: usize, mode: &str) -> HashMap<String, String> {
    let mut hasher = Sha1::new();
    hasher.update(format!("{task_id}|{k}|{mode}").as_bytes());
    let digest = hasher.finalize();
    let seed = u64::from_str_radix(&hex::encode(&digest[..6]), 16).unwrap_or(0);
    let mut ordered = labels.to_vec();
    ordered.sort();
    let mut state = seed;
    for i in (1..ordered.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (state as usize) % (i + 1);
        ordered.swap(i, j);
    }
    ordered
        .into_iter()
        .enumerate()
        .map(|(i, lbl)| (anon_id(i), lbl))
        .collect()
}

pub fn selection_pool(
    mode: &str,
    k: usize,
    rollouts: &HashMap<String, RunView>,
) -> HashMap<String, RunView> {
    let labels: Vec<String> = match mode {
        "agg" => (1..=k).map(|i| format!("agg{i}")).collect(),
        "seq" => (1..=k).map(|i| format!("c{i}")).collect(),
        _ => Vec::new(),
    };
    labels
        .into_iter()
        .filter_map(|l| rollouts.get(&l).cloned().map(|rv| (l, rv)))
        .collect()
}

fn parse_choice(text: &str) -> Option<String> {
    let re = Regex::new(r"(?s)```choice\s*\n?\s*([A-Za-z0-9_-]+)\s*\n?\s*```").unwrap();
    re.captures(text).map(|c| c[1].to_string())
}

fn parse_why(text: &str) -> String {
    let re = Regex::new(r"(?s)<why>(.*?)</why>").unwrap();
    re.captures(text)
        .map(|c| c[1].trim().to_string())
        .unwrap_or_default()
}

fn parse_experience(text: &str) -> HashMap<String, String> {
    let re = Regex::new(r"(?s)<experience>(.*?)</experience>").unwrap();
    let Some(block) = re.captures(text) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for key in ["cue", "lesson", "evidence", "confidence"] {
        let key_re = Regex::new(&format!(r"(?m)^{key}:\s*(.*)$")).unwrap();
        if let Some(c) = key_re.captures(&block[1]) {
            out.insert(key.to_string(), c[1].trim().to_string());
        }
    }
    out
}

pub fn has_experience(experience: &HashMap<String, String>) -> bool {
    ["cue", "lesson", "evidence"]
        .iter()
        .all(|k| experience.get(*k).map(|v| !v.trim().is_empty()).unwrap_or(false))
}

fn copy_file_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.exists() {
        return Ok(());
    }
    fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(src) else {
            continue;
        };
        let target = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&target)?;
        } else if path.is_file() {
            if path.file_name().and_then(|s| s.to_str()) == Some("reference_answer.txt") {
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, target)?;
        }
    }
    Ok(())
}

pub fn run_selection(
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    task_id: &str,
    mode: &str,
    k: usize,
    work_dir: &Path,
    rollouts: &HashMap<String, RunView>,
) -> Selection {
    let mut sel = Selection {
        task_id: task_id.into(),
        mode: mode.into(),
        k,
        evidence: "deliverable".into(),
        selected_at: chrono::Utc::now().to_rfc3339(),
        ..Default::default()
    };
    let pool = selection_pool(mode, k, rollouts);
    sel.n_candidates = pool.len();
    if pool.is_empty() {
        sel.error = "empty selection pool".into();
        return sel;
    }
    let labels: Vec<_> = pool.keys().cloned().collect();
    let id_map = assign_ids(&labels, task_id, k, mode);
    sel.id_map = id_map.clone();

    let ctx = work_dir.join(format!("selection/{mode}{k}"));
    let _ = fs::remove_dir_all(&ctx);
    let _ = fs::create_dir_all(ctx.join("candidates"));
    let _ = copy_file_tree(&adapter.task_files(task_id), &ctx.join("source"));

    for (anon, label) in &id_map {
        if let Some(rv) = pool.get(label) {
            let text = adapter.deliverable_text(&rv.run_dir);
            let _ = fs::write(ctx.join("candidates").join(format!("{anon}.md")), text);
        }
    }

    match prop.call("Select the best candidate.", Some(&ctx), &format!("select.{mode}{k}")) {
        Ok(reply) => {
            sel.usd += reply.cost_usd;
            let Some(chosen_id) = parse_choice(&reply.text) else {
                sel.error = "no ```choice``` block".into();
                sel.fallback = true;
                return sel;
            };
            let Some(chosen_label) = id_map.get(&chosen_id).cloned() else {
                sel.error = format!("unknown choice id {chosen_id}");
                sel.fallback = true;
                return sel;
            };
            let Some(rv) = pool.get(&chosen_label) else {
                sel.error = format!("missing rollout for {chosen_label}");
                return sel;
            };
            sel.chosen_id = chosen_id;
            sel.chosen_label = chosen_label.clone();
            sel.chosen_run_id = rv.run_id.clone();
            sel.chosen_run_dir = rv.run_dir.to_string_lossy().into();
            sel.chosen_is_base = chosen_label.starts_with("base");
            sel.why = parse_why(&reply.text);
            sel.ok = true;
        }
        Err(ProposerError {
            message,
            cost_usd,
            ..
        }) => {
            sel.error = message;
            sel.usd = cost_usd;
            sel.fallback = true;
        }
    }
    sel
}

pub fn reflect_adoption(
    adapter: &dyn BenchmarkAdapter,
    prop: &mut dyn Proposer,
    task_id: &str,
    sel: &mut Selection,
    rollouts: &HashMap<String, RunView>,
    stages: &[Stage],
    work_dir: &Path,
) {
    if !sel.ok || sel.chosen_label.is_empty() {
        return;
    }
    let root = work_dir.join(format!("reflection/{}{}", sel.mode, sel.k));
    let harnesses = work_dir.join("harnesses");
    let chosen_dir = harnesses.join(&sel.chosen_label);
    let pool: Vec<(String, PathBuf, Option<RunView>)> = rollouts
        .iter()
        .filter(|(label, _)| !label.starts_with("base"))
        .map(|(label, rv)| (label.clone(), harnesses.join(label), Some(rv.clone())))
        .collect();
    let decisions: HashMap<String, String> = stages
        .iter()
        .map(|s| {
            (
                s.label.clone(),
                if s.notes.is_empty() {
                    s.error.clone()
                } else {
                    s.notes.clone()
                },
            )
        })
        .collect();
    let rollout_list: Vec<RunView> = rollouts.values().cloned().collect();
    if context::build(
        &root,
        adapter,
        task_id,
        &rollout_list,
        Some(&chosen_dir),
        None,
        &pool,
        &decisions,
    )
    .is_err()
    {
        return;
    }
    let _ = copy_file_tree(&adapter.base_harness(), &root.join("base_harness"));
    let _ = fs::write(
        root.join("adoption.json"),
        serde_json::json!({
            "chosen_label": sel.chosen_label,
            "candidate_ids": sel.id_map,
            "reason": sel.why,
        })
        .to_string(),
    );

    // Ensure decisions from compose stages exist even if notes empty.
    let _ = tree::write_tree;

    match prop.call(
        "Reflect on the adopted harness.",
        Some(&root),
        &format!("experience.{}{}", sel.mode, sel.k),
    ) {
        Ok(exp) => {
            sel.usd += exp.cost_usd;
            if exp.text.trim() != "NO_ENTRY" {
                sel.experience = parse_experience(&exp.text);
            }
        }
        Err(ProposerError {
            message,
            cost_usd,
            ..
        }) => {
            sel.usd += cost_usd;
            if sel.error.is_empty() {
                sel.error = message;
            }
        }
    }
}
