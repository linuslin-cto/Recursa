# Recursa

Rust rewrite of [Harness²](https://github.com/google-research/harness2): recursive agent
harnessing with fixed model weights. Project name matches this directory.

Upstream Python reference: [google-research/harness2](https://github.com/google-research/harness2).

## Crates

| Crate | Role |
|---|---|
| `recursa-core` | Tree surface, ledger, recursion, schedule, selection, library, judge |
| `recursa-demo` | Deterministic ledger adapter + scripted proposer |
| `recursa-cli` | `recursa demo` / `info` / `catalog` |

## Quick start

```powershell
cargo test --workspace
cargo run -p recursa-cli -- demo --output runs/demo --k 2
cargo run -p recursa-cli -- info --k 2
cargo run -p recursa-cli -- catalog --bench lab --output datasets
```

No model credentials are required for the demo.

## Parity

- Budget identity: `R=4k` rollouts, `P=3k` improvement stages per mode
- Demo executor interprets `scripts/summarize.py` in-process
- Workflow tests mirrored from `harness2/tests/test_workflow.py`

## Model channel

See [docs/TRANSPORT.md](docs/TRANSPORT.md) for `FixtureTransport` / `CommandTransport`
and OpenCode JSONL parsing. The local demo needs no credentials.

## License

Apache-2.0
