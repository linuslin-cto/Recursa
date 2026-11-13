//! On-disk task dataset discovery for jb / lab / wb layouts.
//!
//! Layouts (relative to a benchmark root):
//! - jb:  `*/*/task_folder/TASK_INSTRUCTIONS.txt` → id=`domain/task`, domain=`domain`
//! - lab: `*/*/task.json` → id=`domain/task`, domain=`domain`
//! - wb:  `*/task.toml` + `instruction.md` under a subset tasks root;
//!        domain from `metadata.role` (code) or `metadata.category` (office/web)
//!
//! Stub materialisation writes the same shapes so offline tests need no HF checkout.

use crate::benchmarks::{BenchmarkId, BenchmarkStub};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    pub id: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIndex {
    pub benchmark: String,
    pub tasks: Vec<TaskEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitFile {
    pub benchmark: String,
    pub train: Vec<String>,
    pub test: Vec<String>,
}

pub fn write_index(path: &Path, index: &TaskIndex) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(index).unwrap() + "\n")
}

pub fn read_index(path: &Path) -> Result<TaskIndex, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Parent dir for a bench catalog: `<base>/<bench>`.
pub fn dataset_root(base: &Path, bench: &str) -> PathBuf {
    base.join(bench)
}

pub fn tasks_by_domain(index: &TaskIndex) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for t in &index.tasks {
        map.entry(t.domain.clone())
            .or_insert_with(Vec::new)
            .push(t.id.clone());
    }
    for v in map.values_mut() {
        v.sort();
    }
    map
}

/// Discover task id → domain from an on-disk checkout (or stub tree).
pub fn discover(bench: &str, root: &Path, subset: Option<&str>) -> Result<BTreeMap<String, String>, String> {
    let bench = bench.trim().to_ascii_lowercase();
    let tasks = match bench.as_str() {
        "jb" | "jobbench" => discover_jb(root)?,
        "lab" => discover_lab(root)?,
        "wb" | "workbuddy" => {
            let subset = subset.ok_or_else(|| {
                "wb discover requires subset office|web|code".to_string()
            })?;
            discover_wb(root, subset)?
        }
        _ => return Err(format!("Unknown benchmark/subset: {bench}/{:?}", subset)),
    };
    if tasks.is_empty() {
        return Err(format!(
            "No {bench} tasks found at {}. Materialise fixtures or fetch the dataset.",
            root.display()
        ));
    }
    Ok(tasks)
}

fn discover_jb(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut tasks = BTreeMap::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.file_name().and_then(|s| s.to_str()) != Some("TASK_INSTRUCTIONS.txt") {
            continue;
        }
        // .../<domain>/<task>/task_folder/TASK_INSTRUCTIONS.txt
        let task_folder = path.parent().ok_or_else(|| format!("bad path {}", path.display()))?;
        if task_folder.file_name().and_then(|s| s.to_str()) != Some("task_folder") {
            continue;
        }
        let task_dir = task_folder.parent().ok_or_else(|| format!("bad path {}", path.display()))?;
        let rel = task_dir
            .strip_prefix(root)
            .map_err(|_| format!("{} not under {}", task_dir.display(), root.display()))?;
        let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        if parts.len() < 2 {
            continue;
        }
        let id = parts.join("/");
        let domain = parts[0].clone();
        tasks.insert(id, domain);
    }
    Ok(tasks)
}

fn discover_lab(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut tasks = BTreeMap::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.file_name().and_then(|s| s.to_str()) != Some("task.json") {
            continue;
        }
        let task_dir = path.parent().ok_or_else(|| format!("bad path {}", path.display()))?;
        let rel = task_dir
            .strip_prefix(root)
            .map_err(|_| format!("{} not under {}", task_dir.display(), root.display()))?;
        let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        if parts.len() < 2 {
            continue;
        }
        let id = parts.join("/");
        let domain = parts[0].clone();
        tasks.insert(id, domain);
    }
    Ok(tasks)
}

fn discover_wb(root: &Path, subset: &str) -> Result<BTreeMap<String, String>, String> {
    let subset = subset.trim().to_ascii_lowercase();
    if !matches!(subset.as_str(), "office" | "web" | "code") {
        return Err(format!("Unknown wb subset {subset}"));
    }
    let field = if subset == "code" { "role" } else { "category" };
    let mut tasks = BTreeMap::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let task_dir = entry.path();
        let toml_path = task_dir.join("task.toml");
        let instr = task_dir.join("instruction.md");
        if !toml_path.is_file() {
            continue;
        }
        if !instr.is_file() {
            return Err(format!("Missing task instructions: {}", instr.display()));
        }
        let text = fs::read_to_string(&toml_path).map_err(|e| e.to_string())?;
        let domain = parse_toml_metadata_field(&text, field).ok_or_else(|| {
            format!("{}: metadata.{field} must be a nonempty string", toml_path.display())
        })?;
        let id = entry.file_name().to_string_lossy().into_owned();
        tasks.insert(id, domain);
    }
    Ok(tasks)
}

/// Minimal `[metadata]` table reader — avoids a toml crate dep for one field.
fn parse_toml_metadata_field(text: &str, field: &str) -> Option<String> {
    let mut in_meta = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_meta = t == "[metadata]";
            continue;
        }
        if !in_meta || t.is_empty() || t.starts_with('#') {
            continue;
        }
        let Some((k, v)) = t.split_once('=') else { continue };
        if k.trim() != field {
            continue;
        }
        let v = v.trim().trim_matches('"').trim_matches('\'').trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    None
}

/// Select task IDs for a split (`all` / `train` / `test`).
pub fn select_split(
    tasks: &BTreeMap<String, String>,
    split: &str,
    path: Option<&Path>,
    benchmark: &str,
) -> Result<Vec<String>, String> {
    if !matches!(split, "all" | "train" | "test") {
        return Err(format!("Unknown split {split:?}; use all, train, or test"));
    }
    match path {
        None => {
            if split != "all" {
                return Err(
                    "--split train/test requires a split file; use split all or pass a path"
                        .into(),
                );
            }
            Ok(tasks.keys().cloned().collect())
        }
        Some(path) => {
            let data: SplitFile = serde_json::from_str(
                &fs::read_to_string(path).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if data.benchmark != benchmark {
                return Err(format!("{}: expected a split for {benchmark}", path.display()));
            }
            for (name, ids) in [("train", &data.train), ("test", &data.test)] {
                let set: HashSet<&String> = ids.iter().collect();
                if set.len() != ids.len() {
                    return Err(format!("{}: duplicate task IDs in {name}", path.display()));
                }
            }
            let train: HashSet<_> = data.train.iter().cloned().collect();
            let test: HashSet<_> = data.test.iter().cloned().collect();
            if !train.is_disjoint(&test) {
                return Err(format!("{}: train and test overlap", path.display()));
            }
            let known: HashSet<_> = tasks.keys().cloned().collect();
            let unknown: Vec<_> = train.union(&test).filter(|id| !known.contains(*id)).cloned().collect();
            if !unknown.is_empty() {
                return Err(format!(
                    "{}: task IDs absent from the dataset: {:?}",
                    path.display(),
                    &unknown[..unknown.len().min(5)]
                ));
            }
            Ok(match split {
                "train" => {
                    let mut v = data.train;
                    v.sort();
                    v
                }
                "test" => {
                    let mut v = data.test;
                    v.sort();
                    v
                }
                _ => {
                    let mut v: Vec<_> = train.union(&test).cloned().collect();
                    v.sort();
                    v
                }
            })
        }
    }
}

/// Write native-layout fixtures for a stub and an INDEX.json.
pub fn materialise_stub_dataset(root: &Path, stub: &BenchmarkStub) -> Result<TaskIndex, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    match stub.id {
        BenchmarkId::JobBench => {
            for (id, domain) in stub.fixture_tasks() {
                let task_name = id.split('/').nth(1).unwrap_or("task-1");
                let dir = root.join(&domain).join(task_name).join("task_folder");
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                fs::write(
                    dir.join("TASK_INSTRUCTIONS.txt"),
                    format!("[{domain}] Produce the deliverable for {id}.\n"),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        BenchmarkId::Lab => {
            for (id, domain) in stub.fixture_tasks() {
                let task_name = id.split('/').nth(1).unwrap_or("task-1");
                let dir = root.join(&domain).join(task_name);
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let meta = serde_json::json!({"id": id, "domain": domain});
                fs::write(dir.join("task.json"), meta.to_string() + "\n").map_err(|e| e.to_string())?;
            }
        }
        BenchmarkId::WorkBuddy => {
            // Materialise each subset under <root>/<subset>/<task>/…
            for (id, domain) in stub.fixture_tasks() {
                let subset = domain.as_str(); // office|web|code
                let task_slug = id.split('/').nth(1).unwrap_or("task-1");
                let dir = root.join(subset).join(task_slug);
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let field = if subset == "code" { "role" } else { "category" };
                fs::write(
                    dir.join("task.toml"),
                    format!("[metadata]\n{field} = \"{domain}\"\n"),
                )
                .map_err(|e| e.to_string())?;
                fs::write(
                    dir.join("instruction.md"),
                    format!("[{domain}] Produce the deliverable for {id}.\n"),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        BenchmarkId::Demo => return Err("demo has no dataset catalog".into()),
    }

    let map = match stub.id {
        BenchmarkId::WorkBuddy => {
            let mut all = BTreeMap::new();
            for subset in &["office", "web", "code"] {
                let sub_root = root.join(subset);
                if sub_root.is_dir() {
                    for (id, domain) in discover("wb", &sub_root, Some(subset))? {
                        // Prefix subset so INDEX ids stay unique across subsets.
                        all.insert(format!("{subset}/{id}"), domain);
                    }
                }
            }
            if all.is_empty() {
                return Err(format!("No wb tasks found under {}", root.display()));
            }
            all
        }
        _ => discover(stub.id.as_str(), root, None)?,
    };

    let tasks: Vec<TaskEntry> = map
        .into_iter()
        .map(|(id, domain)| TaskEntry { id, domain })
        .collect();
    let index = TaskIndex {
        benchmark: stub.id.as_str().into(),
        tasks,
    };
    write_index(&root.join("INDEX.json"), &index).map_err(|e| e.to_string())?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::stream_order;

    #[test]
    fn materialise_lab_discover_and_stream() {
        let dir = tempfile::tempdir().unwrap();
        let stub = BenchmarkStub::lab();
        let index = materialise_stub_dataset(dir.path(), &stub).unwrap();
        assert_eq!(index.tasks.len(), 2);
        let found = discover("lab", dir.path(), None).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.contains_key("contracts/task-1"));
        let by = tasks_by_domain(&index);
        let stream = stream_order(&by, Some(1), &[]).unwrap();
        assert_eq!(stream.len(), 2);
    }

    #[test]
    fn materialise_jb_and_wb() {
        let dir = tempfile::tempdir().unwrap();
        let jb = materialise_stub_dataset(&dir.path().join("jb"), &BenchmarkStub::jobbench()).unwrap();
        assert_eq!(jb.tasks.len(), 3);
        assert!(dir
            .path()
            .join("jb/accounting/task-1/task_folder/TASK_INSTRUCTIONS.txt")
            .is_file());

        let wb_root = dir.path().join("wb");
        let wb = materialise_stub_dataset(&wb_root, &BenchmarkStub::workbuddy()).unwrap();
        assert_eq!(wb.tasks.len(), 3);
        let office = discover("wb", &wb_root.join("office"), Some("office")).unwrap();
        assert_eq!(office.len(), 1);
    }

    #[test]
    fn select_split_train_test() {
        let dir = tempfile::tempdir().unwrap();
        let index = materialise_stub_dataset(dir.path(), &BenchmarkStub::lab()).unwrap();
        let map: BTreeMap<_, _> = index
            .tasks
            .iter()
            .map(|t| (t.id.clone(), t.domain.clone()))
            .collect();
        let split_path = dir.path().join("split.json");
        let split = SplitFile {
            benchmark: "lab".into(),
            train: vec!["antitrust-competition/task-1".into()],
            test: vec!["contracts/task-1".into()],
        };
        fs::write(&split_path, serde_json::to_string_pretty(&split).unwrap()).unwrap();
        let train = select_split(&map, "train", Some(&split_path), "lab").unwrap();
        assert_eq!(train, ["antitrust-competition/task-1"]);
        let all = select_split(&map, "all", None, "lab").unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn dataset_root_is_base_join_bench() {
        assert_eq!(
            dataset_root(Path::new("datasets"), "lab"),
            PathBuf::from("datasets/lab")
        );
    }
}
