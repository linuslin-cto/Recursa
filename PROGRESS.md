# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + evaluation + catalog run/judge + parallel propose
- Rubric graders + JobBench-style `RUBRICS.json` loader
- OpenCode shim fork-hardening
- GitHub Actions CI (`cargo test` + `recursa score`)
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. Wire weighted JobBench judge subprocess when dataset checkout is present
2. LAB criteria-aware partial credit from real task.json (still stripped from improver view)
3. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```
