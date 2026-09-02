//! The `uf:react-compiler` stage, as a plugin the container can run.
//!
//! In `mode: "syntax"` the stage rewrites nothing — that is what the mode
//! means. Its `Transform` hook therefore always answers
//! [`HookOutcome::Passthrough`] when a module is sound, and refuses the module
//! when it is not, so a build cannot ship code the compiler could not have
//! compiled.

use std::sync::mpsc::{Receiver, channel};

use compact_str::CompactString;
use uf_plugin::{BuiltinPlugin, FnPlugin, HookFailure, HookOutcome, ModuleCode};

use crate::analyze::validate;
use crate::error::ReactCompilerError;
use crate::rule::ReactDiagnostic;

/// One module's findings, kept with the module they came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleFindings {
    /// The module id the container handed the plugin.
    pub id: CompactString,
    /// What the validator found there.
    pub diagnostics: Vec<ReactDiagnostic>,
}

/// The half of the findings channel a build keeps.
///
/// A `Transform` hook can answer with one typed failure, which would lose the
/// position of every finding after the first. Each module's findings are sent
/// down a channel instead, so a build can render all of them through
/// `uf_term`'s code frame and still let the hook decide whether to refuse.
#[derive(Debug)]
pub struct FindingsSink {
    receiver: Receiver<ModuleFindings>,
}

impl FindingsSink {
    /// Take everything sent so far, in the order the modules were validated.
    pub fn drain(&self) -> Vec<ModuleFindings> {
        let mut found = Vec::new();
        while let Ok(module) = self.receiver.try_recv() {
            found.push(module);
        }
        found
    }
}

/// How the stage answers a module that does not validate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnFinding {
    /// Refuse the module, so the build stops. What `mode: "syntax"` is for.
    #[default]
    Refuse,
    /// Send the findings on and let the module through.
    Report,
}

/// Build the `uf:react-compiler` plugin, and the sink its findings arrive on.
///
/// ```
/// use uf_plugin::{PipelineMode, PluginContainer, PluginHook};
/// use uf_react_compiler::{OnFinding, plugin};
///
/// let (compiler, findings) = plugin(OnFinding::Report);
/// let container = PluginContainer::build(PipelineMode::Build, vec![Box::new(compiler)])
///     .expect("one plugin resolves");
///
/// assert_eq!(container.names().next(), Some("uf:react-compiler"));
/// assert!(container.implements(PluginHook::Transform));
///
/// // Syntax mode never rewrites: a sound module comes back untouched.
/// let sound = "component Page() { return null; }\n";
/// assert!(
///     container
///         .transform("app/page.js", sound)
///         .expect("no failure")
///         .is_passthrough()
/// );
///
/// container
///     .transform("app/bad.js", "component Page() { if (true) { useState(0); } }\n")
///     .expect("reporting mode lets the module through");
/// assert_eq!(findings.drain().len(), 1);
/// ```
pub fn plugin(on_finding: OnFinding) -> (FnPlugin, FindingsSink) {
    let (sender, receiver) = channel::<ModuleFindings>();
    let plugin =
        FnPlugin::new(BuiltinPlugin::ReactCompiler.descriptor()).on_transform(move |input| {
            let diagnostics = match validate(input.code) {
                Ok(diagnostics) => diagnostics,
                Err(error) => return Err(failure(&error)),
            };
            let Some(first) = diagnostics.first() else {
                return Ok(HookOutcome::<ModuleCode>::Passthrough);
            };
            let refusal = HookFailure::Rejected { rule: first.rule() };
            // A dropped receiver means the build stopped collecting; the
            // refusal below is what actually stops it.
            let _ = sender.send(ModuleFindings {
                id: CompactString::new(input.id),
                diagnostics,
            });
            match on_finding {
                OnFinding::Refuse => Err(refusal),
                OnFinding::Report => Ok(HookOutcome::Passthrough),
            }
        });
    (plugin, FindingsSink { receiver })
}

/// Report a validation failure in the container's own vocabulary.
fn failure(error: &ReactCompilerError) -> HookFailure {
    match *error {
        ReactCompilerError::SourceTooLarge { bytes, limit } => {
            HookFailure::InputTooLarge { bytes, limit }
        }
        ReactCompilerError::ScopeTooDeep { offset, .. } => {
            HookFailure::UnsupportedSyntax { offset }
        }
    }
}
