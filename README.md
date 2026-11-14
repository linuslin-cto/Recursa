# Recursa

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Crates](https://img.shields.io/badge/workspace-3%20crates-informational.svg)](#crates)
[![Status](https://img.shields.io/badge/status-0.1.0%20preview-yellow.svg)](https://github.com/linuslin-cto/Recursa)

**English** | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

> Recursive agent harnessing with **fixed model weights** — improve the system around the model, not the parameters inside it.

A **harness** is everything that turns a model into an agent: prompts, tools, memory, skills, scripts, orchestration. **Harnessing** is optimizing that system. Recursa is a Rust workspace for **recursive agent harnessing** in an open world, where agents meet tasks that were not anticipated when the harness was designed.

It connects two levels of recursion while keeping weights fixed:

| Level | What happens |
|---|---|
| **Task-level** | **Propose → Probe → Compose**: propose harness edits, probe them on the *same* task, compose a refined harness from behavioral contrasts — no ground-truth feedback required during refinement. |
| **Harnessing-level** | Persist adopted harnesses and evidence-backed lessons in a per-domain experience library so later tasks improve from earlier ones. |

Offline, a single refinement step moves the deterministic demo from **0% → 100%** (+100 pp). On catalog stubs (LAB), refined runs show clear base→refined gains. Model-backed paper orientation: up to **+11.5 pp** (WorkBuddy), **+4.5 pp** (JobBench), **+11.8 pp** (LAB partial).

The release supports **parallel and sequential** recursion, a **deterministic ledger demo**, and **JobBench / LAB / WorkBuddy** fixture catalogs — no API keys required for the local path.

## Contents

- [Why Recursa](#why-recursa)
- [Harness components](#harness-components)
- [Crates](#crates)
- [Requirements](#requirements)
- [Install](#install)
- [Quick start](#quick-start)
- [Run a benchmark stub](#run-a-benchmark-stub)
- [Experiments](#experiments)
- [Evaluation](#evaluation)
- [Model channel](#model-channel)
- [Project layout](#project-layout)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [License](#license)
- [Citation](#citation)
- [Acknowledgements](#acknowledgements)

## Why Recursa

Most agent stacks freeze the harness and only swap models. Recursa does the opposite: **weights stay fixed; the editable tree evolves**.

- **Contrastive refinement** — learn from how candidate harnesses *behave* on the same task, not from labeled answers during Propose–Probe–Compose.
- **Budget identity** — rollouts and improvement stages follow `R = 4k`, `P = 3k` so experiments stay comparable.
- **Offline-first** — ship and verify self-evolution without credentials; wire live executors when ready.
- **Rust core** — typed tree surface, ledger, schedule, selection, library, judge, and report in one workspace.

## Harness components

Recursa exposes eight editable components around the model (**c1…c8**):

| # | Component | Role |
|---|---|---|
| c1 | System prompt | Always-on identity and goals |
| c2 | Guardrails | Safety, triggers, loading policy |
| c3 | Memory | Long-term notes the agent may carry |
| c4 | Tool guidance | How tools are described and used |
| c5 | Subagents | Delegation modes and roles |
| c6 | Skills | On-demand playbooks with triggers |
| c7 | Scripts | Deterministic helpers |
| c8 | Plugins | Executor-specific extensions |

They control context construction, the tool interface, delegation, and behavior inside the tool loop. Diffs and validation live in `recursa-core::tree`.

## Crates

| Crate | Role |
|---|---|
| [`recursa-core`](crates/recursa-core) | Tree, ledger, recursion, schedule, selection, library, judge, report, catalog |
| [`recursa-demo`](crates/recursa-demo) | Deterministic ledger adapter + scripted proposer |
| [`recursa-cli`](crates/recursa-cli) | `recursa` binary: `demo` / `info` / `catalog` / `run` / `judge` / `score` |

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (edition 2021; 1.70+ recommended)
- Cargo (comes with rustup)
- Optional later: Node.js / OpenCode or Codex for live substrates — see [docs/TRANSPORT.md](docs/TRANSPORT.md)

## Install

Clone and build from the repository root:

```bash
git clone https://github.com/linuslin-cto/Recursa.git
cd Recursa
cargo build -p recursa-cli --release
```

On Windows (PowerShell):

```powershell
git clone https://github.com/linuslin-cto/Recursa.git
cd Recursa
cargo build -p recursa-cli --release
```

The local demo needs **no model credentials**.

## Quick start

```bash
# Verify the workspace
cargo test --workspace

# Deterministic demo (Propose–Probe–Compose, k=2)
cargo run -p recursa-cli -- demo --output runs/demo --k 2

# Framework identity + budget identities
cargo run -p recursa-cli -- info --k 2

# Score base vs refined (self-evolution gain)
cargo run -p recursa-cli -- score --output runs/demo --k 1
```

Expected offline score on the ledger demo: base **0%**, refined **100%**, Δ **+100 pp**.

## Run a benchmark stub

Materialize a fixture catalog, refine, then judge. Start with a small task count:

```bash
cargo run -p recursa-cli -- catalog --bench lab --output datasets
cargo run -p recursa-cli -- run --bench lab --dataset datasets --output runs/lab --k 1 --max-tasks 2
cargo run -p recursa-cli -- judge --bench lab --dataset datasets --output runs/lab --k 1
```

| Benchmark | Token | Install | Notes |
|---|---|---|---|
| JobBench | `jb` | `catalog --bench jb` | Professional-work style rubrics |
| LAB | `lab` | `catalog --bench lab` | Legal-work style criteria |
| WorkBuddy | `wb` | `catalog --bench wb` | Office / web / code oriented stubs |

Choose **one** `--bench` per command. Judging must use the same bench, dataset, `--k`, and output as the refine run. Optional: set `RECURSA_EXTERNAL_JUDGE` to a program that writes `scores.json` into the run directory.

## Experiments

Scale task-level recursion in two ways:

| Mode | Behavior |
|---|---|
| **Parallel** (`agg`) | Explore multiple candidate harnesses; compose refinements from their execution evidence |
| **Sequential** (`seq`) | Carry the refined harness and accumulated evidence through successive Propose–Probe–Compose steps |

| Option | Meaning |
|---|---|
| `--k 1` | One Propose–Probe–Compose step |
| `--k K` | Budget scales with `K` (`R = 4k` rollouts, `P = 3k` stages per mode) |
| `--max-tasks N` | Cap catalog tasks (`0` = all) |
| `--seed` | Reproducible task selection when supported |

After each task, harnessing-level recursion retains the adopted harness and evidence-supported lessons for later tasks in the same domain.

## Evaluation

Refine → judge → report, with an explicit **base vs refined** comparison.

```bash
cargo test -p recursa-core --test self_evolution
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```

| Suite (offline) | Base | Refined | Δ |
|---|---|---|---|
| Demo ledger | 0% | 100% | **+100 pp** |
| LAB catalog (n=2) | 50% | 100% | **+50 pp** |

Scoring uses partial credit (`n_passed / n_criteria`). Ungraded rows are excluded from the denominator. Full protocol: [docs/EVALUATION.md](docs/EVALUATION.md).

> [!NOTE]
> At `k=1`, parallel and sequential share the same procedure; any gap is a noise floor, not a mode result.

## Model channel

Live executors plug in through session transports (`FixtureTransport` / `CommandTransport`) and OpenCode-oriented JSONL parsing. Details: [docs/TRANSPORT.md](docs/TRANSPORT.md).

```bash
# Optional: isolate runs when changing models or task selection
export RECURSA_RUNS_DIR=runs/lab-seq-k1   # Unix
# $env:RECURSA_RUNS_DIR = "runs/lab-seq-k1"  # PowerShell
```

Keep bench, substrate, `k`, mode, and seed identical across refine / judge / report.

## Project layout

```text
Recursa/
├── Cargo.toml                 # Workspace
├── crates/
│   ├── recursa-core/          # Library
│   ├── recursa-demo/          # Offline demo adapter
│   └── recursa-cli/           # CLI binary
├── docs/
│   ├── EVALUATION.md
│   ├── TRANSPORT.md
│   ├── ACTIONS.md
│   └── ci.yml.example
├── README.md                  # English (source of truth)
├── README.zh-CN.md
└── README.ja.md
```

## Documentation

| Doc | Topic |
|---|---|
| [docs/EVALUATION.md](docs/EVALUATION.md) | Self-evolution scoring, base vs refined, paper orientation |
| [docs/TRANSPORT.md](docs/TRANSPORT.md) | Model channel, OpenCode JSONL, fixtures |
| [docs/ACTIONS.md](docs/ACTIONS.md) | Enabling GitHub Actions CI |
| [PROGRESS.md](PROGRESS.md) | Implementation status |

## Contributing

Issues and PRs are welcome.

1. Fork and create a feature branch.
2. Keep changes focused; match existing Rust style.
3. Run before opening a PR:

```bash
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```

4. Prefer English for code comments and PR descriptions; README translations belong in the locale files.

## License

Licensed under [Apache-2.0](LICENSE).

## Citation

Recursa implements the recursive harnessing method described in Harness²:

```bibtex
@article{xu2026harness2,
  title   = {Harness$^2$: Recursive Agent Harnessing for an Open World},
  author  = {Xu, Ruiyao and Chen, Yanfei and CuiZhu, Zhongying and Dalvi Mishra, Bhavana and Ming, Yifei and Yu, Han and Han, Rujun and Lee, Chen-Yu and Pfister, Tomas},
  journal = {arXiv preprint arXiv:2610.14224},
  year    = {2026},
  url     = {https://arxiv.org/abs/2610.14224}
}
```

Paper / upstream reference: [google-research/harness2](https://github.com/google-research/harness2).

## Acknowledgements

Inspired by the Harness² research line. Recursa is an independent Rust reimplementation focused on offline verification, typed core APIs, and a small CLI surface — not an official Google product.
