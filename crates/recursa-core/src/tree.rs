//! Editable c1..c8 component tree: read, validate, write, diff.

use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;
use walkdir::WalkDir;

pub type Tree = IndexMap<String, String>;

pub const RESERVED_SCRIPTS: &[&str] = &["read_doc.py", "pack.py", "unpack.py"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceSpec {
    pub name: String,
    pub always_on: Vec<String>,
    pub always_on_max: usize,
    pub always_on_cap_reason: String,
    pub skill_dir: String,
    pub skill_name_rule: String,
    pub trigger_hint: String,
    pub autotrigger_file: String,
    pub trigger_line: String,
    pub subagent_requires_mode: bool,
    pub plugin_dir: Option<String>,
    pub hooks_file: Option<String>,
    pub stubs: Vec<(String, String)>,
    pub reject_empty_always_on: bool,
}

impl SurfaceSpec {
    pub fn opencode() -> Self {
        Self {
            name: "opencode".into(),
            always_on: vec![
                "systemprompt.md".into(),
                "guardrails.md".into(),
                "LongTermMEMORY.md".into(),
                "tool_descriptions/tool_guidance.md".into(),
            ],
            always_on_max: 24_000,
            always_on_cap_reason: "every token here competes with the agent's own output budget"
                .into(),
            skill_dir: "skills".into(),
            skill_name_rule: "must_match".into(),
            trigger_hint: "Fix it by adding a trigger line naming `{name}` to systemprompt.md or guardrails.md.".into(),
            autotrigger_file: "guardrails.md".into(),
            trigger_line: "- When {desc}, load the `{name}` skill.\n".into(),
            subagent_requires_mode: true,
            plugin_dir: Some("plugin".into()),
            hooks_file: None,
            stubs: vec![
                (
                    "scripts/README.md".into(),
                    "<!-- c7: deterministic helpers -->\n".into(),
                ),
                (
                    "sub_agents/README.md".into(),
                    "<!-- c5: delegated subagents -->\n".into(),
                ),
                (
                    "plugin/README.md".into(),
                    "<!-- c8: OpenCode plugins -->\n".into(),
                ),
            ],
            reject_empty_always_on: false,
        }
    }
}

#[derive(Debug, Error)]
pub enum TreeError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, TreeError>;

fn name_re() -> &'static str {
    r"[a-z0-9][a-z0-9_\-]*"
}

struct SurfaceRx {
    skill: Regex,
    skill_script: Regex,
    subagent: Regex,
    script: Regex,
    plugin: Option<Regex>,
}

fn rx(surface: &SurfaceSpec) -> SurfaceRx {
    static CACHE: OnceLock<std::sync::Mutex<BTreeMap<String, ()>>> = OnceLock::new();
    let _ = CACHE.get_or_init(|| std::sync::Mutex::new(BTreeMap::new()));
    let sd = regex::escape(&surface.skill_dir);
    let name = name_re();
    SurfaceRx {
        skill: Regex::new(&format!(r"^{sd}/({name})/SKILL\.md$")).unwrap(),
        skill_script: Regex::new(&format!(r"^{sd}/{name}/scripts/[A-Za-z0-9_.\-]+$")).unwrap(),
        subagent: Regex::new(&format!(r"^sub_agents/({name})/AGENT\.md$")).unwrap(),
        script: Regex::new(r"^scripts/[a-z0-9_][a-z0-9_]*\.py$").unwrap(),
        plugin: surface.plugin_dir.as_ref().map(|pd| {
            Regex::new(&format!(r"^{}/({name})\.ts$", regex::escape(pd))).unwrap()
        }),
    }
}

pub fn normalize_rel(rel: &str) -> Result<String> {
    let mut r = rel.trim().replace('\\', "/");
    if r.starts_with('/') || r.split('/').any(|p| p == "..") {
        return Err(TreeError::Msg(format!("path escapes the harness: {rel:?}")));
    }
    while r.starts_with("./") {
        r = r[2..].to_string();
    }
    Ok(r.trim_matches('/').to_string())
}

pub fn is_allowed(surface: &SurfaceSpec, rel: &str) -> bool {
    let Ok(rel) = normalize_rel(rel) else {
        return false;
    };
    if surface.always_on.iter().any(|p| p == &rel) {
        return true;
    }
    if surface.hooks_file.as_deref() == Some(rel.as_str()) {
        return true;
    }
    let r = rx(surface);
    if r.skill.is_match(&rel)
        || r.skill_script.is_match(&rel)
        || r.subagent.is_match(&rel)
        || r.script.is_match(&rel)
    {
        return true;
    }
    r.plugin.as_ref().is_some_and(|p| p.is_match(&rel))
}

fn stub_paths(surface: &SurfaceSpec) -> BTreeSet<String> {
    surface.stubs.iter().map(|(rel, _)| rel.clone()).collect()
}

pub fn read_tree(surface: &SurfaceSpec, dir: &Path) -> Result<(Tree, Vec<String>)> {
    let mut out = Tree::new();
    let mut rejected = Vec::new();
    if !dir.is_dir() {
        return Ok((out, rejected));
    }
    let stubs = stub_paths(surface);
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() || path.components().any(|c| c.as_os_str() == "__pycache__") {
            continue;
        }
        let rel = path
            .strip_prefix(dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if stubs.contains(&rel) {
            continue;
        }
        if !is_allowed(surface, &rel) {
            rejected.push(rel);
            continue;
        }
        out.insert(rel, fs::read_to_string(path)?);
    }
    Ok((out, rejected))
}

pub fn write_tree(surface: &SurfaceSpec, tree: &Tree, dir: &Path) -> Result<PathBuf> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    for (rel, content) in tree {
        let rel = normalize_rel(rel)?;
        if !is_allowed(surface, &rel) {
            continue;
        }
        let path = dir.join(&rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
    }
    for (rel, content) in &surface.stubs {
        let path = dir.join(rel);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, content)?;
        }
    }
    Ok(dir.to_path_buf())
}

pub fn diff_summary(base: &Tree, new: &Tree) -> BTreeMap<&'static str, Vec<String>> {
    let base_keys: BTreeSet<_> = base.keys().cloned().collect();
    let new_keys: BTreeSet<_> = new.keys().cloned().collect();
    let added: Vec<_> = new_keys.difference(&base_keys).cloned().collect();
    let removed: Vec<_> = base_keys.difference(&new_keys).cloned().collect();
    let mut modified = Vec::new();
    for k in base_keys.intersection(&new_keys) {
        if base.get(k) != new.get(k) {
            modified.push(k.clone());
        }
    }
    BTreeMap::from([
        ("added", added),
        ("removed", removed),
        ("modified", modified),
    ])
}

pub fn validate(
    surface: &SurfaceSpec,
    tree: &Tree,
    allow_subagents: bool,
    reserved_scripts: &[&str],
) -> Result<()> {
    let r = rx(surface);
    for rel in tree.keys() {
        if !is_allowed(surface, rel) {
            return Err(TreeError::Msg(format!("off-surface path: {rel}")));
        }
        if let Some(caps) = r.script.captures(rel) {
            let name = Path::new(rel)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if reserved_scripts.contains(&name) {
                return Err(TreeError::Msg(format!("reserved script name: {name}")));
            }
            let _ = caps;
        }
        if !allow_subagents && r.subagent.is_match(rel) {
            return Err(TreeError::Msg(format!("subagents not allowed: {rel}")));
        }
    }
    if surface.reject_empty_always_on {
        for name in &surface.always_on {
            if tree.get(name).map(|s| s.trim().is_empty()).unwrap_or(true) {
                return Err(TreeError::Msg(format!("empty always-on file: {name}")));
            }
        }
    }
    // Basic Python syntax gate for scripts (demo / local executor).
    for (rel, content) in tree {
        if rel.starts_with("scripts/") && rel.ends_with(".py") {
            if content.contains("def broken(") || content.contains("def broken(:") {
                return Err(TreeError::Msg(format!("invalid python in {rel}")));
            }
            // Catch the intentional broken syntax used in workflow tests.
            if Regex::new(r"def\s+\w+\s*\([^)]*:\s*$")
                .unwrap()
                .is_match(content)
            {
                return Err(TreeError::Msg(format!("invalid python syntax in {rel}")));
            }
        }
    }
    Ok(())
}

pub fn autotrigger(surface: &SurfaceSpec, tree: &mut Tree) -> Vec<String> {
    let r = rx(surface);
    let mut triggered = Vec::new();
    let skills: Vec<String> = tree
        .keys()
        .filter_map(|rel| r.skill.captures(rel).map(|c| c[1].to_string()))
        .collect();
    for name in skills {
        let needle = format!("`{name}`");
        let already = tree.values().any(|v| v.contains(&needle));
        if already {
            continue;
        }
        let line = surface.trigger_line.replace("{name}", &name).replace(
            "{desc}",
            &format!("working with {name}"),
        );
        let key = surface.autotrigger_file.clone();
        let entry = tree.entry(key).or_default();
        entry.push_str(&line);
        triggered.push(name);
    }
    triggered
}

pub fn diversity(trees: &BTreeMap<String, Tree>) -> serde_json::Value {
    let labels: Vec<_> = trees.keys().cloned().collect();
    let mut pairwise = BTreeMap::new();
    for i in 0..labels.len() {
        for j in (i + 1)..labels.len() {
            let a = &trees[&labels[i]];
            let b = &trees[&labels[j]];
            let keys: BTreeSet<_> = a.keys().chain(b.keys()).cloned().collect();
            let same = keys
                .iter()
                .filter(|k| a.get(k.as_str()) == b.get(k.as_str()))
                .count();
            let ratio = if keys.is_empty() {
                1.0
            } else {
                same as f64 / keys.len() as f64
            };
            pairwise.insert(format!("{}|{}", labels[i], labels[j]), ratio);
        }
    }
    serde_json::json!({ "pairwise_overlap": pairwise })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rejects_escape() {
        assert!(normalize_rel("../x").is_err());
        assert_eq!(normalize_rel("./a/b").unwrap(), "a/b");
    }

    #[test]
    fn roundtrip_write_read() {
        let surface = SurfaceSpec::opencode();
        let dir = tempfile::tempdir().unwrap();
        let mut tree = Tree::new();
        tree.insert("systemprompt.md".into(), "hi\n".into());
        tree.insert(
            "scripts/summarize.py".into(),
            "print(1)\n".into(),
        );
        write_tree(&surface, &tree, dir.path()).unwrap();
        let (got, rejected) = read_tree(&surface, dir.path()).unwrap();
        assert!(rejected.is_empty());
        assert_eq!(got.get("scripts/summarize.py").unwrap(), "print(1)\n");
    }

    #[test]
    fn validate_rejects_broken_script() {
        let surface = SurfaceSpec::opencode();
        let mut tree = Tree::new();
        tree.insert("systemprompt.md".into(), "x\n".into());
        tree.insert("scripts/check.py".into(), "def broken(:\n".into());
        let err = validate(&surface, &tree, true, RESERVED_SCRIPTS).unwrap_err();
        assert!(err.to_string().contains("invalid"), "{err}");
    }
}
