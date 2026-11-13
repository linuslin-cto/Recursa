# Recursa progress

Project root / name: **Recursa**. Upstream reference: `harness2/`.

## Done

- Core recursion + full automated test suite
- Substrates, transports, benchmarks, inputs fingerprint, graders, stream_order
- `judge_selections_with_grader` wired to `Grader`
- `scripts/opencode-session.ps1` dry-run / live / `OPENCODE_BIN` shim
- Dataset loaders: `discover` (jb/lab/wb), `select_split`, native fixture materialise + INDEX
- `recursa catalog --bench jb|lab|wb` → `datasets/<bench>/`
- `cargo test --workspace` green

## Next

1. Wire discovered tasks into schedule / adapter rollout paths
2. Parallel propose via Sync proposers / scoped sessions
3. Real HF/checkout fetch helpers (optional; discover already reads native trees)
4. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- info --k 1
cargo run -p recursa-cli -- catalog --bench lab --output datasets
```
