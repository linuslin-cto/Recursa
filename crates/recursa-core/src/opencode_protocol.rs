//! OpenCode JSONL event-stream stand-ins (no bun / model contact).

use crate::proposer::ProposerError;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct SessionParse {
    pub text: String,
    pub cost_usd: f64,
    pub ok: bool,
    pub error: String,
}

/// Parse a newline-delimited OpenCode event stream.
pub fn parse_events(stdout: &str) -> SessionParse {
    let mut text = String::new();
    let mut cost_usd = 0.0;
    let mut error = String::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match event.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "text" => {
                if let Some(t) = event
                    .pointer("/part/text")
                    .and_then(|v| v.as_str())
                    .or_else(|| event.get("text").and_then(|v| v.as_str()))
                {
                    text.push_str(t);
                }
            }
            "step_finish" => {
                if let Some(c) = event
                    .pointer("/part/cost")
                    .and_then(|v| v.as_f64())
                    .or_else(|| event.pointer("/part/cost").and_then(|v| v.as_i64()).map(|i| i as f64))
                {
                    cost_usd += c;
                }
            }
            "error" => {
                error = event
                    .pointer("/error/data/message")
                    .and_then(|v| v.as_str())
                    .or_else(|| event.pointer("/error/message").and_then(|v| v.as_str()))
                    .unwrap_or("session error")
                    .to_string();
            }
            _ => {}
        }
    }
    SessionParse {
        text,
        cost_usd,
        ok: error.is_empty(),
        error,
    }
}

/// Prefer an on-disk answer file when present; otherwise use parsed stream text.
pub fn resolve_answer(parse: &SessionParse, answer_file: Option<&str>) -> Result<String, ProposerError> {
    if !parse.ok {
        return Err(ProposerError::new(
            parse.error.clone(),
            1,
            parse.cost_usd,
        ));
    }
    if let Some(body) = answer_file {
        if !body.trim().is_empty() {
            return Ok(body.to_string());
        }
    }
    if parse.text.trim().is_empty() {
        return Err(ProposerError::new("empty session answer", 1, parse.cost_usd));
    }
    Ok(parse.text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_error_rejects_partial_and_preserves_cost() {
        let stdout = r#"{"type":"text","part":{"type":"text","text":"partial or final answer"}}
{"type":"step_finish","part":{"type":"step-finish","cost":0.25}}
{"type":"error","error":{"name":"APIError","data":{"message":"provider unavailable"}}}
"#;
        let parse = parse_events(stdout);
        let err = resolve_answer(&parse, Some("partial or final answer")).unwrap_err();
        assert!(err.message.contains("provider unavailable"));
        assert!((err.cost_usd - 0.25).abs() < 1e-9);
    }

    #[test]
    fn successful_file_and_stdout_answers() {
        let stdout = r#"{"type":"text","part":{"type":"text","text":"partial or final answer"}}
{"type":"step_finish","part":{"type":"step-finish","cost":0.25}}
"#;
        let parse = parse_events(stdout);
        let from_file = resolve_answer(&parse, Some("partial or final answer")).unwrap();
        assert_eq!(from_file, "partial or final answer");
        let from_stdout = resolve_answer(&parse, None).unwrap();
        assert_eq!(from_stdout, "partial or final answer");
        assert!((parse.cost_usd - 0.25).abs() < 1e-9);
    }
}
