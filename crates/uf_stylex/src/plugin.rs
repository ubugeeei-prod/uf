//! The `uf:style` stage, as a plugin the container can run.
//!
//! `uf_plugin` owns the descriptor and knows nothing about StyleX; this module
//! owns the transform and knows nothing about ordering. [`FnPlugin`] is the
//! seam between them, which is why the container never grows a dependency on a
//! transform crate and why the stage that `uf inspect --json` prints is the
//! stage that actually runs.

use std::sync::mpsc::{Receiver, channel};

use uf_plugin::{BuiltinPlugin, FnPlugin, HookFailure, HookOutcome, ModuleCode};

use crate::compile::compile_module;
use crate::error::StyleXError;
use crate::sheet::StyleSheet;

/// Rule id reported when a module's CSS value could escape its own rule.
pub const UNSAFE_VALUE_RULE: &str = "stylex/unsafe-value";
/// Rule id reported when a module uses a key that would poison a prototype.
pub const FORBIDDEN_KEY_RULE: &str = "stylex/forbidden-key";

/// The half of the style channel a build keeps.
///
/// A `Transform` hook sees one module and returns one module, so it has nowhere
/// to put a whole build's stylesheet. Each module's rules are sent down a
/// channel instead, and the build folds them once every module has been
/// through. A channel rather than a shared, locked sheet: the transform never
/// waits on another module, and a sheet is order-insensitive, so folding at the
/// end gives the same bytes as folding as it goes.
#[derive(Debug)]
pub struct SheetSink {
    receiver: Receiver<StyleSheet>,
}

impl SheetSink {
    /// Fold every sheet sent so far into one.
    ///
    /// Safe to call more than once: what has already been folded is gone from
    /// the channel, so a second call returns only what arrived since.
    pub fn drain(&self) -> StyleSheet {
        let mut sheet = StyleSheet::new();
        while let Ok(part) = self.receiver.try_recv() {
            sheet.extend(&part);
        }
        sheet
    }
}

/// Build the `uf:style` plugin, and the sink its rules arrive on.
///
/// ```
/// use uf_plugin::{PipelineMode, PluginContainer, PluginHook};
/// use uf_stylex::plugin;
///
/// let (style, sheet) = plugin();
/// let container = PluginContainer::build(PipelineMode::Build, vec![Box::new(style)])
///     .expect("one plugin resolves");
///
/// assert_eq!(container.names().next(), Some("uf:style"));
/// assert!(container.implements(PluginHook::Transform));
///
/// let module = "import { stylex } from \"@uniflowed/stylex\";\n\
///               const s = stylex.create({ a: { color: \"red\" } });\n";
/// let rewritten = container.transform("app/page.js", module).expect("no failure");
/// assert!(rewritten.is_handled(), "the call is replaced by class names");
/// assert_eq!(sheet.drain().len(), 1);
/// ```
pub fn plugin() -> (FnPlugin, SheetSink) {
    let (sender, receiver) = channel::<StyleSheet>();
    let plugin = FnPlugin::new(BuiltinPlugin::Style.descriptor()).on_transform(move |input| {
        let compiled = match compile_module(input.code) {
            Ok(compiled) => compiled,
            Err(error) => return Err(failure(&error)),
        };
        if !compiled.changed {
            return Ok(HookOutcome::Passthrough);
        }
        // A dropped receiver means the build stopped collecting. The module was
        // still rewritten correctly, so that is not this hook's failure.
        let _ = sender.send(compiled.sheet);
        Ok(HookOutcome::Handled(ModuleCode::new(compiled.code)))
    });
    (plugin, SheetSink { receiver })
}

/// Report a compile failure in the container's own vocabulary.
///
/// The two guards that exist for security reasons report as `Rejected` with a
/// rule id, so a build log says the module was refused rather than that it was
/// unparseable — those are very different things to see in CI.
fn failure(error: &StyleXError) -> HookFailure {
    match error {
        StyleXError::SourceTooLarge { bytes, limit } => HookFailure::InputTooLarge {
            bytes: *bytes,
            limit: *limit,
        },
        StyleXError::UnsafeValue { .. } => HookFailure::Rejected {
            rule: UNSAFE_VALUE_RULE,
        },
        StyleXError::ForbiddenKey { .. } => HookFailure::Rejected {
            rule: FORBIDDEN_KEY_RULE,
        },
        other => HookFailure::UnsupportedSyntax {
            offset: other.position().map_or(0, |at| at.offset as usize),
        },
    }
}
