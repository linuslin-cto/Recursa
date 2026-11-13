# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + evaluation + catalog run/judge + parallel propose
- Weighted rubrics + JobBench `RUBRICS.json` + LAB `task.json` criteria (stripped from improver)
- OpenCode shim fork-hardening; CI template at `docs/ci.yml.example`
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. Enable GitHub Actions (needs `workflow` OAuth scope) from `docs/ci.yml.example`
2. Optional external judge subprocess wrapper (`RECURSA_EXTERNAL_JUDGE`)
3. Keep tests green every tick

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```
