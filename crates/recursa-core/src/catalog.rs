//! Catalog-backed adapter: discovered dataset tasks → schedule rollout path.

use crate::benchmarks::{BenchmarkId, BenchmarkStub};
use crate::datasets::{self, TaskIndex};
use crate::proposer::{Proposer, ProposerError, Reply};
use crate::spec::{BenchSpec, BenchmarkAdapter, SpecError};
use crate::tree::{self, SurfaceSpec, Tree};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct CatalogAdapter {
    pub root: PathBuf,
    pub dataset_root: PathBuf,
    pub stub: BenchmarkStub,
    pub index: TaskIndex,
    pub spec: BenchSpec,
    surface: SurfaceSpec,
    pub execution_count: AtomicUsize,
}

impl CatalogAdapter {
    pub fn open(
        work_root: impl Into<PathBuf>,
        dataset_root: impl Into<PathBuf>,
        stub: BenchmarkStub,
    ) -> Result<Self, String> {
        let root = work_root.into();
        let dataset_root = dataset_root.into();
        let index_path = dataset_root.join("INDEX.json");
        let index = if index_path.is_file() {
            datasets::read_index(&index_path)?
        } else {
            datasets::materialise_stub_dataset(&dataset_root, &stub)?
        };
        let mut spec = stub.placeholder_spec(root.join("runs"));
        spec.name = match stub.id {
            BenchmarkId::Demo => "demo".into(),
            _ => stub.id.as_str().into(),
        };
        let surface = SurfaceSpec::opencode();
        let mut base: Tree = IndexMap::new();
        for name in &surface.always_on {
            base.insert(name.clone(), String::new());
        }
        base.insert(
            "systemprompt.md".into(),
            format!(
                "You are a Recursa catalog executor for {}.\nProduce the task deliverable.\n",
                stub.display
            ),
        );
        let base_dir = root.join("base");
        tree::write_tree(&surface, &base, &base_dir).map_err(|e| e.to_string())?;
        Ok(Self {
            root,
            dataset_root,
            stub,
            index,
            spec,
            surface,
            execution_count: AtomicUsize::new(0),
        })
    }

    pub fn tasks_by_domain(&self) -> HashMap<String, Vec<String>> {
        datasets::tasks_by_domain(&self.index)
    }

    pub fn on_disk_task_dir(&self, task_id: &str) -> PathBuf {
        // Native layouts: lab/jb use domain/task; wb uses subset/task_slug under dataset_root.
        let parts: Vec<_> = task_id.split('/').collect();
        match self.stub.id {
            BenchmarkId::WorkBuddy if parts.len() >= 2 => {
                self.dataset_root.join(parts[0]).join(parts[1])
            }
            BenchmarkId::JobBench if parts.len() >= 2 => self
                .dataset_root
                .join(parts[0])
                .join(parts[1])
                .join("task_folder"),
            _ if parts.len() >= 2 => self.dataset_root.join(parts[0]).join(parts[1]),
            _ => self.dataset_root.join(task_id.replace('/', "__")),
        }
    }

    fn read_prompt(&self, task_id: &str) -> String {
        let dir = self.on_disk_task_dir(task_id);
        for name in [
            "TASK_INSTRUCTIONS.txt",
            "instruction.md",
            "prompt.md",
            "task.json",
        ] {
            let p = dir.join(name);
            if p.is_file() {
                if name == "task.json" {
                    return fs::read_to_string(&p).unwrap_or_else(|_| {
                        format!("[{}] Produce the deliverable for {task_id}.", self.domain_of(task_id))
                    });
                }
                return fs::read_to_string(&p).unwrap_or_default();
            }
        }
        format!(
            "[{}] Produce the deliverable for {task_id}.",
            self.domain_of(task_id)
        )
    }
}

impl BenchmarkAdapter for CatalogAdapter {
    fn spec(&self) -> &BenchSpec {
        &self.spec
    }

    fn surface(&self) -> SurfaceSpec {
        self.surface.clone()
    }

    fn tasks(&self, _split: &str) -> Vec<String> {
        self.index.tasks.iter().map(|t| t.id.clone()).collect()
    }

    fn domain_of(&self, task_id: &str) -> String {
        self.index
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .map(|t| t.domain.clone())
            .unwrap_or_else(|| {
                task_id
                    .split('/')
                    .next()
                    .unwrap_or("unknown")
                    .to_string()
            })
    }

    fn task_prompt(&self, task_id: &str) -> String {
        self.read_prompt(task_id)
    }

    fn task_files(&self, task_id: &str) -> PathBuf {
        let dir = self.on_disk_task_dir(task_id);
        let source = dir.join("source");
        if source.is_dir() {
            source
        } else {
            dir
        }
    }

    fn base_harness(&self) -> PathBuf {
        self.root.join("base")
    }

    fn validate_tree(&self, harness: &Tree) -> Result<(), SpecError> {
        if !harness.contains_key("systemprompt.md") {
            return Err(SpecError::Msg(
                "catalog executor requires systemprompt.md".into(),
            ));
        }
        Ok(())
    }

    fn execute(
        &self,
        task_id: &str,
        harness_dir: Option<&Path>,
        run_id: &str,
        _timeout_secs: Option<u64>,
    ) -> Result<PathBuf, SpecError> {
        self.execution_count.fetch_add(1, Ordering::SeqCst);
        let run_dir = self.root.join("executions").join(run_id);
        fs::create_dir_all(&run_dir).map_err(|e| SpecError::Msg(e.to_string()))?;
        let harness = harness_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.base_harness());
        let prompt = self.task_prompt(task_id);
        let sys = fs::read_to_string(harness.join("systemprompt.md")).unwrap_or_default();
        // Dry stub: echo prompt; mark refined if harness differs from base.
        let refined = {
            let base_sys = fs::read_to_string(self.base_harness().join("systemprompt.md"))
                .unwrap_or_default();
            sys != base_sys
        };
        let answer = if refined {
            format!("OK: {task_id}\n{prompt}")
        } else {
            format!("BASE: {task_id}\n{prompt}")
        };
        fs::write(run_dir.join("answer.txt"), &answer).map_err(|e| SpecError::Msg(e.to_string()))?;
        fs::write(
            run_dir.join("trajectory.md"),
            format!("[ACTION] catalog stub execute\n[OBSERVATION]\n{answer}"),
        )
        .map_err(|e| SpecError::Msg(e.to_string()))?;
        Ok(run_dir)
    }

    fn delivered(&self, run_dir: &Path) -> bool {
        run_dir.join("answer.txt").is_file()
    }

    fn trajectory(&self, run_dir: &Path, _obs_trunc: usize) -> String {
        fs::read_to_string(run_dir.join("trajectory.md")).unwrap_or_default()
    }

    fn deliverable_text(&self, run_dir: &Path) -> String {
        fs::read_to_string(run_dir.join("answer.txt")).unwrap_or_default()
    }

    fn deliverable_files(&self, run_dir: &Path) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("answer.txt".into(), self.deliverable_text(run_dir));
        m
    }

    fn score(&self, run_dir: &Path) -> Option<HashMap<String, serde_json::Value>> {
        let text = fs::read_to_string(run_dir.join("answer.txt")).ok()?;
        let pass = text.starts_with("OK:");
        let mut m = HashMap::new();
        m.insert("n_passed".into(), serde_json::json!(u64::from(pass)));
        m.insert("n_criteria".into(), serde_json::json!(1));
        m.insert("all_pass".into(), serde_json::json!(pass));
        m.insert("metric".into(), serde_json::json!("catalog_ok_prefix"));
        Some(m)
    }
}

/// Dry-run proposer for catalog schedule: edit systemprompt, pick first candidate.
pub struct CatalogProposer;

impl CatalogProposer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CatalogProposer {
    fn default() -> Self {
        Self::new()
    }
}

impl Proposer for CatalogProposer {
    fn fork(&self) -> Option<Box<dyn Proposer>> {
        Some(Box::new(CatalogProposer))
    }

    fn call(
        &mut self,
        _mission: &str,
        context_dir: Option<&Path>,
        label: &str,
    ) -> Result<Reply, ProposerError> {
        let context = context_dir.ok_or_else(|| {
            ProposerError::new("catalog proposer requires a context directory", 1, 0.0)
        })?;
        let mut workspace = None;
        let text;
        if label.starts_with("select.") {
            let cand_dir = context.join("candidates");
            let mut names: Vec<_> = fs::read_dir(&cand_dir)
                .map_err(|e| ProposerError::new(e.to_string(), 1, 0.0))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
                .collect();
            names.sort();
            let chosen = names
                .first()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .ok_or_else(|| ProposerError::new("no candidates", 1, 0.0))?;
            // Prefer a candidate whose deliverable starts with OK:
            let mut pick = chosen;
            for path in &names {
                let body = fs::read_to_string(path).unwrap_or_default();
                if body.contains("OK:") {
                    pick = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or(pick);
                    break;
                }
            }
            text = format!("<why>Prefer refined catalog output.</why>\n```choice\n{pick}\n```");
        } else if label.starts_with("experience.") {
            text = "NO_ENTRY".into();
        } else {
            let ws = context.join("harness");
            let prompt_path = ws.join("systemprompt.md");
            let mut body = fs::read_to_string(&prompt_path).unwrap_or_default();
            if !body.contains("RECURSA_CATALOG_REFINED") {
                body.push_str("\n<!-- RECURSA_CATALOG_REFINED -->\n");
                let _ = fs::create_dir_all(ws.join(".."));
                let _ = fs::write(&prompt_path, body);
            }
            workspace = Some(ws);
            text = "<notes>Mark harness as refined for catalog dry-run.</notes>".into();
        }
        Ok(Reply {
            label: label.into(),
            text,
            ok: true,
            cost_usd: 0.0,
            attempts: 1,
            cached: false,
            workspace,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule;

    #[test]
    fn catalog_lab_runs_through_schedule() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("datasets/lab");
        let work = tmp.path().join("work");
        let stub = BenchmarkStub::lab();
        datasets::materialise_stub_dataset(&data, &stub).unwrap();
        let adapter = CatalogAdapter::open(&work, &data, stub).unwrap();
        let mut prop = CatalogProposer::new();
        let by = adapter.tasks_by_domain();
        let stream = schedule::stream_order(&by, Some(1), &[]).unwrap();
        assert!(!stream.is_empty());
        let (domain, task_id) = &stream[0];
        let row = schedule::run_task(
            &adapter,
            &mut prop,
            task_id,
            1,
            &adapter.spec.runs_root,
            &work.join("library/1"),
        );
        assert_eq!(row["status"], "ok", "{row}");
        assert_eq!(adapter.domain_of(task_id), *domain);
        let sel = &row["summary"]["selections"]["agg"];
        let chosen = PathBuf::from(sel["chosen_run_dir"].as_str().unwrap());
        assert!(
            adapter.deliverable_text(&chosen).starts_with("OK:"),
            "refined catalog deliverable expected"
        );
    }
}
