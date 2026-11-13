# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + full automated test suite (demo / workflow / resume / graders)
- Substrates, transports, datasets discover+catalog, stream_order
- Rebranded runtime markers / env (`RECURSA_*`, `.recursa_complete`)
- `docs/EVALUATION.md` + `self_evolution` offline check (task + harnessing levels)
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. Wire catalog datasets into a `recursa run` schedule path
2. Parallel propose via Sync proposers / scoped sessions
3. Model-backed judge/report CLI parity with the evaluation guide
4. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo test -p recursa-core --test self_evolution
cargo run -p recursa-cli -- info --k 1
```
