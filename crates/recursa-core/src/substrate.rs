//! Solver substrate seam: OpenCode / Codex install + launch contracts.

use crate::opencode_protocol::{self, SessionParse};
use crate::proposer::ProposerError;
use crate::tree::{self, SurfaceSpec, Tree};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMode {
    WorkspaceAgentsMd,
    HomeDirJinja,
    WorkspaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstrateInfo {
    pub name: String,
    pub short: String,
    pub install_modes: Vec<InstallMode>,
    pub surface: SurfaceSpec,
}

#[derive(Debug, Clone)]
pub struct Install {
    pub staging_dir: PathBuf,
    pub fingerprint: String,
    pub extras: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub task_ref: String,
    pub run_ids: Vec<String>,
    pub timeout: u64,
}

pub trait Substrate: Send + Sync {
    fn name(&self) -> &str;
    fn short(&self) -> &str;
    fn surface(&self) -> SurfaceSpec;
    fn materialise(
        &self,
        tree: &Tree,
        staging: &Path,
        mode: InstallMode,
    ) -> Result<Install, String>;
    fn read_trajectory(&self, stream: &str, obs_trunc: usize) -> String;
    fn preflight(&self) -> Result<(), String>;
}

pub fn opencode() -> SubstrateInfo {
    SubstrateInfo {
        name: "opencode".into(),
        short: "oc".into(),
        install_modes: vec![
            InstallMode::WorkspaceAgentsMd,
            InstallMode::HomeDirJinja,
            InstallMode::WorkspaceConfig,
        ],
        surface: SurfaceSpec::opencode(),
    }
}

pub fn codex() -> SubstrateInfo {
    let mut surface = SurfaceSpec::opencode();
    surface.name = "codex".into();
    surface.plugin_dir = None;
    surface.hooks_file = Some("hooks.json".into());
    SubstrateInfo {
        name: "codex".into(),
        short: "cx".into(),
        install_modes: vec![InstallMode::WorkspaceAgentsMd],
        surface,
    }
}

fn fingerprint_tree(tree: &Tree) -> String {
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    let mut keys: Vec<_> = tree.keys().cloned().collect();
    keys.sort();
    for k in keys {
        h.update(k.as_bytes());
        h.update([0]);
        if let Some(v) = tree.get(&k) {
            h.update(v.as_bytes());
        }
        h.update([0]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()[..16].to_string()
}

/// OpenCode substrate: stages the editable tree and parses JSONL trajectories.
pub struct OpencodeSubstrate {
    pub info: SubstrateInfo,
}

impl Default for OpencodeSubstrate {
    fn default() -> Self {
        Self { info: opencode() }
    }
}

impl Substrate for OpencodeSubstrate {
    fn name(&self) -> &str {
        &self.info.name
    }
    fn short(&self) -> &str {
        &self.info.short
    }
    fn surface(&self) -> SurfaceSpec {
        self.info.surface.clone()
    }
    fn materialise(
        &self,
        tree: &Tree,
        staging: &Path,
        mode: InstallMode,
    ) -> Result<Install, String> {
        let _ = mode;
        tree::write_tree(&self.info.surface, tree, staging).map_err(|e| e.to_string())?;
        Ok(Install {
            staging_dir: staging.to_path_buf(),
            fingerprint: fingerprint_tree(tree),
            extras: HashMap::new(),
        })
    }
    fn read_trajectory(&self, stream: &str, obs_trunc: usize) -> String {
        let parse = opencode_protocol::parse_events(stream);
        let mut out = parse.text;
        if out.len() > obs_trunc {
            out.truncate(obs_trunc);
            out.push_str("…");
        }
        if !parse.error.is_empty() {
            out.push_str("\n[ERROR] ");
            out.push_str(&parse.error);
        }
        out
    }
    fn preflight(&self) -> Result<(), String> {
        // Live bun/opencode checks land with real executors; local demo skips.
        Ok(())
    }
}

/// Codex substrate stub — same staging, different surface/short name.
pub struct CodexSubstrate {
    pub info: SubstrateInfo,
}

impl Default for CodexSubstrate {
    fn default() -> Self {
        Self { info: codex() }
    }
}

impl Substrate for CodexSubstrate {
    fn name(&self) -> &str {
        &self.info.name
    }
    fn short(&self) -> &str {
        &self.info.short
    }
    fn surface(&self) -> SurfaceSpec {
        self.info.surface.clone()
    }
    fn materialise(
        &self,
        tree: &Tree,
        staging: &Path,
        mode: InstallMode,
    ) -> Result<Install, String> {
        let _ = mode;
        tree::write_tree(&self.info.surface, tree, staging).map_err(|e| e.to_string())?;
        // Codex uses AGENTS.md as the always-on entry; mirror systemprompt when present.
        if let Some(prompt) = tree.get("systemprompt.md") {
            let _ = fs::write(staging.join("AGENTS.md"), prompt);
        }
        Ok(Install {
            staging_dir: staging.to_path_buf(),
            fingerprint: fingerprint_tree(tree),
            extras: HashMap::new(),
        })
    }
    fn read_trajectory(&self, stream: &str, obs_trunc: usize) -> String {
        let mut out = stream.to_string();
        if out.len() > obs_trunc {
            out.truncate(obs_trunc);
            out.push_str("…");
        }
        out
    }
    fn preflight(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Drive a fixture JSONL session through the OpenCode protocol parser.
pub fn launch_opencode_fixture(
    stdout: &str,
    answer_file: Option<&str>,
) -> Result<(SessionParse, String), ProposerError> {
    let parse = opencode_protocol::parse_events(stdout);
    let answer = opencode_protocol::resolve_answer(&parse, answer_file)?;
    Ok((parse, answer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn substrate_shorts_are_stable() {
        assert_eq!(opencode().short, "oc");
        assert_eq!(codex().short, "cx");
    }

    #[test]
    fn opencode_materialise_and_fixture_launch() {
        let sub = OpencodeSubstrate::default();
        let dir = tempfile::tempdir().unwrap();
        let mut tree = IndexMap::new();
        tree.insert("systemprompt.md".into(), "hi\n".into());
        tree.insert("scripts/summarize.py".into(), "print(1)\n".into());
        let install = sub
            .materialise(&tree, &dir.path().join("stage"), InstallMode::WorkspaceConfig)
            .unwrap();
        assert!(install.staging_dir.join("scripts/summarize.py").is_file());
        assert_eq!(install.fingerprint.len(), 16);

        let stdout = r#"{"type":"text","part":{"text":"done"}}
{"type":"step_finish","part":{"cost":0.1}}
"#;
        let (parse, answer) = launch_opencode_fixture(stdout, None).unwrap();
        assert_eq!(answer, "done");
        assert!((parse.cost_usd - 0.1).abs() < 1e-9);
    }

    #[test]
    fn codex_writes_agents_md() {
        let sub = CodexSubstrate::default();
        let dir = tempfile::tempdir().unwrap();
        let mut tree = IndexMap::new();
        tree.insert("systemprompt.md".into(), "codex rules\n".into());
        let install = sub
            .materialise(&tree, dir.path(), InstallMode::WorkspaceAgentsMd)
            .unwrap();
        assert_eq!(
            fs::read_to_string(install.staging_dir.join("AGENTS.md")).unwrap(),
            "codex rules\n"
        );
    }
}
