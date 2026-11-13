//! Improver / selector channel abstraction.

use crate::opencode_protocol;
use crate::spec::ChannelSpec;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub label: String,
    pub text: String,
    pub ok: bool,
    pub cost_usd: f64,
    pub attempts: usize,
    pub cached: bool,
    pub workspace: Option<PathBuf>,
}

impl Reply {
    pub fn ok_text(label: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            text: text.into(),
            ok: true,
            cost_usd: 0.0,
            attempts: 1,
            cached: false,
            workspace: None,
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ProposerError {
    pub message: String,
    pub attempts: usize,
    pub cost_usd: f64,
}

impl ProposerError {
    pub fn new(message: impl Into<String>, attempts: usize, cost_usd: f64) -> Self {
        Self {
            message: message.into(),
            attempts,
            cost_usd,
        }
    }
}

pub trait Proposer: Send + Sync {
    fn call(
        &mut self,
        mission: &str,
        context_dir: Option<&Path>,
        label: &str,
    ) -> Result<Reply, ProposerError>;
}

/// How a channel-backed proposer obtains a session transcript.
pub trait SessionTransport: Send + Sync {
    fn run_session(
        &mut self,
        mission: &str,
        context_dir: Option<&Path>,
        label: &str,
        model: &str,
    ) -> Result<TransportResult, ProposerError>;
}

#[derive(Debug, Clone)]
pub struct TransportResult {
    pub stdout: String,
    pub answer_file: Option<String>,
    pub workspace: Option<PathBuf>,
    pub attempts: usize,
}

/// Fixture transport: feed canned JSONL (tests / dry-run).
pub struct FixtureTransport {
    pub stdout: String,
    pub answer_file: Option<String>,
    pub fail: bool,
}

impl SessionTransport for FixtureTransport {
    fn run_session(
        &mut self,
        _mission: &str,
        context_dir: Option<&Path>,
        _label: &str,
        _model: &str,
    ) -> Result<TransportResult, ProposerError> {
        if self.fail {
            // Still return stdout so the protocol parser can recover cost from step_finish.
            return Ok(TransportResult {
                stdout: self.stdout.clone(),
                answer_file: self.answer_file.clone(),
                workspace: context_dir.map(|p| p.join("harness")),
                attempts: 1,
            });
        }
        Ok(TransportResult {
            stdout: self.stdout.clone(),
            answer_file: self.answer_file.clone(),
            workspace: context_dir.map(|p| p.join("harness")),
            attempts: 1,
        })
    }
}

/// Spawn an external command that prints OpenCode JSONL on stdout.
pub struct CommandTransport {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl SessionTransport for CommandTransport {
    fn run_session(
        &mut self,
        mission: &str,
        context_dir: Option<&Path>,
        label: &str,
        model: &str,
    ) -> Result<TransportResult, ProposerError> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .arg("--model")
            .arg(model)
            .arg("--label")
            .arg(label)
            .arg("--mission")
            .arg(mission);
        if let Some(dir) = context_dir {
            cmd.arg("--context").arg(dir);
        }
        let output = cmd.output().map_err(|e| {
            ProposerError::new(format!("failed to spawn {}: {e}", self.program.display()), 1, 0.0)
        })?;
        if !output.status.success() && output.stdout.is_empty() {
            return Err(ProposerError::new(
                format!(
                    "transport exited {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ),
                1,
                0.0,
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let answer_file = context_dir.and_then(|d| {
            let p = d.join("output/answer.md");
            fs_read_optional(&p)
        });
        Ok(TransportResult {
            stdout,
            answer_file,
            workspace: context_dir.map(|p| p.join("harness")),
            attempts: 1,
        })
    }
}

fn fs_read_optional(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().filter(|s| !s.trim().is_empty())
}

/// Channel-backed proposer: transport → OpenCode JSONL parser → Reply.
pub struct ChannelProposer {
    pub spec: ChannelSpec,
    transport: Box<dyn SessionTransport>,
    pub calls: Vec<Reply>,
}

impl ChannelProposer {
    pub fn new(spec: ChannelSpec) -> Result<Self, ProposerError> {
        if spec.model.trim().is_empty() {
            return Err(ProposerError::new(
                "proposer model is empty; set HARNESS2_IMPROVER_MODEL",
                0,
                0.0,
            ));
        }
        Err(ProposerError::new(
            "channel proposer needs an explicit transport; use with_transport()",
            0,
            0.0,
        ))
    }

    pub fn with_transport(spec: ChannelSpec, transport: Box<dyn SessionTransport>) -> Result<Self, ProposerError> {
        if spec.model.trim().is_empty() {
            return Err(ProposerError::new(
                "proposer model is empty; set HARNESS2_IMPROVER_MODEL",
                0,
                0.0,
            ));
        }
        Ok(Self {
            spec,
            transport,
            calls: Vec::new(),
        })
    }
}

impl Proposer for ChannelProposer {
    fn call(
        &mut self,
        mission: &str,
        context_dir: Option<&Path>,
        label: &str,
    ) -> Result<Reply, ProposerError> {
        let tr = self
            .transport
            .run_session(mission, context_dir, label, &self.spec.model)?;
        let parse = opencode_protocol::parse_events(&tr.stdout);
        match opencode_protocol::resolve_answer(&parse, tr.answer_file.as_deref()) {
            Ok(text) => {
                let reply = Reply {
                    label: label.into(),
                    text,
                    ok: true,
                    cost_usd: parse.cost_usd,
                    attempts: tr.attempts.max(1),
                    cached: false,
                    workspace: tr.workspace,
                };
                self.calls.push(reply.clone());
                Ok(reply)
            }
            Err(mut err) => {
                err.attempts = tr.attempts.max(1);
                if err.cost_usd == 0.0 {
                    err.cost_usd = parse.cost_usd;
                }
                let failed = Reply {
                    label: label.into(),
                    text: String::new(),
                    ok: false,
                    cost_usd: err.cost_usd,
                    attempts: err.attempts,
                    cached: false,
                    workspace: None,
                };
                self.calls.push(failed);
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::ChannelSpec;

    #[test]
    fn fixture_transport_preserves_cost_on_error() {
        let stdout = r#"{"type":"text","part":{"text":"partial"}}
{"type":"step_finish","part":{"cost":0.25}}
{"type":"error","error":{"data":{"message":"provider unavailable"}}}
"#;
        let spec = ChannelSpec::new("opencode", "local/fixture").unwrap();
        let mut prop = ChannelProposer::with_transport(
            spec,
            Box::new(FixtureTransport {
                stdout: stdout.into(),
                answer_file: Some("partial".into()),
                fail: false,
            }),
        )
        .unwrap();
        let err = prop.call("mission", None, "fixture").unwrap_err();
        assert!(err.message.contains("provider unavailable"));
        assert!((err.cost_usd - 0.25).abs() < 1e-9);
        assert!(!prop.calls.last().unwrap().ok);
    }

    #[test]
    fn fixture_transport_success() {
        let stdout = r#"{"type":"text","part":{"text":"final answer"}}
{"type":"step_finish","part":{"cost":0.1}}
"#;
        let spec = ChannelSpec::new("opencode", "local/fixture").unwrap();
        let mut prop = ChannelProposer::with_transport(
            spec,
            Box::new(FixtureTransport {
                stdout: stdout.into(),
                answer_file: None,
                fail: false,
            }),
        )
        .unwrap();
        let reply = prop.call("mission", None, "ok").unwrap();
        assert_eq!(reply.text, "final answer");
        assert!((reply.cost_usd - 0.1).abs() < 1e-9);
    }
}
