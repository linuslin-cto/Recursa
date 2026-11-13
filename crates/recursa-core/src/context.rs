//! Evidence packing for improver sessions.

use crate::spec::BenchmarkAdapter;
use crate::spec::RunView;
use crate::tree::{self, Tree};
use anyhow::{Context as _, Result};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ContextTree {
    pub root: PathBuf,
}

pub fn assert_tree_has_no_grader_verdicts(tree: &Tree) -> Result<(), String> {
    for (rel, content) in tree {
        if content.contains("\"pass_rate\"") || content.contains("\"n_passed\"") {
            return Err(format!(
                "harness tree leaks grader verdicts via {rel}: grader fields must not enter the editable surface"
            ));
        }
    }
    Ok(())
}

pub fn mission(tree: &ContextTree, job: &str) -> String {
    format!(
        "# MISSION\n\n{job}\n\nEdit files under `{}/harness/`.\n",
        tree.root.display()
    )
}

fn copy_file(src: &Path, dst: &Path, prefer_link: bool) -> std::io::Result<u64> {
    // Hardlink only for read-only evidence; writable harness trees must be real copies.
    if prefer_link {
        if fs::hard_link(src, dst).is_ok() {
            return Ok(0);
        }
    }
    fs::copy(src, dst)
}

fn copy_dir(src: &Path, dst: &Path, prefer_link: bool) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }
    fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let rel = path.strip_prefix(src)?;
        let target = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&target)?;
        } else if path.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            // Drop leak globs later at adapter boundary; skip reference answers by name.
            if path.file_name().and_then(|s| s.to_str()) == Some("reference_answer.txt") {
                continue;
            }
            copy_file(path, &target, prefer_link)?;
        }
    }
    Ok(())
}

pub fn build(
    ctx_dir: &Path,
    adapter: &dyn BenchmarkAdapter,
    task_id: &str,
    rollouts: &[RunView],
    harness_dir: Option<&Path>,
    library_dir: Option<&Path>,
    pool: &[(String, PathBuf, Option<RunView>)],
    decisions: &HashMap<String, String>,
) -> Result<ContextTree> {
    if ctx_dir.exists() {
        fs::remove_dir_all(ctx_dir)?;
    }
    fs::create_dir_all(ctx_dir)?;

    let source = adapter.task_files(task_id);
    copy_dir(&source, &ctx_dir.join("source"), true)?;

    if let Some(h) = harness_dir {
        // Writable edit surface — must not hardlink to the staged base.
        copy_dir(h, &ctx_dir.join("harness"), false)?;
        copy_dir(h, &ctx_dir.join("base_harness"), false)?;
    }

    let roll_dir = ctx_dir.join("rollouts");
    fs::create_dir_all(&roll_dir)?;
    for rv in rollouts {
        let text = if rv.run_dir.exists() {
            adapter.trajectory(&rv.run_dir, 4000)
        } else {
            format!("missing rollout: {}", rv.error)
        };
        fs::write(roll_dir.join(format!("{}.md", rv.label)), text)?;
    }

    if !pool.is_empty() {
        let pool_dir = ctx_dir.join("pool");
        for (label, harness, rollout) in pool {
            let entry = pool_dir.join(label);
            fs::create_dir_all(&entry)?;
            if harness.exists() {
                copy_dir(harness, &entry.join("harness"), true)?;
                // Simple whole-tree diff marker for evidence.
                let surface = adapter.surface();
                let (base, _) = tree::read_tree(&surface, harness_dir.unwrap_or(harness))?;
                let (cand, _) = tree::read_tree(&surface, harness)?;
                let diff = tree::diff_summary(&base, &cand);
                fs::write(
                    entry.join("harness.diff"),
                    serde_json::to_string_pretty(&diff)?,
                )?;
            }
            if let Some(rv) = rollout {
                let text = if rv.run_dir.exists() {
                    adapter.trajectory(&rv.run_dir, 4000)
                } else {
                    rv.error.clone()
                };
                fs::write(entry.join("rollout.md"), text)?;
            }
        }
    }

    if !decisions.is_empty() {
        let ddir = ctx_dir.join("decisions");
        fs::create_dir_all(&ddir)?;
        for (label, notes) in decisions {
            fs::write(ddir.join(format!("{label}.md")), notes)?;
        }
    }

    if let Some(lib) = library_dir {
        if lib.exists() {
            copy_dir(lib, &ctx_dir.join("library"), true)?;
        }
    }

    fs::write(
        ctx_dir.join("adoption.json"),
        serde_json::to_string_pretty(&json!({
            "task_id": task_id,
            "prompt": adapter.task_prompt(task_id),
        }))?,
    )
    .context("write adoption.json")?;

    Ok(ContextTree {
        root: ctx_dir.to_path_buf(),
    })
}
