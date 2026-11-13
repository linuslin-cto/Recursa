//! Benchmark seam: method / substrate / benchmark stay separate.

use crate::tree::{SurfaceSpec, Tree};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const CHANNELS: &[&str] = &["claude_cli", "opencode"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSpec {
    pub kind: String,
    pub model: String,
    pub effort: String,
    pub steps: usize,
    pub timeout: u64,
}

impl ChannelSpec {
    pub fn new(kind: impl Into<String>, model: impl Into<String>) -> Result<Self, SpecError> {
        let kind = kind.into();
        if !CHANNELS.contains(&kind.as_str()) {
            return Err(SpecError::Msg(format!(
                "unknown channel {kind:?}; expected one of {CHANNELS:?}"
            )));
        }
        Ok(Self {
            kind,
            model: model.into(),
            effort: "high".into(),
            steps: 120,
            timeout: 3600,
        })
    }
}

pub fn resolve_channel_model(
    kind: Option<&str>,
    model: Option<&str>,
) -> Result<ChannelSpec, SpecError> {
    let use_default = kind.is_none() && model.is_none();
    if kind.is_none() != model.is_none() {
        return Err(SpecError::Msg(format!(
            "channel and model must be overridden TOGETHER; got kind={kind:?}, model={model:?}"
        )));
    }
    let kind = kind.unwrap_or(crate::config::DEFAULT_PROPOSER_CHANNEL);
    if !CHANNELS.contains(&kind) {
        return Err(SpecError::Msg(format!(
            "unknown channel {kind:?}; expected one of {CHANNELS:?}"
        )));
    }
    let model = model
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            crate::config::proposer_model_by_channel(kind)
                .unwrap_or("")
                .to_string()
        });
    if model.trim().is_empty() {
        if use_default {
            return ChannelSpec::new(kind, "");
        }
        return Err(SpecError::Msg("model override must not be empty".into()));
    }
    if kind == "opencode" && !model.contains('/') {
        return Err(SpecError::Msg(format!(
            "opencode channel needs a provider-prefixed model id, got {model:?}"
        )));
    }
    if kind == "claude_cli" && model.contains('/') {
        return Err(SpecError::Msg(format!(
            "claude_cli channel takes a bare Anthropic id, got {model:?}"
        )));
    }
    ChannelSpec::new(kind, model)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchPrompts {
    pub work: String,
    pub domain_word: String,
    pub inv_levers: String,
    pub dom_levers: String,
    pub yardstick: String,
    pub deliverable_word: String,
    pub domain_guidance: String,
    pub selection_checks: String,
}

impl BenchPrompts {
    pub fn simple(work: impl Into<String>) -> Self {
        Self {
            work: work.into(),
            domain_word: "domain".into(),
            inv_levers: String::new(),
            dom_levers: String::new(),
            yardstick: String::new(),
            deliverable_word: "deliverable".into(),
            domain_guidance: String::new(),
            selection_checks: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSpec {
    pub name: String,
    pub runs_root: PathBuf,
    pub install_mode: String,
    pub prompts: BenchPrompts,
    pub channels: HashMap<String, ChannelSpec>,
    pub leak_globs: Vec<String>,
    pub allow_subagents: bool,
    pub reserved_scripts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunView {
    pub label: String,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub ok: bool,
    pub delivered: bool,
    pub error: String,
}

impl RunView {
    pub fn missing(label: impl Into<String>, run_id: impl Into<String>, work_dir: &Path) -> Self {
        Self {
            label: label.into(),
            run_id: run_id.into(),
            run_dir: work_dir.join("missing"),
            ok: false,
            delivered: false,
            error: String::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SpecError {
    #[error("{0}")]
    Msg(String),
}

/// What every benchmark adapter must supply.
pub trait BenchmarkAdapter: Send + Sync {
    fn spec(&self) -> &BenchSpec;
    fn surface(&self) -> SurfaceSpec;
    fn tasks(&self, split: &str) -> Vec<String>;
    fn domain_of(&self, task_id: &str) -> String;
    fn task_prompt(&self, task_id: &str) -> String;
    fn task_files(&self, task_id: &str) -> PathBuf;
    fn base_harness(&self) -> PathBuf;
    fn validate_tree(&self, harness: &Tree) -> Result<(), SpecError>;
    fn execute(
        &self,
        task_id: &str,
        harness_dir: Option<&Path>,
        run_id: &str,
        timeout_secs: Option<u64>,
    ) -> Result<PathBuf, SpecError>;
    fn delivered(&self, run_dir: &Path) -> bool;
    fn trajectory(&self, run_dir: &Path, obs_trunc: usize) -> String;
    fn deliverable_text(&self, run_dir: &Path) -> String;
    fn deliverable_files(&self, run_dir: &Path) -> HashMap<String, String>;
    fn score(&self, _run_dir: &Path) -> Option<HashMap<String, serde_json::Value>> {
        None
    }
}

pub fn resolve_prompts(adapter: &dyn BenchmarkAdapter, _task_id: &str) -> BenchPrompts {
    adapter.spec().prompts.clone()
}
