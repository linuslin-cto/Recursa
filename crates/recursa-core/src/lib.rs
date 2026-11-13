//! Recursa core library — recursive agent harnessing for an open world.
//!
//! Tree surface, ledger budgets, Propose–Probe–Compose recursion, selection,
//! experience library, and scheduling for recursive agent harnessing.

pub mod benchmarks;
pub mod catalog;
pub mod config;
pub mod context;
pub mod datasets;
pub mod grader;
pub mod judge;
pub mod ledger;
pub mod library;
pub mod opencode_protocol;
pub mod proposer;
pub mod recursion;
pub mod report;
pub mod runmark;
pub mod schedule;
pub mod selection;
pub mod spec;
pub mod substrate;
pub mod tree;

pub use ledger::{expected_rollouts, expected_sessions, BudgetError, Ledger};
pub use recursion::{run_mode, run_parallel, run_sequential, ModeResult, Stage};
pub use schedule::{mode_token, ns, Namespace, MODES};
pub use spec::{
    resolve_channel_model, BenchPrompts, BenchSpec, BenchmarkAdapter, ChannelSpec, RunView,
};
pub use tree::{SurfaceSpec, Tree};
