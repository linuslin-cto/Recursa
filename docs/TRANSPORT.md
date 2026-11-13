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
- Env (convention, matching upstream):
  - `HARNESS2_IMPROVER_MODEL` / `HARNESS2_SOLVER_MODEL`
  - `BUN`, `OPENCODE_DIR` when using the real OpenCode package

Example:

```powershell
$env:HARNESS2_IMPROVER_MODEL = "provider/model-id"
# Shim (fixture without OPENCODE_DIR; live when OPENCODE_DIR + bun are set):
# CommandTransport { program: pwsh, args: ["-File", "scripts/opencode-session.ps1"] }
```

`scripts/opencode-session.ps1` emits dry-run JSONL when `OPENCODE_DIR` is unset.
Live spawn is optional; the demo path uses `DemoProposer` and needs no credentials.
