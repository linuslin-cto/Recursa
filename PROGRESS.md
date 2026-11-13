# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + full test suite; rebranded `RECURSA_*` / `.recursa_complete`
- Evaluation + self_evolution + `recursa score` / `run` / `judge`
- Parallel propose (`Proposer::fork`) + ChannelProposer/transport fork
- Rubric graders (`rubrics.json` / `RubricFileGrader`) for catalog stubs
- OpenCode shim: per-label locks, timeout, `RECURSA_FORCE_FIXTURE`, model env fallback
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. Map real jb/lab/wb upstream rubric schemas into RubricFileGrader
2. Optional CI job for `cargo test` + `recursa score --skip-run`
3. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
cargo run -p recursa-cli -- catalog --bench lab --output datasets
cargo run -p recursa-cli -- run --bench lab --dataset datasets --output runs/lab --k 1 --max-tasks 1
cargo run -p recursa-cli -- judge --bench lab --dataset datasets --output runs/lab --k 1
```
