# Recursa

Recursive agent harnessing with **fixed model weights**: refine the system around
the model (prompts, tools, memory, skills, scripts) instead of training parameters.

Two recursion levels:

1. **Task-level** — Propose–Probe–Compose: propose harness edits, probe them on the
   same task, compose a refined harness from behavioral contrasts (no ground-truth
   feedback required during refinement).
2. **Harnessing-level** — persist adopted harnesses and evidence-backed lessons in a
   per-domain experience library so later tasks can improve from earlier ones.

## Crates

| Crate | Role |
|---|---|
| `recursa-core` | Tree surface, ledger, recursion, schedule, selection, library, judge |
| `recursa-demo` | Deterministic ledger adapter + scripted proposer |
| `recursa-cli` | `recursa demo` / `info` / `catalog` |

## Quick start

```powershell
cargo test --workspace
cargo run -p recursa-cli -- demo --output runs/demo --k 2
cargo run -p recursa-cli -- info --k 2
cargo run -p recursa-cli -- catalog --bench lab --output datasets
```

No model credentials are required for the demo.

## Evaluation (self-evolution)

See [docs/EVALUATION.md](docs/EVALUATION.md).

```powershell
cargo test -p recursa-core --test self_evolution
cargo run -p recursa-cli -- score --output runs/demo --k 1
```

Offline: base fails signed totals (0%), refined passes (100%), Δ **+100 pp**;
task-2 sees task-1 in the domain library. Budget identity: `R=4k`, `P=3k`.

## Model channel

See [docs/TRANSPORT.md](docs/TRANSPORT.md) for `FixtureTransport` / `CommandTransport`
and OpenCode JSONL parsing.

## License

Apache-2.0

## Citation

Recursa implements the recursive harnessing method described in:

```bibtex
@article{xu2026harness2,
  title   = {Harness$^2$: Recursive Agent Harnessing for an Open World},
  author  = {Xu, Ruiyao and Chen, Yanfei and CuiZhu, Zhongying and Dalvi Mishra, Bhavana and Ming, Yifei and Yu, Han and Han, Rujun and Lee, Chen-Yu and Pfister, Tomas},
  journal = {arXiv preprint arXiv:2610.14224},
  year    = {2026},
  url     = {https://arxiv.org/abs/2610.14224}
}
```
