//! Benchmark adapter stubs for JobBench / LAB / WorkBuddy.
//!
//! Full data+grader wiring stays behind setup; these stubs lock names, prompts,
//! and domain vocabulary so the method layer can schedule against them.

use crate::spec::{BenchPrompts, BenchSpec, ChannelSpec};
use crate::substrate::{self, InstallMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkId {
    JobBench,
    Lab,
    WorkBuddy,
    Demo,
}

impl BenchmarkId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JobBench => "jb",
            Self::Lab => "lab",
            Self::WorkBuddy => "wb",
            Self::Demo => "demo",
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "jb" | "jobbench" => Some(Self::JobBench),
            "lab" => Some(Self::Lab),
            "wb" | "workbuddy" => Some(Self::WorkBuddy),
            "demo" => Some(Self::Demo),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkStub {
    pub id: BenchmarkId,
    pub display: String,
    pub install_mode: InstallMode,
    pub substrate_short: String,
    pub prompts: BenchPrompts,
    pub domains: Vec<String>,
}

impl BenchmarkStub {
    pub fn jobbench() -> Self {
        Self {
            id: BenchmarkId::JobBench,
            display: "JobBench".into(),
            install_mode: InstallMode::WorkspaceAgentsMd,
            substrate_short: "oc".into(),
            prompts: BenchPrompts::simple("professional workplace deliverables"),
            domains: vec![
                "accounting".into(),
                "hr".into(),
                "operations".into(),
            ],
        }
    }

    pub fn lab() -> Self {
        Self {
            id: BenchmarkId::Lab,
            display: "LAB".into(),
            install_mode: InstallMode::WorkspaceConfig,
            substrate_short: "oc".into(),
            prompts: BenchPrompts::simple("legal analysis and memo drafting"),
            domains: vec![
                "antitrust-competition".into(),
                "contracts".into(),
            ],
        }
    }

    pub fn workbuddy() -> Self {
        Self {
            id: BenchmarkId::WorkBuddy,
            display: "WorkBuddy-Bench".into(),
            install_mode: InstallMode::HomeDirJinja,
            substrate_short: "oc".into(),
            prompts: BenchPrompts::simple("office, web, and code workplace tasks"),
            domains: vec!["office".into(), "web".into(), "code".into()],
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::jobbench(), Self::lab(), Self::workbuddy()]
    }

    pub fn placeholder_spec(&self, runs_root: PathBuf) -> BenchSpec {
        let channel = ChannelSpec::new("opencode", "local/stub").expect("channel");
        let mut channels = HashMap::new();
        channels.insert("proposer".into(), channel.clone());
        channels.insert("selector".into(), channel);
        BenchSpec {
            name: self.id.as_str().into(),
            runs_root,
            install_mode: match self.install_mode {
                InstallMode::WorkspaceAgentsMd => "workspace_agents_md",
                InstallMode::HomeDirJinja => "home_dir_jinja",
                InstallMode::WorkspaceConfig => "workspace_config",
            }
            .into(),
            prompts: self.prompts.clone(),
            channels,
            leak_globs: vec!["reference_answer.txt".into(), "scores.json".into()],
            allow_subagents: true,
            reserved_scripts: Vec::new(),
        }
    }

    pub fn substrate_info(&self) -> substrate::SubstrateInfo {
        match self.substrate_short.as_str() {
            "cx" => substrate::codex(),
            _ => substrate::opencode(),
        }
    }

    /// Fixture task ids for dry-run catalogs.
    ///
    /// JobBench includes two accounting tasks so harnessing-level library
    /// carry-over (task-1 → task-2) can be verified offline.
    pub fn fixture_tasks(&self) -> Vec<(String, String)> {
        match self.id {
            BenchmarkId::JobBench => vec![
                ("accounting/task-1".into(), "accounting".into()),
                ("accounting/task-2".into(), "accounting".into()),
                ("hr/task-1".into(), "hr".into()),
                ("operations/task-1".into(), "operations".into()),
            ],
            _ => self
                .domains
                .iter()
                .map(|d| (format!("{d}/task-1"), d.clone()))
                .collect(),
        }
    }
}

/// On-disk fixture catalog: `<root>/<task_id>/prompt.md` + `source/`.
#[derive(Debug, Clone)]
pub struct TaskCatalog {
    pub root: PathBuf,
    pub stub: BenchmarkStub,
}

impl TaskCatalog {
    pub fn materialise_fixtures(&self) -> std::io::Result<()> {
        for (task_id, domain) in self.stub.fixture_tasks() {
            let dir = self.root.join(task_id.replace('/', "__"));
            std::fs::create_dir_all(dir.join("source"))?;
            std::fs::write(
                dir.join("prompt.md"),
                format!("[{domain}] Produce the deliverable for {task_id}.\n"),
            )?;
            std::fs::write(
                dir.join("source/brief.txt"),
                format!("fixture brief for {task_id}\n"),
            )?;
        }
        Ok(())
    }

    pub fn list_tasks(&self) -> Vec<String> {
        self.stub
            .fixture_tasks()
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    pub fn domain_of(&self, task_id: &str) -> String {
        self.stub
            .fixture_tasks()
            .into_iter()
            .find(|(id, _)| id == task_id)
            .map(|(_, d)| d)
            .unwrap_or_else(|| "unknown".into())
    }

    pub fn task_dir(&self, task_id: &str) -> PathBuf {
        self.root.join(task_id.replace('/', "__"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stubs_cover_three_benchmarks() {
        let all = BenchmarkStub::all();
        assert_eq!(all.len(), 3);
        assert_eq!(BenchmarkId::from_token("jb"), Some(BenchmarkId::JobBench));
        assert_eq!(all[1].id.as_str(), "lab");
        assert_eq!(all[2].domains, ["office", "web", "code"]);
    }

    #[test]
    fn fixture_catalog_materialises_per_domain() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = TaskCatalog {
            root: dir.path().to_path_buf(),
            stub: BenchmarkStub::lab(),
        };
        catalog.materialise_fixtures().unwrap();
        assert_eq!(catalog.list_tasks().len(), 2);
        assert!(catalog
            .task_dir("contracts/task-1")
            .join("source/brief.txt")
            .is_file());
        assert_eq!(catalog.domain_of("contracts/task-1"), "contracts");
    }
}
