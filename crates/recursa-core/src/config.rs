//! Shared runtime knobs aligned with the Python package.

pub const PHASE_WIDTH: usize = 8;
pub const TRANSPORT_ATTEMPTS: usize = 3;
pub const DEFAULT_PROPOSER_CHANNEL: &str = "opencode";

pub fn proposer_model_by_channel(kind: &str) -> Option<&'static str> {
    match kind {
        "opencode" => Some(""),
        "claude_cli" => Some("claude-opus-4-6"),
        _ => None,
    }
}
