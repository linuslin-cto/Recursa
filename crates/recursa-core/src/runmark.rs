//! Rollout completion markers and task-input fingerprints.

use sha1::{Digest, Sha1};
use std::fs;
use std::path::{Path, PathBuf};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Stable digest of the task prompt plus every source file (byproducts ignored).
pub fn inputs_fingerprint(prompt: &str, files: Option<&Path>) -> String {
    let mut hasher = Sha1::new();
    hasher.update(prompt.as_bytes());
    hasher.update([0]);
    if let Some(root) = files {
        if root.is_dir() {
            let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(root)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .filter(|e| {
                    !e.path()
                        .components()
                        .any(|c| c.as_os_str() == "__pycache__" || c.as_os_str() == ".git")
                })
                .map(|e| e.path().to_path_buf())
                .collect();
            paths.sort();
            for path in paths {
                let rel = path
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                hasher.update(rel.as_bytes());
                hasher.update([0]);
                if let Ok(bytes) = fs::read(&path) {
                    hasher.update(&bytes);
                }
                hasher.update([0]);
            }
        }
    }
    hex(&hasher.finalize())[..16].to_string()
}

const MARKER: &str = ".harness2_complete";

pub fn mark_complete(run_dir: &Path, run_id: &str, inputs: &str) {
    let _ = fs::create_dir_all(run_dir);
    let body = serde_json::json!({
        "run_id": run_id,
        "inputs": inputs,
    });
    let _ = fs::write(run_dir.join(MARKER), body.to_string());
}

pub fn is_complete(run_dir: &Path, inputs: &str) -> bool {
    why_not(run_dir, inputs) == "reusable"
}

pub fn why_not(run_dir: &Path, inputs: &str) -> String {
    let path = run_dir.join(MARKER);
    let Ok(text) = fs::read_to_string(&path) else {
        return "missing marker".into();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return "corrupt marker".into();
    };
    let got = v.get("inputs").and_then(|x| x.as_str()).unwrap_or("");
    if got == inputs {
        "reusable".into()
    } else {
        format!("task inputs {got} != {inputs}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_stable_and_sensitive() {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().join("documents");
        fs::create_dir_all(&files).unwrap();
        fs::write(files.join("source.txt"), "version-one").unwrap();
        let first = inputs_fingerprint("Use version one.", Some(&files));
        assert_eq!(first, inputs_fingerprint("Use version one.", Some(&files)));
        assert_ne!(first, inputs_fingerprint("Use version two.", Some(&files)));
        fs::write(files.join("source.txt"), "version-two").unwrap();
        assert_ne!(first, inputs_fingerprint("Use version one.", Some(&files)));
    }

    #[test]
    fn marker_refuses_changed_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path().join("run");
        mark_complete(&run, "r1", "aaaa");
        assert!(is_complete(&run, "aaaa"));
        assert!(!is_complete(&run, "bbbb"));
        assert_eq!(why_not(&run, "aaaa"), "reusable");
        assert!(why_not(&run, "bbbb").contains("aaaa != bbbb"));
    }
}
