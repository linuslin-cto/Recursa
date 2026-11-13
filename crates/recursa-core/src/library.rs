//! Per-domain experience library R.

use crate::selection::has_experience;
use crate::tree::{self, SurfaceSpec, Tree};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn slug(task_id: &str) -> String {
    task_id.replace('/', "__")
}

pub fn domain_dir(domain: &str) -> String {
    let re = regex::Regex::new(r"\s*[/\\]\s*").unwrap();
    let s = re.replace_all(domain, "__");
    let s = s.trim();
    if s.is_empty() {
        "unknown".into()
    } else {
        s.to_string()
    }
}

pub struct Library {
    pub root: PathBuf,
    pub domain: String,
    pub surface: SurfaceSpec,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>, domain: impl Into<String>, surface: SurfaceSpec) -> Self {
        Self {
            root: root.into(),
            domain: domain.into(),
            surface,
        }
    }

    pub fn dir(&self) -> PathBuf {
        self.root.join(domain_dir(&self.domain))
    }

    pub fn index(&self) -> PathBuf {
        self.dir().join("INDEX.md")
    }

    pub fn entry_dir(&self, task_id: &str) -> PathBuf {
        self.dir().join("harnesses").join(slug(task_id))
    }

    pub fn exists(&self) -> bool {
        self.index().is_file()
    }

    pub fn context_dir(&self) -> Option<PathBuf> {
        let d = self.dir();
        if self.exists()
            && walkdir::WalkDir::new(&d)
                .into_iter()
                .filter_map(|e| e.ok())
                .any(|e| e.file_name() == "EXPERIENCE.md")
        {
            Some(d)
        } else {
            None
        }
    }

    pub fn entries(&self) -> Vec<String> {
        let d = self.dir().join("harnesses");
        if !d.is_dir() {
            return Vec::new();
        }
        let mut names: Vec<_> = fs::read_dir(d)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    pub fn rebuild_index(&self) -> std::io::Result<()> {
        let dir = self.dir();
        fs::create_dir_all(&dir)?;
        let mut lines = vec![format!("# Library — {}\n", self.domain)];
        for name in self.entries() {
            lines.push(format!("- {name}"));
        }
        fs::write(self.index(), lines.join("\n") + "\n")
    }

    pub fn add(
        &self,
        task_id: &str,
        harness: &Tree,
        experience: &HashMap<String, String>,
        why: &str,
    ) -> Result<(), String> {
        if !has_experience(experience) {
            return Ok(());
        }
        let entry = self.entry_dir(task_id);
        let _ = fs::remove_dir_all(&entry);
        tree::write_tree(&self.surface, harness, &entry.join("harness"))
            .map_err(|e| e.to_string())?;
        let mut body = String::new();
        body.push_str(&format!("# Experience — {task_id}\n\n"));
        body.push_str(&format!("why: {why}\n"));
        for key in ["cue", "lesson", "evidence", "confidence"] {
            if let Some(v) = experience.get(key) {
                body.push_str(&format!("{key}: {v}\n"));
            }
        }
        fs::write(entry.join("EXPERIENCE.md"), body).map_err(|e| e.to_string())?;
        self.rebuild_index().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn tree_of(&self, task_id: &str) -> Tree {
        tree::read_tree(&self.surface, &self.entry_dir(task_id).join("harness"))
            .map(|(t, _)| t)
            .unwrap_or_default()
    }
}

pub fn experience_from_value(v: &Value) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(obj) = v.as_object() {
        for (k, val) in obj {
            if let Some(s) = val.as_str() {
                out.insert(k.clone(), s.to_string());
            }
        }
    }
    out
}

pub fn path_join(root: &Path, parts: &[&str]) -> PathBuf {
    parts.iter().fold(root.to_path_buf(), |acc, p| acc.join(p))
}
