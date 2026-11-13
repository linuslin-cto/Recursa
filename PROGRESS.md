# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + evaluation + catalog run/judge + parallel propose
- Weighted rubrics + LAB criteria stripping + external judge env hook
- OpenCode shim fork-hardening; CI template + `docs/ACTIONS.md`
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. `gh auth refresh -s workflow` then install `.github/workflows/ci.yml` from example
2. Point `RECURSA_EXTERNAL_JUDGE` at real jb/lab judge CLIs when checkouts exist
3. Keep tests green every tick

## Last verify

`cargo test --workspace` + `recursa score --skip-run` green (Δ +100 pp). Remaining Next items need local GH workflow scope / dataset checkouts.

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```
