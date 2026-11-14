# Recursa

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Crates](https://img.shields.io/badge/workspace-3%20crates-informational.svg)](#crates)
[![Status](https://img.shields.io/badge/status-0.1.0%20preview-yellow.svg)](https://github.com/linuslin-cto/Recursa)

[English](README.md) | **简体中文** | [日本語](README.ja.md)

> 在**固定模型权重**下做递归式 Agent Harnessing —— 优化模型外围系统，而不是训练参数。

**Harness（挽具）** 是让模型成为 Agent 的外围系统：提示词、工具、记忆、技能、脚本、编排等。**Harnessing** 则是持续优化这套系统。Recursa 是面向开放世界的 Rust 工作区实现：Agent 会遇到设计 harness 时未曾预见的任务。

在权重不变的前提下，连接两层递归：

| 层级 | 做什么 |
|---|---|
| **任务级** | **Propose → Probe → Compose**：提出 harness 编辑 → 在同一任务上探测效果 → 从行为对比中合成 refined harness（精炼阶段不需要标准答案反馈） |
| **Harnessing 级** | 将采纳的 harness 与有证据支持的经验写入按领域划分的经验库，让后续任务从先前任务中受益 |

离线确定性 demo：单次精炼即可 **0% → 100%**（+100 pp）。LAB catalog stub 上也有清晰的 base→refined 增益。论文中模型侧参考上限：WorkBuddy **+11.5 pp**、JobBench **+4.5 pp**、LAB partial **+11.8 pp**。

当前发布支持 **并行 / 串行** 递归、**确定性 ledger demo**，以及 **JobBench / LAB / WorkBuddy** fixture catalog —— 本地路径无需 API Key。

## 目录

- [为什么选 Recursa](#为什么选-recursa)
- [Harness 组件](#harness-组件)
- [Crates](#crates)
- [环境要求](#环境要求)
- [安装](#安装)
- [快速开始](#快速开始)
- [运行 Benchmark Stub](#运行-benchmark-stub)
- [实验](#实验)
- [评估](#评估)
- [模型通道](#模型通道)
- [项目结构](#项目结构)
- [文档](#文档)
- [贡献](#贡献)
- [许可证](#许可证)
- [引用](#引用)
- [致谢](#致谢)

## 为什么选 Recursa

多数 Agent 栈冻结 harness、只换模型。Recursa 反过来做：**权重固定，可编辑树持续演化**。

- **对比式精炼** —— 从候选 harness 在同一任务上的*行为差异*学习，而不是在 Propose–Probe–Compose 阶段依赖标注答案。
- **预算恒等** —— 按 `R = 4k`、`P = 3k` 约束 rollout 与改进阶段，保证实验可对比。
- **离线优先** —— 无密钥即可验证自演化；需要时再接入在线执行器。
- **Rust 核心** —— 在同一工作区提供类型化的 tree、ledger、schedule、selection、library、judge、report。

## Harness 组件

Recursa 在模型外围暴露八个可编辑组件（**c1…c8**）：

| # | 组件 | 作用 |
|---|---|---|
| c1 | System prompt | 常驻身份与目标 |
| c2 | Guardrails | 安全、触发与加载策略 |
| c3 | Memory | Agent 可携带的长期笔记 |
| c4 | Tool guidance | 工具描述与使用方式 |
| c5 | Subagents | 委托模式与角色 |
| c6 | Skills | 带触发条件的按需剧本 |
| c7 | Scripts | 确定性辅助脚本 |
| c8 | Plugins | 执行器相关扩展 |

它们控制上下文构造、工具接口、委托，以及工具循环内的行为。差分与校验见 `recursa-core::tree`。

## Crates

| Crate | 职责 |
|---|---|
| [`recursa-core`](crates/recursa-core) | Tree、ledger、recursion、schedule、selection、library、judge、report、catalog |
| [`recursa-demo`](crates/recursa-demo) | 确定性 ledger adapter + 脚本化 proposer |
| [`recursa-cli`](crates/recursa-cli) | `recursa` 二进制：`demo` / `info` / `catalog` / `run` / `judge` / `score` |

## 环境要求

- [Rust](https://www.rust-lang.org/tools/install)（edition 2021；建议 1.70+）
- Cargo（随 rustup 安装）
- 后续可选：Node.js / OpenCode 或 Codex 等在线 substrate —— 见 [docs/TRANSPORT.md](docs/TRANSPORT.md)

## 安装

在仓库根目录克隆并构建：

```bash
git clone https://github.com/linuslin-cto/Recursa.git
cd Recursa
cargo build -p recursa-cli --release
```

Windows（PowerShell）：

```powershell
git clone https://github.com/linuslin-cto/Recursa.git
cd Recursa
cargo build -p recursa-cli --release
```

本地 demo **不需要**模型凭证。

## 快速开始

```bash
# 验证工作区
cargo test --workspace

# 确定性 demo（Propose–Probe–Compose，k=2）
cargo run -p recursa-cli -- demo --output runs/demo --k 2

# 框架身份与预算恒等
cargo run -p recursa-cli -- info --k 2

# 对比 base 与 refined（自演化增益）
cargo run -p recursa-cli -- score --output runs/demo --k 1
```

离线 ledger demo 期望结果：base **0%**，refined **100%**，Δ **+100 pp**。

## 运行 Benchmark Stub

物化 fixture catalog → 精炼 → 评判。建议先用少量任务：

```bash
cargo run -p recursa-cli -- catalog --bench lab --output datasets
cargo run -p recursa-cli -- run --bench lab --dataset datasets --output runs/lab --k 1 --max-tasks 2
cargo run -p recursa-cli -- judge --bench lab --dataset datasets --output runs/lab --k 1
```

| Benchmark | Token | 安装 | 说明 |
|---|---|---|---|
| JobBench | `jb` | `catalog --bench jb` | 偏专业工作的加权 rubric |
| LAB | `lab` | `catalog --bench lab` | 偏法律工作的 criteria |
| WorkBuddy | `wb` | `catalog --bench wb` | 办公 / Web / 代码向 stub |

每条命令只选 **一个** `--bench`。评判必须与精炼使用相同的 bench、dataset、`--k` 与 output。可选：设置 `RECURSA_EXTERNAL_JUDGE`，指向会向 run 目录写入 `scores.json` 的程序。

## 实验

任务级递归有两种扩展方式：

| 模式 | 行为 |
|---|---|
| **并行**（`agg`） | 探索多个候选 harness，再根据执行证据合成精炼结果 |
| **串行**（`seq`） | 在连续的 Propose–Probe–Compose 步骤中携带 refined harness 与累积证据 |

| 选项 | 含义 |
|---|---|
| `--k 1` | 一次 Propose–Probe–Compose |
| `--k K` | 预算随 `K` 缩放（每模式 `R = 4k` rollouts，`P = 3k` stages） |
| `--max-tasks N` | 限制 catalog 任务数（`0` = 全部） |
| `--seed` | 在支持时复现任务选择 |

每个任务结束后，Harnessing 级递归会保留采纳的 harness 与有证据支持的经验，供同领域后续任务使用。

## 评估

流程为 refine → judge → report，并显式比较 **base vs refined**。

```bash
cargo test -p recursa-core --test self_evolution
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```

| 套件（离线） | Base | Refined | Δ |
|---|---|---|---|
| Demo ledger | 0% | 100% | **+100 pp** |
| LAB catalog（n=2） | 50% | 100% | **+50 pp** |

评分采用部分得分（`n_passed / n_criteria`）。未评分行不计入分母。完整协议见 [docs/EVALUATION.md](docs/EVALUATION.md)。

> [!NOTE]
> 在 `k=1` 时，并行与串行是同一过程；任何差异属于噪声底，而非模式结论。

## 模型通道

在线执行器通过 session transport（`FixtureTransport` / `CommandTransport`）与面向 OpenCode 的 JSONL 解析接入。详见 [docs/TRANSPORT.md](docs/TRANSPORT.md)。

```bash
# 更换模型或任务选择时建议隔离 runs
export RECURSA_RUNS_DIR=runs/lab-seq-k1   # Unix
# $env:RECURSA_RUNS_DIR = "runs/lab-seq-k1"  # PowerShell
```

精炼 / 评判 / 报告之间请保持 bench、substrate、`k`、mode、seed 一致。

## 项目结构

```text
Recursa/
├── Cargo.toml                 # Workspace
├── crates/
│   ├── recursa-core/          # 核心库
│   ├── recursa-demo/          # 离线 demo adapter
│   └── recursa-cli/           # CLI 二进制
├── docs/
│   ├── EVALUATION.md
│   ├── TRANSPORT.md
│   ├── ACTIONS.md
│   └── ci.yml.example
├── README.md                  # 英文（权威源）
├── README.zh-CN.md
└── README.ja.md
```

## 文档

| 文档 | 主题 |
|---|---|
| [docs/EVALUATION.md](docs/EVALUATION.md) | 自演化评分、base vs refined、论文方向数字 |
| [docs/TRANSPORT.md](docs/TRANSPORT.md) | 模型通道、OpenCode JSONL、fixture |
| [docs/ACTIONS.md](docs/ACTIONS.md) | 启用 GitHub Actions CI |
| [PROGRESS.md](PROGRESS.md) | 实现进度 |

## 贡献

欢迎 Issue 与 PR。

1. Fork 并创建功能分支。
2. 改动尽量聚焦，风格与现有 Rust 代码一致。
3. 开 PR 前请运行：

```bash
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```

4. 代码注释与 PR 描述优先英文；README 翻译请写入对应语言文件。

## 许可证

采用 [Apache-2.0](LICENSE)。

## 引用

Recursa 实现了 Harness² 所述的递归 harnessing 方法：

```bibtex
@article{xu2026harness2,
  title   = {Harness$^2$: Recursive Agent Harnessing for an Open World},
  author  = {Xu, Ruiyao and Chen, Yanfei and CuiZhu, Zhongying and Dalvi Mishra, Bhavana and Ming, Yifei and Yu, Han and Han, Rujun and Lee, Chen-Yu and Pfister, Tomas},
  journal = {arXiv preprint arXiv:2610.14224},
  year    = {2026},
  url     = {https://arxiv.org/abs/2610.14224}
}
```

论文 / 上游参考：[google-research/harness2](https://github.com/google-research/harness2)。

## 致谢

灵感来自 Harness² 研究线。Recursa 是独立的 Rust 再实现，侧重离线验证、类型化核心 API 与精简 CLI —— **不是** Google 官方产品。
