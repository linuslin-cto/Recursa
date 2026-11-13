# Recursa progress

Project root / name: **Recursa**.

## Done

- Core recursion + evaluation + catalog run/judge + parallel propose
- Weighted rubrics + LAB criteria stripping + external judge env hook
- OpenCode shim fork-hardening; CI template + `docs/ACTIONS.md`
- Catalog score recovers missing `rubrics.json` from dataset (stale-run safe)
- Published: https://github.com/linuslin-cto/Recursa

## Next

1. `gh auth refresh -s workflow` then install `.github/workflows/ci.yml` from example
2. Point `RECURSA_EXTERNAL_JUDGE` at real jb/lab judge CLIs when checkouts exist
3. Keep tests green every tick

## Last verify

`cargo test --workspace` green — 2026-11-14.

Self-evolution scores (offline stub, base → refined Δ):

| Suite | base | refined | Δ |
|-------|------|---------|---|
| demo ledger | 0% | 100% | **+100 pp** |
| lab catalog (n=2) | 50% | 100% | **+50 pp** |

Paper orientation (model-backed): WB ≤+11.5 pp, JB ≤+4.5 pp, LAB partial ≤+11.8 pp.

GH token scopes still `gist,read:org,repo` (no `workflow`); CI install remains blocked offline.

Remaining `harness2` strings: BibTeX key / paper title in README citation; `.gitignore` entry for local upstream clone. Not product branding.

## Verify

```powershell
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
cargo run -p recursa-cli -- judge --bench lab --dataset datasets --output runs/lab --k 1
```
