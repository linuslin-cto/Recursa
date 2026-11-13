# Channel transport

`ChannelProposer` talks to improver/selector models through a `SessionTransport`.
Stdout must be OpenCode-style JSONL; `opencode_protocol` parses text, cost, and errors.

## FixtureTransport

In-process canned JSONL for unit tests and dry runs. No network.

```rust
use recursa_core::proposer::{ChannelProposer, FixtureTransport};
use recursa_core::spec::ChannelSpec;

let spec = ChannelSpec::new("opencode", "local/fixture")?;
let mut prop = ChannelProposer::with_transport(
    spec,
    Box::new(FixtureTransport {
        stdout: include_str!("events.jsonl").into(),
        answer_file: None,
        fail: false,
    }),
)?;
```

## CommandTransport

Spawns an external program (bun + opencode CLI, or a shim). Expected flags:

```
<program> [args...] --model <id> --label <label> --mission <text> [--context <dir>]
```

- Prints JSONL events on stdout (`text`, `step_finish`, `error`, …).
- Optional answer file: `<context>/output/answer.md`.
- Env (convention):
  - `RECURSA_IMPROVER_MODEL` / `RECURSA_SOLVER_MODEL`
  - `RECURSA_RUNS_DIR` for experiment output roots
  - `BUN`, `OPENCODE_DIR` / `OPENCODE_BIN` when using a live OpenCode package

Example:

```powershell
$env:RECURSA_IMPROVER_MODEL = "provider/model-id"
# Shim (fixture without OPENCODE_DIR; live when OPENCODE_DIR + bun are set):
# CommandTransport { program: pwsh, args: ["-File", "scripts/opencode-session.ps1"] }
```

`scripts/opencode-session.ps1` emits dry-run JSONL when `OPENCODE_DIR` is unset.
Live spawn is optional; the demo path uses `DemoProposer` and needs no credentials.
