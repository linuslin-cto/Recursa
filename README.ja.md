# Recursa

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Crates](https://img.shields.io/badge/workspace-3%20crates-informational.svg)](#crates)
[![Status](https://img.shields.io/badge/status-0.1.0%20preview-yellow.svg)](https://github.com/linuslin-cto/Recursa)

[English](README.md) | [简体中文](README.zh-CN.md) | **日本語**

> **モデル重みを固定**したまま、再帰的にエージェント・ハーネスを改善する —— パラメータではなく、周囲のシステムを最適化する。

**ハーネス**とは、モデルをエージェントとして動かす周辺システム（プロンプト、ツール、メモリ、スキル、スクリプト、オーケストレーションなど）です。**ハーネス最適化（harnessing）**はそのシステムを洗練する過程です。Recursa は、設計時に想定しなかったタスクにも出会うオープンワールド向けの、Rust ワークスペース実装です。

重みを変えずに、2 層の再帰を接続します：

| 層 | 内容 |
|---|---|
| **タスクレベル** | **Propose → Probe → Compose**：ハーネス編集を提案 → 同一タスクで効果を検証 → 行動の対比から refined ハーネスを合成（精錬中に正解ラベルは不要） |
| **ハーネス最適化レベル** | 採用したハーネスと根拠付き教訓をドメイン別経験ライブラリに保存し、後続タスクが先行タスクから学べるようにする |

オフラインの決定論デモでは、1 回の精錬で **0% → 100%**（+100 pp）。LAB catalog stub でも base→refined の明確な改善が出ます。論文側のモデル付き参考上限：WorkBuddy **+11.5 pp**、JobBench **+4.5 pp**、LAB partial **+11.8 pp**。

本リリースは **並列 / 逐次**再帰、**決定論 ledger デモ**、および **JobBench / LAB / WorkBuddy** の fixture catalog をサポートします。ローカル経路に API キーは不要です。

## 目次

- [なぜ Recursa か](#なぜ-recursa-か)
- [ハーネス構成要素](#ハーネス構成要素)
- [Crates](#crates)
- [必要条件](#必要条件)
- [インストール](#インストール)
- [クイックスタート](#クイックスタート)
- [ベンチマーク stub の実行](#ベンチマーク-stub-の実行)
- [実験](#実験)
- [評価](#評価)
- [モデルチャネル](#モデルチャネル)
- [プロジェクト構成](#プロジェクト構成)
- [ドキュメント](#ドキュメント)
- [コントリビュート](#コントリビュート)
- [ライセンス](#ライセンス)
- [引用](#引用)
- [謝辞](#謝辞)

## なぜ Recursa か

多くのエージェント構成はハーネスを固定し、モデルだけ差し替えます。Recursa はその逆です：**重みは固定、編集可能ツリーが進化**します。

- **対比的精錬** —— Propose–Probe–Compose 中はラベル付き正解ではなく、同一タスク上の候補ハーネスの*振る舞い*から学ぶ
- **予算の同一性** —— `R = 4k`、`P = 3k` で rollout と改善段階を揃え、実験を比較可能にする
- **オフライン優先** —— 認証情報なしで自己進化を検証；必要なら後からライブ実行器を接続
- **Rust コア** —— tree / ledger / schedule / selection / library / judge / report を一つのワークスペースに集約

## ハーネス構成要素

Recursa はモデル周辺に 8 つの編集可能コンポーネント（**c1…c8**）を公開します：

| # | コンポーネント | 役割 |
|---|---|---|
| c1 | System prompt | 常時オンのアイデンティティと目標 |
| c2 | Guardrails | 安全・トリガ・読み込み方針 |
| c3 | Memory | エージェントが持ち運べる長期メモ |
| c4 | Tool guidance | ツールの説明と使い方 |
| c5 | Subagents | 委譲モードと役割 |
| c6 | Skills | トリガ付きオンデマンド手順 |
| c7 | Scripts | 決定論的ヘルパー |
| c8 | Plugins | 実行器固有の拡張 |

これらはコンテキスト構築、ツール IF、委譲、ツールループ内の挙動を制御します。差分と検証は `recursa-core::tree` にあります。

## Crates

| Crate | 役割 |
|---|---|
| [`recursa-core`](crates/recursa-core) | Tree、ledger、recursion、schedule、selection、library、judge、report、catalog |
| [`recursa-demo`](crates/recursa-demo) | 決定論 ledger adapter + スクリプト化された proposer |
| [`recursa-cli`](crates/recursa-cli) | `recursa` バイナリ：`demo` / `info` / `catalog` / `run` / `judge` / `score` |

## 必要条件

- [Rust](https://www.rust-lang.org/tools/install)（edition 2021；1.70+ 推奨）
- Cargo（rustup に同梱）
- 後から任意：Node.js / OpenCode や Codex などのライブ substrate —— [docs/TRANSPORT.md](docs/TRANSPORT.md)

## インストール

リポジトリ直下でクローン＆ビルド：

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

ローカルデモに**モデル認証情報は不要**です。

## クイックスタート

```bash
# ワークスペース検証
cargo test --workspace

# 決定論デモ（Propose–Probe–Compose、k=2）
cargo run -p recursa-cli -- demo --output runs/demo --k 2

# フレームワーク識別子と予算恒等
cargo run -p recursa-cli -- info --k 2

# base と refined を採点（自己進化ゲイン）
cargo run -p recursa-cli -- score --output runs/demo --k 1
```

オフライン ledger デモの期待値：base **0%**、refined **100%**、Δ **+100 pp**。

## ベンチマーク stub の実行

fixture catalog を展開 → 精錬 → 判定。まずは少数タスクから：

```bash
cargo run -p recursa-cli -- catalog --bench lab --output datasets
cargo run -p recursa-cli -- run --bench lab --dataset datasets --output runs/lab --k 1 --max-tasks 2
cargo run -p recursa-cli -- judge --bench lab --dataset datasets --output runs/lab --k 1
```

| Benchmark | Token | インストール | 備考 |
|---|---|---|---|
| JobBench | `jb` | `catalog --bench jb` | 専門業務向け加重 rubric |
| LAB | `lab` | `catalog --bench lab` | 法務向け criteria |
| WorkBuddy | `wb` | `catalog --bench wb` | オフィス / Web / コード向け stub |

コマンドごとに `--bench` は**一つ**だけ。判定は精錬と同じ bench・dataset・`--k`・output を使ってください。任意：`RECURSA_EXTERNAL_JUDGE` に run ディレクトリへ `scores.json` を書くプログラムを指定できます。

## 実験

タスクレベル再帰は次の 2 通りでスケールします：

| モード | 挙動 |
|---|---|
| **並列**（`agg`） | 複数候補ハーネスを探索し、実行証拠から精錬を合成 |
| **逐次**（`seq`） | 連続する Propose–Probe–Compose で refined ハーネスと証拠を引き継ぐ |

| オプション | 意味 |
|---|---|
| `--k 1` | Propose–Probe–Compose を 1 回 |
| `--k K` | 予算が `K` に比例（モードあたり `R = 4k` rollouts、`P = 3k` stages） |
| `--max-tasks N` | catalog タスク数の上限（`0` = すべて） |
| `--seed` | 対応時にタスク選択を再現 |

各タスク後、ハーネス最適化レベル再帰が採用ハーネスと根拠付き教訓を残し、同ドメインの後続タスクを導きます。

## 評価

refine → judge → report の流れで、**base vs refined** を明示比較します。

```bash
cargo test -p recursa-core --test self_evolution
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```

| スイート（オフライン） | Base | Refined | Δ |
|---|---|---|---|
| Demo ledger | 0% | 100% | **+100 pp** |
| LAB catalog（n=2） | 50% | 100% | **+50 pp** |

採点は部分点（`n_passed / n_criteria`）。未採点行は分母に入れません。詳細は [docs/EVALUATION.md](docs/EVALUATION.md)。

> [!NOTE]
> `k=1` では並列と逐次は同じ手続きです。差はノイズフロアであり、モード結果ではありません。

## モデルチャネル

ライブ実行器は session transport（`FixtureTransport` / `CommandTransport`）と OpenCode 向け JSONL 解析経由で接続します。詳細：[docs/TRANSPORT.md](docs/TRANSPORT.md)。

```bash
# モデルやタスク選択を変えるときは runs を分離
export RECURSA_RUNS_DIR=runs/lab-seq-k1   # Unix
# $env:RECURSA_RUNS_DIR = "runs/lab-seq-k1"  # PowerShell
```

精錬 / 判定 / レポートで bench、substrate、`k`、mode、seed を揃えてください。

## プロジェクト構成

```text
Recursa/
├── Cargo.toml                 # Workspace
├── crates/
│   ├── recursa-core/          # ライブラリ
│   ├── recursa-demo/          # オフライン demo adapter
│   └── recursa-cli/           # CLI バイナリ
├── docs/
│   ├── EVALUATION.md
│   ├── TRANSPORT.md
│   ├── ACTIONS.md
│   └── ci.yml.example
├── README.md                  # 英語（正本）
├── README.zh-CN.md
└── README.ja.md
```

## ドキュメント

| ドキュメント | 内容 |
|---|---|
| [docs/EVALUATION.md](docs/EVALUATION.md) | 自己進化スコア、base vs refined、論文の方向性数字 |
| [docs/TRANSPORT.md](docs/TRANSPORT.md) | モデルチャネル、OpenCode JSONL、fixture |
| [docs/ACTIONS.md](docs/ACTIONS.md) | GitHub Actions CI の有効化 |
| [PROGRESS.md](PROGRESS.md) | 実装ステータス |

## コントリビュート

Issue と PR を歓迎します。

1. Fork して feature ブランチを作成
2. 変更は焦点を絞り、既存の Rust スタイルに合わせる
3. PR 前に実行：

```bash
cargo test --workspace
cargo run -p recursa-cli -- score --output runs/demo --k 1 --skip-run
```

4. コードコメントと PR 説明は英語優先；README の翻訳は各言語ファイルへ

## ライセンス

[Apache-2.0](LICENSE) です。

## 引用

Recursa は Harness² で述べられた再帰的ハーネス最適化手法を実装しています：

```bibtex
@article{xu2026harness2,
  title   = {Harness$^2$: Recursive Agent Harnessing for an Open World},
  author  = {Xu, Ruiyao and Chen, Yanfei and CuiZhu, Zhongying and Dalvi Mishra, Bhavana and Ming, Yifei and Yu, Han and Han, Rujun and Lee, Chen-Yu and Pfister, Tomas},
  journal = {arXiv preprint arXiv:2610.14224},
  year    = {2026},
  url     = {https://arxiv.org/abs/2610.14224}
}
```

論文 / 上流参照：[google-research/harness2](https://github.com/google-research/harness2)。

## 謝辞

Harness² の研究ラインに着想を得ています。Recursa はオフライン検証・型付きコア API・小さな CLI に焦点を当てた独立した Rust 再実装であり、Google の公式プロダクトではありません。
