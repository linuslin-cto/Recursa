# Evaluation: verifying self-evolution

Recursa’s eval scheme follows the recursive-harnessing experiment design:
refine → judge (optional) → report, with a **base vs refined** comparison and a
**harnessing-level library** that carries lessons across tasks in a domain.

## What “self-evolution” means here

| Level | Mechanism | Offline check |
|---|---|---|
| Task-level | Propose–Probe–Compose on one task | Refined deliverable beats base on the same inputs |
| Harnessing-level | Domain library `R` after adoption + reflection | Later task contexts include earlier harnesses / experience |

Model weights stay fixed; only the editable harness tree evolves.

## Online protocol (benchmarks)

Mirrored from the upstream experiment guide, using Recursa env names:

```powershell
$env:RECURSA_RUNS_DIR = "runs/lab-seq-k1"
cargo run -p recursa-cli -- catalog --bench lab --output datasets
# Model-backed refine / judge / report (when wired):
# recursa --bench lab --k 1 --mode sequential --only-domains antitrust-competition --max-tasks 1
# recursa-judge --bench lab --k 1 --mode sequential   # include --bases for base_judged.json
# recursa-report --bench lab --k 1 --mode sequential
```

Keep **benchmark, substrate, k, mode, seed** identical across refine / judge / report.
Use a fresh `RECURSA_RUNS_DIR` when changing models or task selection.
Domain-stratified splits: `select_split` + `stream_order` (seeded shuffle).

Baseline: score reference (`base*`) rollouts separately from the adopted refined
harness so gains are attributable to harness edits, not task noise.

## Offline protocol (this repo)

No API keys. The deterministic demo uses a **positive-only** ledger sum as the
base harness; the scripted improver removes the filter so **signed** totals match.

```powershell
cargo test -p recursa-core --test self_evolution
cargo run -p recursa-cli -- demo --output runs/demo --k 1
```

`self_evolution` asserts:

1. Base execution on `ledger-1` emits the positive-only total (wrong for signed).
2. After recursion + selection, the adopted refined harness emits the signed total.
3. Domain library contains `ledger-1` before / for `ledger-2` (cross-task transfer).
4. `ledger-2` also lands on the signed total (stream continues to improve).

Budget identity remains `R = 4k` rollouts and `P = 3k` improvement stages per mode.
