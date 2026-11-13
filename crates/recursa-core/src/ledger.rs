//! Budget accounting: R = 4k rollouts and P = 3k improvement stages.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub fn expected_rollouts(k: usize) -> usize {
    4 * k
}

pub fn expected_sessions(k: usize) -> usize {
    3 * k
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct BudgetError(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub kind: String,
    pub label: String,
    pub usd: f64,
    pub cached: bool,
    pub attempts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub mode: String,
    pub task_id: String,
    pub k: usize,
    pub entries: Vec<Entry>,
}

impl Ledger {
    pub fn new(mode: impl Into<String>, task_id: impl Into<String>, k: usize) -> Self {
        Self {
            mode: mode.into(),
            task_id: task_id.into(),
            k,
            entries: Vec::new(),
        }
    }

    pub fn rollout(&mut self, label: impl Into<String>, usd: f64, cached: bool) {
        self.entries.push(Entry {
            kind: "rollout".into(),
            label: label.into(),
            usd,
            cached,
            attempts: 1,
        });
    }

    pub fn session(&mut self, label: impl Into<String>, usd: f64, cached: bool, attempts: usize) {
        self.entries.push(Entry {
            kind: "session".into(),
            label: label.into(),
            usd,
            cached,
            attempts,
        });
    }

    pub fn r_count(&self) -> usize {
        self.entries.iter().filter(|e| e.kind == "rollout").count()
    }

    pub fn p_count(&self) -> usize {
        self.entries.iter().filter(|e| e.kind == "session").count()
    }

    pub fn labels(&self, kind: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| e.label.clone())
            .collect()
    }

    pub fn assert_identity(&self) -> Result<(), BudgetError> {
        let want_r = expected_rollouts(self.k);
        let want_p = expected_sessions(self.k);
        let rl = self.labels("rollout");
        let sl = self.labels("session");
        let mut dupes: Vec<String> = rl
            .iter()
            .filter(|label| rl.iter().filter(|l| l == label).count() > 1)
            .cloned()
            .collect();
        for label in &sl {
            if sl.iter().filter(|l| *l == label).count() > 1 {
                dupes.push(label.clone());
            }
        }
        dupes.sort();
        dupes.dedup();
        if dupes.is_empty() && self.r_count() == want_r && self.p_count() == want_p {
            return Ok(());
        }
        Err(BudgetError(format!(
            "{}-{} {}: R={} (want {}), P={} (want {})\n  rollouts: {}\n  sessions: {}{}",
            self.mode,
            self.k,
            self.task_id,
            self.r_count(),
            want_r,
            self.p_count(),
            want_p,
            if rl.is_empty() {
                "(none)".into()
            } else {
                rl.join(", ")
            },
            if sl.is_empty() {
                "(none)".into()
            } else {
                sl.join(", ")
            },
            if dupes.is_empty() {
                String::new()
            } else {
                format!("\n  DUPLICATE labels: {dupes:?}")
            }
        )))
    }

    pub fn session_attempts(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == "session")
            .map(|e| e.attempts)
            .sum()
    }

    pub fn session_usd(&self) -> f64 {
        let sum: f64 = self
            .entries
            .iter()
            .filter(|e| e.kind == "session")
            .map(|e| e.usd)
            .sum();
        (sum * 1_000_000.0).round() / 1_000_000.0
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "mode": self.mode,
            "task_id": self.task_id,
            "k": self.k,
            "R": self.r_count(),
            "P": self.p_count(),
            "session_attempts": self.session_attempts(),
            "session_usd": self.session_usd(),
            "entries": self.entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_holds_for_k2() {
        let mut ledger = Ledger::new("agg", "t", 2);
        for i in 0..8 {
            ledger.rollout(format!("r{i}"), 0.0, false);
        }
        for i in 0..6 {
            ledger.session(format!("s{i}"), 0.0, false, 1);
        }
        ledger.assert_identity().unwrap();
    }
}
