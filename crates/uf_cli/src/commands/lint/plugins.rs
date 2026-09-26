//! Project rules: lint rules a project writes in JavaScript, declared through
//! `uf.config.js`'s `plugins` and enabled in `lint.rules`, run beside uf's own.
//!
//! # Why a process
//!
//! uf cannot execute JavaScript, and the rules a team already has are
//! JavaScript. So a lint run with a project rule enabled starts one worker on
//! the project's Capability JS Host — `@uniflowed/host`'s `lint-worker.js`,
//! importing each plugin through the Flow loader `uf test` uses — hands it
//! every file's source and the ESTree uf's parser produced for it, and reads
//! back what the rules reported. A run with no project rule enabled starts
//! nothing: [`enabled_project_rules`] is a walk of `lint.rules`, and it is all
//! such a run pays.
//!
//! One [`Session`] is one worker's life, and the three callers hold it for as
//! long as they have files: `uf lint` for its pass, `--fix` for every round of
//! every file, and `uf lsp` for the editor session, so the host's start-up is
//! paid once rather than once a file or once a keystroke.
//!
//! # The shape a rule has
//!
//! ESLint's, over the subset rules are written against — see
//! `packages/host/internal/lint-rules.js` and the formatting and linting guide.
//! A plugin module's default export is `{ name, rules }`, and `name/rule` is the
//! id `lint.rules` enables. The object is also a valid Vite plugin, since Vite
//! ignores a key it does not know, so one module can be both.
//!
//! An id is a project rule only when it is enabled, names no uf rule, and sits
//! in a namespace uf does not own: `react/hooks-rule` is a misspelt uf rule,
//! and handing it to the plugins would turn a typo into a plugin's problem.
//!
//! # Fixes, and which tier they are in
//!
//! uf's tiers are about meaning: a safe fix leaves the program meaning what it
//! meant, and uf cannot read a project's fix to find out whether it does. The
//! rule can say, though, and ESLint already gives it the word for it — a rule
//! whose `meta.fixable` is `"whitespace"` promises layout-only edits, and those
//! are [`Safety::Safe`]; `"code"` is [`Safety::Unsafe`], applied by
//! `--fix-unsafe` and offered in an editor but never by `--fix` or on save.
//!
//! # What the cost is bounded by
//!
//! JavaScript rules are the part of a lint run uf does not write, so they are
//! the part that can hang it. Two ceilings, both enforced from this side,
//! because a wedged event loop runs no timer of its own: [`LOAD_BUDGET`] for
//! starting the host and importing the plugins, and [`FILE_BUDGET`] for one
//! file's rules. A host that passes either is killed and the file is named;
//! the next file gets a fresh host, at most [`MAX_HOST_STARTS`] in a session,
//! and after that the session stops trying and says so.
//!
//! Every one of those is a problem, and a problem fails the run. A rule that
//! was enabled and could not answer is a question the run did not answer, and
//! exiting `0` over it would say the opposite.
//!
//! # What the cost is measured by
//!
//! The worker times every call into a rule — `create` and each listener — and
//! returns the microseconds per rule with each file. The totals, and the
//! session's wall time with the host's start-up and each tree's trip through
//! the pipe in it, are in the report, so a slow rule is named by the run that
//! paid for it.

#[cfg(test)]
mod tests;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Value, json};
use uf_config::{CapabilityJsHost, RuleLevel, UniflowedConfig};
use uf_infra::{FxHashMap, FxHashSet, LineIndex};
use uf_lint::{Diagnostic, Severity, SourceFile};
use uf_plugin::{PluginSource, classify_plugin_name};
use uf_term::Status;
use uf_test::{HostCommand, HostKind};

use crate::commands::builder::uniflowed_package;
use crate::commands::test::uf_binary;
use crate::commands::vite::resolve_host;
use crate::fix::Safety;
use crate::support::plural;
use crate::ui::Ui;

/// How long the host may take to start and import the project's plugins.
///
/// Generous, because it covers a cold `uf transform` of every plugin module,
/// and it is paid once a session rather than once a file.
pub(crate) const LOAD_BUDGET: Duration = Duration::from_secs(30);

/// How long one file's rules may run before the host is stopped.
///
/// A rule that needs longer than this on one file is a rule a person needs to
/// hear about, and a lint run is the wrong place to wait for it: ESLint has no
/// ceiling at all, so a hung listener there hangs its CI job until the job's
/// own timeout, which names nothing.
pub(crate) const FILE_BUDGET: Duration = Duration::from_secs(5);

/// How many hosts one session may start, the first included.
pub(crate) const MAX_HOST_STARTS: usize = 3;

/// The worker module inside `@uniflowed/host`.
const WORKER: &str = "lint-worker.js";

/// What running a project's rules produced.
#[derive(Debug, Default)]
pub(crate) struct ProjectRules {
    /// Whether any project rule was enabled, so that the pass happened at all.
    pub(crate) ran: bool,
    /// The rules' findings, before they join uf's own in the report.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Microseconds spent inside each rule's own code, slowest first.
    pub(crate) timings: Vec<(&'static str, f64)>,
    /// Files the rules answered for.
    pub(crate) files: usize,
    /// Wall time of the whole session, the host's start-up included.
    pub(crate) micros: u64,
    /// Everything that kept an enabled rule from answering. Each fails the run.
    pub(crate) problems: Vec<String>,
}

/// One edit a project rule asked for, in bytes of the file it was reported in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectFix {
    /// The rule that asked for it.
    pub(crate) rule: &'static str,
    /// Which of uf's tiers the rule's `meta.fixable` puts it in.
    pub(crate) safety: Safety,
    /// Byte offset where the replaced text starts.
    pub(crate) start: usize,
    /// Byte offset just past the replaced text.
    pub(crate) end: usize,
    /// What goes in its place.
    pub(crate) text: String,
    /// The 1-based line of the diagnostic it answers.
    pub(crate) line: usize,
    /// The 1-based byte column of the diagnostic it answers.
    pub(crate) column: usize,
}

/// What the rules said about one file.
#[derive(Debug, Default)]
pub(crate) struct Answer {
    /// Their findings, at the levels `lint.rules` gives them.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// The edits those findings came with.
    pub(crate) fixes: Vec<ProjectFix>,
}

/// The enabled rule ids a project's plugins have to define, with their levels.
///
/// Empty when `plugins` is: without a plugin there is nothing that could
/// define one, and an id nothing defines was already kept rather than refused
/// before project rules existed — starting a host to say so would make every
/// such typo cost a process.
pub(crate) fn enabled_project_rules(config: &UniflowedConfig) -> Vec<(&'static str, RuleLevel)> {
    if config.plugins.is_empty() {
        return Vec::new();
    }
    let owned: FxHashSet<&str> = uf_lint::rules()
        .iter()
        .filter_map(|descriptor| {
            descriptor
                .id
                .split_once('/')
                .map(|(namespace, _)| namespace)
        })
        .collect();
    config
        .lint
        .rules
        .iter()
        .filter(|(id, level)| level.is_enabled() && uf_lint::canonical_rule_id(id).is_none())
        .filter(|(id, _)| {
            id.split_once('/').is_some_and(|(namespace, rule)| {
                !namespace.is_empty() && !rule.is_empty() && !owned.contains(namespace)
            })
        })
        .map(|(id, level)| (intern(id), *level))
        .collect()
}

/// Run the project's rules over `sources`, for `uf lint` and `uf check`.
///
/// Returns an empty, not-`ran` outcome without starting anything when no
/// project rule is enabled. An error is a project that cannot run its rules at
/// all — no host, no `@uniflowed/host`, a plugin path outside the project —
/// and everything short of that is a problem inside the outcome.
pub(crate) fn run(
    root: &Utf8Path,
    config: &UniflowedConfig,
    sources: &[SourceFile],
) -> Result<ProjectRules> {
    let Some(mut session) = Session::start(root, config)? else {
        return Ok(ProjectRules::default());
    };
    let mut diagnostics = Vec::new();
    for file in sources {
        if session.gave_up() {
            break;
        }
        diagnostics.extend(session.lint(file).diagnostics);
    }
    let mut outcome = session.finish();
    outcome.diagnostics = diagnostics;
    Ok(outcome)
}

/// A worker's life: started on the first file that needs it, restarted after
/// a file that stops it, and given up on after [`MAX_HOST_STARTS`].
pub(crate) struct Session {
    command: HostCommand,
    load: Value,
    levels: FxHashMap<&'static str, RuleLevel>,
    root: Utf8PathBuf,
    worker: Option<Worker>,
    starts: usize,
    gave_up: bool,
    started: Instant,
    micros: FxHashMap<&'static str, f64>,
    outcome: ProjectRules,
}

impl Session {
    /// A session for `config`'s project rules, or [`None`] when there are none.
    ///
    /// Starts no process: the first [`Session::lint`] does, so a session
    /// created for a pass that turns out to have no Flow file costs nothing.
    pub(crate) fn start(root: &Utf8Path, config: &UniflowedConfig) -> Result<Option<Self>> {
        let enabled = enabled_project_rules(config);
        if enabled.is_empty() {
            return Ok(None);
        }
        let modules = plugin_modules(config, root)?;
        let command = host_command(root, config)?;
        let load = json!({
            "type": "load",
            "root": root.as_str(),
            "modules": modules,
            "rules": enabled.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        });
        Ok(Some(Self {
            command,
            load,
            levels: enabled.iter().copied().collect(),
            root: root.to_path_buf(),
            worker: None,
            starts: 0,
            gave_up: false,
            started: Instant::now(),
            micros: FxHashMap::default(),
            outcome: ProjectRules {
                ran: true,
                ..ProjectRules::default()
            },
        }))
    }

    /// Whether the session has stopped trying: a load that failed, or a host
    /// that had to be started too many times.
    pub(crate) fn gave_up(&self) -> bool {
        self.gave_up
    }

    /// Everything that has kept a rule from answering so far, in order.
    pub(crate) fn problems(&self) -> &[String] {
        &self.outcome.problems
    }

    /// Run every enabled rule over one file.
    ///
    /// Never an error: a file the rules could not answer for is a problem in
    /// [`Session::problems`] and an empty answer, so one bad file costs its own
    /// findings and not the pass.
    pub(crate) fn lint(&mut self, file: &SourceFile) -> Answer {
        let mut answer = Answer::default();
        if self.gave_up || file.path.ends_with(".json") {
            return answer;
        }
        // A file that does not parse is `flow/syntax`'s to report, and there
        // is no tree to hand a rule.
        let Ok(ast) = uf_transform::estree::parse(&file.source) else {
            return answer;
        };
        if self.worker.is_none() && !self.restart() {
            return answer;
        }
        let request = json!({
            "type": "lint",
            "path": file.path,
            "filename": self.root.join(&file.path).as_str(),
            "source": file.source,
            "ast": ast,
        });
        let Some(worker) = self.worker.as_mut() else {
            return answer;
        };
        match worker.ask(&request, FILE_BUDGET) {
            Ok(reply) => {
                self.outcome.files += 1;
                collect(
                    &reply,
                    file,
                    &self.levels,
                    &mut answer,
                    &mut self.outcome.problems,
                    &mut self.micros,
                );
            }
            Err(stopped) => {
                self.worker = None;
                self.outcome.problems.push(match stopped {
                    Stopped::TimedOut => uf_infra::into_string(uf_infra::cstr!(
                        "project rules did not finish `{}` within {} s, so the rule host was \
                         stopped",
                        file.path,
                        FILE_BUDGET.as_secs()
                    )),
                    Stopped::Exited => uf_infra::into_string(uf_infra::cstr!(
                        "the rule host exited while linting `{}`; what it printed is above",
                        file.path
                    )),
                });
            }
        }
        answer
    }

    /// Start a worker and load the plugins into it, or give up and say why.
    fn restart(&mut self) -> bool {
        if self.starts == MAX_HOST_STARTS {
            self.gave_up = true;
            self.outcome.problems.push(uf_infra::into_string(uf_infra::cstr!(
                "project rules stopped after the rule host was started {MAX_HOST_STARTS} times, \
                 so the files after that went unlinted by them"
            )));
            return false;
        }
        self.starts += 1;
        let mut fresh = match Worker::start(&self.command) {
            Ok(worker) => worker,
            Err(error) => {
                self.gave_up = true;
                self.outcome
                    .problems
                    .push(uf_infra::into_string(uf_infra::cstr!("{error:#}")));
                return false;
            }
        };
        match fresh.ask(&self.load, LOAD_BUDGET) {
            Ok(reply) => {
                // The same plugins load the same way every time, so only the
                // first start's account of them is worth reading.
                if self.starts == 1 {
                    self.outcome.problems.extend(strings(&reply, "problems"));
                }
                self.worker = Some(fresh);
                true
            }
            Err(stopped) => {
                self.gave_up = true;
                self.outcome.problems.push(match stopped {
                    Stopped::TimedOut => uf_infra::into_string(uf_infra::cstr!(
                        "the rule host did not load the project's plugins within {} s",
                        LOAD_BUDGET.as_secs()
                    )),
                    Stopped::Exited => String::from(
                        "the rule host exited while loading the project's plugins; what it \
                         printed is above",
                    ),
                });
                false
            }
        }
    }

    /// Stop the worker and total what the session cost.
    pub(crate) fn finish(mut self) -> ProjectRules {
        drop(self.worker.take());
        let mut timings: Vec<(&'static str, f64)> = self.micros.drain().collect();
        timings.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
        self.outcome.timings = timings;
        self.outcome.micros = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.outcome
    }
}

/// The fixes to apply this round: the tier `allow_unsafe` admits, in document
/// order, with any edit that overlaps an earlier one left for the next round.
///
/// Overlaps are dropped rather than refused for the reason `fix::plan` drops
/// them: the fixer lints the result and plans again, so the dropped edit is
/// planned against text that exists, or not at all if the first one answered
/// it.
pub(crate) fn plan_fixes(fixes: &[ProjectFix], allow_unsafe: bool) -> Vec<&ProjectFix> {
    let mut chosen: Vec<&ProjectFix> = fixes
        .iter()
        .filter(|fix| allow_unsafe || fix.safety == Safety::Safe)
        .collect();
    chosen.sort_by_key(|fix| (fix.start, fix.end));
    let mut planned = Vec::with_capacity(chosen.len());
    let mut reached = 0;
    for fix in chosen {
        if fix.start >= reached {
            reached = fix.end;
            planned.push(fix);
        }
    }
    planned
}

/// `source` with `fixes` applied; they must be in the order [`plan_fixes`]
/// returns. An edit whose range is not on character boundaries of this text is
/// skipped, which is what a range computed against other text looks like.
pub(crate) fn apply_fixes(source: &str, fixes: &[&ProjectFix]) -> String {
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for fix in fixes {
        let usable = at <= fix.start
            && fix.start <= fix.end
            && fix.end <= source.len()
            && source.is_char_boundary(fix.start)
            && source.is_char_boundary(fix.end);
        if !usable {
            continue;
        }
        out.push_str(&source[at..fix.start]);
        out.push_str(&fix.text);
        at = fix.end;
    }
    out.push_str(&source[at..]);
    out
}

/// Sort the way [`uf_lint::LintReport::diagnostics`] is sorted, so project
/// rules' findings sit among uf's own rather than after them.
pub(crate) fn sort(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
            .then(a.rule.cmp(b.rule))
    });
}

/// The `projectRules` object in `uf lint --json`, present when the pass ran.
pub(crate) fn payload(outcome: &ProjectRules) -> Value {
    json!({
        "files": outcome.files,
        "micros": outcome.micros,
        "rules": outcome
            .timings
            .iter()
            .map(|(rule, micros)| json!({ "rule": rule, "micros": micros }))
            .collect::<Vec<_>>(),
        "problems": outcome.problems,
    })
}

/// One line saying what the pass cost, and every problem it had.
pub(crate) fn render(ui: &mut Ui, outcome: &ProjectRules) {
    if !outcome.ran {
        return;
    }
    let slowest = outcome
        .timings
        .iter()
        .take(3)
        .map(|(rule, micros)| {
            uf_infra::into_string(uf_infra::cstr!("{rule} {:.1} ms", micros / 1000.0))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let total = Duration::from_micros(outcome.micros).as_secs_f64() * 1000.0;
    let mut headline = uf_infra::into_string(uf_infra::cstr!(
        "project rules: {} over {} in {total:.0} ms",
        plural(outcome.timings.len(), "rule"),
        plural(outcome.files, "file")
    ));
    if !slowest.is_empty() {
        headline.push_str(" — ");
        headline.push_str(&slowest);
    }
    let problems: Vec<&str> = outcome.problems.iter().map(String::as_str).collect();
    ui.render(|renderer, out| {
        renderer.blank(out);
        renderer.status(out, Status::Info, &headline);
        if !problems.is_empty() {
            renderer.status(
                out,
                Status::Error,
                &uf_infra::into_string(uf_infra::cstr!(
                    "{} kept project rules from answering",
                    plural(problems.len(), "problem")
                )),
            );
            renderer.bullet_list(out, 2, &problems);
        }
    });
}

/// Each plugin entry as the worker imports it: an absolute path for a file in
/// the project, the specifier as written for a package.
fn plugin_modules(config: &UniflowedConfig, root: &Utf8Path) -> Result<Vec<String>> {
    config
        .plugins
        .iter()
        .filter_map(|entry| match classify_plugin_name(entry.name(), root) {
            Ok(PluginSource::Builtin) => None,
            Ok(PluginSource::Package { specifier }) => Some(Ok(specifier.to_string())),
            Ok(PluginSource::ProjectFile { path }) => Some(Ok(root.join(path).to_string())),
            Err(error) => Some(Err(anyhow!(uf_infra::cstr!(
                "the `plugins` entry `{}` cannot be loaded for its rules: {error}",
                entry.name()
            )))),
        })
        .collect()
}

/// The command that starts the worker: the project's host, with the Flow
/// loader registered the way `uf test` registers it.
fn host_command(root: &Utf8Path, config: &UniflowedConfig) -> Result<HostCommand> {
    let host = resolve_host(config)?;
    let kind = match host.kind {
        CapabilityJsHost::Node => HostKind::Node,
        CapabilityJsHost::Bun => HostKind::Bun,
        // Deno has a Flow loader, and `uf test` starts workers on it. What a
        // lint worker there would also need is a permission set — Deno grants
        // nothing by default, and `uf test` builds one from the project's
        // `permissions` — and a lint run that granted itself one nobody wrote
        // down is the thing that set exists to prevent. Refused by name until
        // that set is argued for this worker too.
        CapabilityJsHost::Deno => bail!(uf_infra::cstr!(
            "project rules run on Node.js or Bun, and this project's Capability JS Host is \
             Deno, where the rule worker would need a permission set uf does not grant a lint \
             run yet. Install Node.js or Bun and name it in \
             `app.runtime.capabilityJsHost.default`, or turn the project rules in `lint.rules` \
             off."
        )),
    };
    let package = uniflowed_package(root, "host", WORKER).map_err(|_| {
        anyhow!(uf_infra::cstr!(
            "project rules run in `@uniflowed/host`, and no version of it with `{WORKER}` is \
             installed for {root}: add `@uniflowed/host` to the project's dependencies and run \
             `uf install`"
        ))
    })?;
    Ok(
        HostCommand::new(kind, host.program, package.join(WORKER), root.to_path_buf())
            .with_flow_loader(
                Utf8Path::new("@uniflowed/host/register"),
                &package.join("bun-preload.js"),
                &package.join("deno-preload.js"),
            )
            .with_uf_binary(uf_binary()?),
    )
}

/// Why a request got no reply.
enum Stopped {
    /// The budget passed first.
    TimedOut,
    /// The worker's output ended, which is the worker ending.
    Exited,
}

/// A running worker and the thread reading its replies.
///
/// The reader is a thread because a blocking read cannot be given a deadline;
/// waiting on its channel can.
struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    replies: Receiver<String>,
}

impl Worker {
    fn start(command: &HostCommand) -> Result<Self> {
        let mut process = Command::new(command.program.as_std_path());
        process
            .args(&command.leading_args)
            .arg(command.worker.as_str())
            .current_dir(command.root.as_std_path())
            .env("UF_PROJECT_ROOT", command.root.as_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(binary) = &command.uf_binary {
            process.env("UF_BINARY", binary.as_str());
        }
        let mut child = process.spawn().map_err(|error| {
            anyhow!(uf_infra::cstr!(
                "could not start `{}` to run the project's rules: {error}",
                command.program
            ))
        })?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!(uf_infra::cstr!("the rule host has no stdout")))?;
        let (sender, replies) = channel();
        std::thread::Builder::new()
            .name(String::from("uf-lint-host"))
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            child,
            stdin,
            replies,
        })
    }

    /// Send one request and wait up to `budget` for its reply.
    ///
    /// There is never a stale reply to skip: a request that runs out of time
    /// ends the worker, so the next request goes to a process that has only
    /// ever seen its own.
    fn ask(&mut self, request: &Value, budget: Duration) -> Result<Value, Stopped> {
        let mut line = request.to_string();
        line.push('\n');
        let stdin = self.stdin.as_mut().ok_or(Stopped::Exited)?;
        if stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .is_err()
        {
            return Err(Stopped::Exited);
        }
        let deadline = Instant::now() + budget;
        loop {
            match self
                .replies
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(reply) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&reply) {
                        return Ok(value);
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Err(Stopped::TimedOut),
                Err(RecvTimeoutError::Disconnected) => return Err(Stopped::Exited),
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Closing stdin is how a worker hears there is no more work; the kill
        // is for the one that is not listening any more.
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Add one file's reply to `answer`, its problems to `problems` and its
/// timings to `micros`.
///
/// A diagnostic for a rule the project did not enable is dropped rather than
/// trusted: the worker was only asked for the enabled ones, so anything else is
/// a worker that is not the one this driver started.
fn collect(
    reply: &Value,
    file: &SourceFile,
    levels: &FxHashMap<&'static str, RuleLevel>,
    answer: &mut Answer,
    problems: &mut Vec<String>,
    micros: &mut FxHashMap<&'static str, f64>,
) {
    let index = LineIndex::new(&file.source);
    for reported in reply
        .get("diagnostics")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = reported.get("rule").and_then(Value::as_str) else {
            continue;
        };
        let Some((&rule, &level)) = levels.get_key_value(id) else {
            continue;
        };
        let start = byte_offset(&file.source, units(reported, "start"));
        let position = index.line_col(start);
        answer.diagnostics.push(Diagnostic {
            rule,
            severity: if level.is_error() {
                Severity::Error
            } else {
                Severity::Warn
            },
            path: Some(file.path.clone()),
            line: position.line,
            column: position.column,
            message: reported
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        });
        if let Some(fix) = reported.get("fix").filter(|fix| fix.is_object())
            && let Some(text) = fix.get("text").and_then(Value::as_str)
        {
            let fix_start = byte_offset(&file.source, units(fix, "start"));
            answer.fixes.push(ProjectFix {
                rule,
                safety: match fix.get("kind").and_then(Value::as_str) {
                    Some("whitespace") => Safety::Safe,
                    _ => Safety::Unsafe,
                },
                start: fix_start,
                end: byte_offset(&file.source, units(fix, "end")).max(fix_start),
                text: text.to_owned(),
                line: position.line,
                column: position.column,
            });
        }
    }
    for (id, spent) in reply
        .get("micros")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        if let (Some((&rule, _)), Some(spent)) = (levels.get_key_value(id.as_str()), spent.as_f64())
        {
            *micros.entry(rule).or_default() += spent;
        }
    }
    problems.extend(strings(reply, "problems"));
}

/// `value[key]` as a count of UTF-16 units, or `0`.
fn units(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|units| usize::try_from(units).ok())
        .unwrap_or(0)
}

/// The strings in `reply[key]`, or none.
fn strings(reply: &Value, key: &str) -> Vec<String> {
    reply
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The byte offset of a JavaScript string index — a count of UTF-16 code
/// units, which is what ESTree's `range` counts — in `source`.
pub(crate) fn byte_offset(source: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, character) in source.char_indices() {
        if units >= utf16 {
            return index;
        }
        units += character.len_utf16();
    }
    source.len()
}

/// A rule id with the `'static` lifetime a [`Diagnostic`] carries.
///
/// Project rule ids come from the config rather than from uf's catalogue, so
/// they are leaked once each — at most one allocation per distinct id for the
/// life of the process, which in `uf lsp` is the editor session.
fn intern(id: &str) -> &'static str {
    static IDS: OnceLock<Mutex<FxHashSet<&'static str>>> = OnceLock::new();
    let mut ids = IDS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(known) = ids.get(id) {
        return known;
    }
    let leaked: &'static str = Box::leak(id.to_owned().into_boxed_str());
    ids.insert(leaked);
    leaked
}
