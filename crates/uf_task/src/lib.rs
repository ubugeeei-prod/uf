//! Dependency-aware, cached task execution.
//!
//! `docs/roadmap.md` promised "cached, dependency-aware task execution" and
//! `uf run` was a `sh -c` per task, in a depth-first `for` loop, with no
//! memory between runs. This crate is the three things that sentence names,
//! plus the one the `sh` was — and it is deliberately not the fifth thing it
//! could have been.
//!
//! * **Dependency-aware.** A plan is a graph, not a recursion, so two tasks
//!   with no path between them run at once — bounded by
//!   [`runner::DEFAULT_CONCURRENCY`] — and a `dependsOn` that closes a loop is
//!   an error instead of a task the visited set quietly skipped. See
//!   [`graph`].
//! * **Cached, with the default pointing at "run it".** A task is answered
//!   from `.uf/cache/task` only when it has said what it reads and every one
//!   of those files still hashes to what it hashed last time. A task that has
//!   said nothing runs, every time, for ever. See [`cache`] for the key and
//!   for where it departs from `uf check`'s.
//! * **Wrong safely.** `force` runs everything regardless;
//!   [`runner::Decision`] carries why each task ran or did not, in words, so a
//!   cache that is behaving strangely can be asked rather than guessed at; and
//!   a replayed task's recorded output is written out again, so a second run
//!   of a green pipeline reads like the first one rather than like nothing
//!   happening.
//!
//! * **Started, rather than handed to a shell.** [`command::parse`] reads a
//!   task's command string, and a command that is a program and its arguments
//!   — every one of this repository's own — is started by uf on any platform,
//!   `sh` or no `sh`. A command that uses shell syntax says which construct
//!   made it one, so the caller can name it.
//!
//! What it is not is a build system. It does not restore artefacts, it does
//! not know what a task read that the task did not declare, and it has no
//! opinion about workspaces — `--filter` and the rest of ubugeeei-prod/uf#272
//! are still open. The line it holds is that everything it *does* claim is
//! something it checked.

mod cache;
mod command;
mod digest;
mod graph;
mod inputs;
mod runner;

pub use crate::cache::{Change, TaskCache};
pub use crate::command::{Command, Direct, ShellSyntax, parse};
pub use crate::graph::{Plan, PlanError, PlanNode};
pub use crate::inputs::InputError;
pub use crate::runner::{
    Concurrency, DEFAULT_CONCURRENCY, Decision, Observe, RunOptions, RunReason, RunReport,
    ScheduledTask, Spawn, Status, TaskOutcome, run,
};
