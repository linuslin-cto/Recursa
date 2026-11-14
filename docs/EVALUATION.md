# Evaluation: verifying and scoring self-evolution

Recursa follows the recursive-harnessing experiment loop:

**refine → judge → report**, with a **base vs refined** comparison and a
**harnessing-level library** across tasks in a domain.

## What “self-evolution” means

| Level | Mechanism | Offline check |
|---|---|---|
| Task-level | Propose–Probe–Compose on one task | Refined deliverable beats base on the same inputs |
| Harnessing-level | Domain library `R` after adoption | Later task contexts include earlier harnesses |
| Score | Judge chosen runs; summarise by (mode, k) | Refined score − base score = Δ pp > 0 |

Model weights stay fixed; only the editable harness tree evolves.

## Scoring rules (aligned with upstream report)

- Grade = `n_passed / n_criteria` (partial credit). Ungraded rows are **excluded**
  from the denominator (never treated as zero).
- LAB-style reports may show both **partial %** and **all-criteria pass %**.
- Pick provenance: final / candidate / base.
- At **k=1**, parallel and sequential are the same procedure; any gap is a
  **noise floor**, not a mode result.
- Baseline: score `base*` reference executions separately (`--bases` role) so
  gains come from harness edits.

## Offline protocol (this repo)

```powershell
cargo test -p recursa-core --test self_evolution
cargo run -p recursa-cli -- score --output runs/demo --k 1
```

`recursa score` re-runs the demo, grades signed totals, writes
`runs/.../judged.json`, and prints:

- refined summary (score %)
- **Base vs refined** Δ in percentage points
- paper reference gains (orientation only)

Expected offline result on the ledger demo:

| Cell | Base | Refined | Δ |
|---|---|---|---|
| parallel k=1 | 0% | 100% | **+100 pp** |
| sequential k=1 | 0% | 100% | **+100 pp** |

(`self_evolution` asserts the same numbers.)

Catalog stubs use **weighted** rubrics (e.g. OK: weight 2, task-id weight 1).
JobBench fixtures include two accounting tasks so harnessing-level library
carry-over (`accounting/task-1` → `task-2`) is covered by
`catalog_self_evolution`. Optional: set `RECURSA_EXTERNAL_JUDGE` to a program
that writes `scores.json` into the run directory (see `ExternalJudgeGrader`).

```powershell
cargo test -p recursa-core --test catalog_self_evolution
cargo run -p recursa-cli -- run --bench jb --dataset datasets --output runs/jb --k 1
cargo run -p recursa-cli -- judge --bench jb --dataset datasets --output runs/jb --k 1
```

## Paper reference gains (model-backed benches)

Published orientation figures (not reproduced by the offline demo):

| Benchmark | Max reported gain |
|---|---|
| WorkBuddy-Bench | +11.5 pp |
| JobBench | +4.5 pp |
| LAB partial pass | +11.8 pp |

## Online protocol (when model channels are wired)

```powershell
$env:RECURSA_RUNS_DIR = "runs/lab-seq-k1"
cargo run -p recursa-cli -- catalog --bench lab --output datasets
# refine / judge / report with identical bench, substrate, k, mode, seed
```

Keep identity fixed across refine / judge / report. Use a fresh
`RECURSA_RUNS_DIR` when changing models or task selection.
Budget identity: `R = 4k` rollouts, `P = 3k` improvement stages per mode.
