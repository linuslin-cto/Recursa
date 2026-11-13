# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + full test suite; rebranded `RECURSA_*` / `.recursa_complete`
- `docs/EVALUATION.md` + `self_evolution` (task + harnessing + score Δ)
- `report` module + `recursa score` (base vs refined, judged.json)
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. Wire catalog datasets into a `recursa run` schedule path
2. Parallel propose via Sync proposers / scoped sessions
3. Model-backed bench judge/report against jb/lab/wb graders
4. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1
```
