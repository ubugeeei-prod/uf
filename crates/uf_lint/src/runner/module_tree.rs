//! One parse, for every rule that needs the module's tree.
//!
//! Four runners in this crate want a module read: `flow/syntax` wants what
//! the parser said about it, the accessibility, markup and `import.meta` rules
//! and the JSX rules `eslint-plugin-react` users know want to walk the tree,
//! and the two `react/*` tree rules want it lowered and rendered as ESTree.
//! Each of them used to read the module for itself, so `uf lint` over one file
//! that trips all three gates parsed it **three times** — and
//! `uf_transform::estree` has always said the intent was "one file, one
//! reading, whichever command asked".
//!
//! This is that sentence made true inside a command as well as across
//! commands. A parse of `packages/router/internal/runtime.js` is about 24,000
//! allocations, so the two that are gone were 15% of what `uf lint` spent on
//! it. See ubugeeei-prod/uf#668.
//!
//! # Why it has to be one function rather than a shared value
//!
//! [`uf_flow::Parsed`] cannot be handed back to the caller. The port's frames
//! are large, so the tree has to be built on a thread with
//! [`uf_flow::PARSE_STACK_BYTES`] — and every walk over it recurses too, so
//! the *readers* need that stack as much as the parser did. A shared tree
//! would therefore have to carry its thread with it. Running the readers
//! where the tree already is costs nothing and needs no such machinery.

use uf_config::UniflowedConfig;
use uf_profiler::profile_span;

use crate::cache::LintCache;
use crate::scan::FileScan;
use crate::{Diagnostic, LintError};

/// Read the module once and report every rule that needed it.
///
/// With a `cache`, the React tree rules' answer — nearly all of what a module
/// costs to lint — is read from `.uf/cache/lint` when an earlier run filed one
/// for this path, text and question, and filed there when it is worked out
/// here. See [`crate::cache`].
///
/// # Errors
///
/// [`LintError`] only if the parse thread cannot be started; a module the
/// parser refuses is a diagnostic, not an error.
pub(crate) fn run_module_tree_rules(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    cache: Option<&LintCache>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), LintError> {
    profile_span!("run_module_tree_rules");
    let syntax = super::flow_syntax::wanted(scan, config);
    let tree_work = super::tree::wanted(scan, config);
    let react_work = super::react_tree::wanted(scan, config);
    let jsx_work = super::react_jsx::wanted(scan, config);
    if syntax.is_none() && tree_work.is_none() && react_work.is_none() && jsx_work.is_none() {
        return Ok(());
    }

    let source = &scan.file.source;
    let path = scan.file.path.as_str();
    let question = react_work
        .as_ref()
        .map(|work| super::react_tree::question(config, work));
    let filed: Option<super::react_tree::ReactAnswer> = cache
        .zip(question.as_deref())
        .and_then(|(cache, question)| cache.read(path, source, question));

    // Nothing else wants the tree, so there is nothing to parse it for. An
    // answer is filed only for a text that parsed cleanly, and this is that
    // text, so the parse this skips is one that would have succeeded.
    if let (Some(work), Some(answer)) = (&react_work, &filed)
        && syntax.is_none()
        && tree_work.is_none()
        && jsx_work.is_none()
    {
        super::react_tree::report(scan, work, answer.clone(), diagnostics);
        return Ok(());
    }

    let filed_ref = filed.as_ref();
    let work = || {
        let outcome = match uf_flow::parse(source) {
            // A ceiling, or a parser that panicked. `uf_flow::upstream` turned
            // the same refusal into this same diagnostic when `flow/syntax`
            // parsed for itself, and the wording is `ParseFailure`'s either
            // way.
            Err(refusal) => Outcome {
                syntax: vec![uf_flow::ParseDiagnostic {
                    message: refusal.to_string(),
                    line: None,
                    column: None,
                }],
                ..Outcome::default()
            },
            Ok(parsed) => {
                let mut outcome = Outcome {
                    syntax: parsed.diagnostics.clone(),
                    ..Outcome::default()
                };
                // A recovered tree is the parser's guess at what the author
                // meant, and reporting an `<img>` the parser invented would be
                // reporting a file that does not exist. `flow/syntax` has the
                // errors; the tree rules stop here. Both runners applied this
                // rule for themselves before they shared a parse.
                if parsed.is_ok() {
                    if let Some(work) = &tree_work {
                        outcome.tree = super::tree::walk(&parsed, work);
                    }
                    if let Some(work) = &jsx_work {
                        outcome.jsx = super::react_jsx::walk(&parsed, work);
                    }
                    if let Some(work) = &react_work {
                        outcome.react = Some(match filed_ref {
                            Some(answer) => answer.clone(),
                            None => {
                                outcome.worked_out = true;
                                super::react_tree::analyse_parsed(&parsed, scan, config, work)
                            }
                        });
                    }
                }
                outcome
            }
        };
        // Spans opened here are thread-local and die with the thread; this is
        // the hand-over `scope::flush_thread_spans` documents.
        uf_profiler::scope::flush_thread_spans();
        outcome
    };

    // The parse and every tree rule run on the thread below, so a
    // `uf_profiler::ThreadWindow` open on this one would count none of them.
    // The worker hands its allocations back with its outcome, the same way it
    // hands its spans over above — or `tests/allocation_budget.rs` would be
    // measuring the scan and the thread start, and nothing #668 is about.
    let allocations = uf_profiler::Handover::capture();
    let outcome = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("uf-lint-module".into())
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, move || allocations.run(work))
            .map_err(|error| LintError::Flow(uf_flow::FlowError::Initialize(error.to_string())))?
            .join()
            .map(uf_profiler::HandedBack::receive)
            .map_err(|_| {
                LintError::Flow(uf_flow::FlowError::Runtime(
                    "the Flow parser thread panicked".to_owned(),
                ))
            })
    })?;

    if let Some(severity) = syntax {
        super::flow_syntax::report(scan, severity, &outcome.syntax, diagnostics);
    }
    if let Some(work) = &tree_work {
        super::tree::report(scan, work, outcome.tree, diagnostics);
    }
    if let Some(work) = &jsx_work {
        super::react_jsx::report(scan, work, outcome.jsx, diagnostics);
    }
    if let (Some(work), Some(answer)) = (&react_work, outcome.react) {
        if outcome.worked_out
            && let (Some(cache), Some(question)) = (cache, &question)
        {
            cache.write(path, source, question, &answer);
        }
        super::react_tree::report(scan, work, answer, diagnostics);
    }
    Ok(())
}

/// What the one parse produced, for each runner that asked.
#[derive(Default)]
struct Outcome {
    syntax: Vec<uf_flow::ParseDiagnostic>,
    tree: Vec<super::tree::Finding>,
    jsx: Vec<super::react_jsx::Finding>,
    /// What the React tree rules worked out, or [`None`] when they did not
    /// run: nobody asked, or the tree was recovered from errors.
    react: Option<super::react_tree::ReactAnswer>,
    /// Whether [`Self::react`] was worked out here rather than read from the
    /// cache, and so is new to it.
    worked_out: bool,
}
