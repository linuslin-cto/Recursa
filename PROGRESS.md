# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + full test suite; rebranded `RECURSA_*` / `.recursa_complete`
- `docs/EVALUATION.md` + `self_evolution` + `recursa score` (Δ +100 pp)
- `CatalogAdapter` + `recursa run` (catalog → stream_order → schedule)
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. Parallel propose via Sync proposers / scoped sessions
2. Model-backed bench judge/report against jb/lab/wb graders
3. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1
cargo run -p recursa-cli -- run --bench lab --dataset datasets --output runs/lab --k 1 --max-tasks 1
```
