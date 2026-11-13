# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + evaluation + catalog run/judge + parallel propose
- Rubric graders + JobBench-style `RUBRICS.json` loader
- OpenCode shim fork-hardening
- JobBench-style `RUBRICS.json` loader; CI template at `docs/ci.yml.example`
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. Enable GitHub Actions (needs `workflow` OAuth scope) from `docs/ci.yml.example`
2. Wire weighted JobBench judge subprocess when dataset checkout is present
3. LAB criteria-aware partial credit from real task.json
4. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```
