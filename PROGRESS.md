# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + full test suite; rebranded `RECURSA_*` / `.recursa_complete`
- `docs/EVALUATION.md` + `self_evolution` + `recursa score` (Δ +100 pp)
- `CatalogAdapter` + `recursa run` / `recursa judge` (catalog → schedule → report)
- Parallel propose via `Proposer::fork` + rayon; ChannelProposer/transport fork
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. Real jb/lab/wb grader rubrics (beyond dry `OK:` / signed-total stubs)
2. Live OpenCode session hardening under forked CommandTransport
3. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
cargo run -p recursa-cli -- run --bench lab --dataset datasets --output runs/lab --k 1 --max-tasks 1
cargo run -p recursa-cli -- judge --bench lab --dataset datasets --output runs/lab --k 1
```
