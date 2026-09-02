//! Turning one module into the block of JavaScript that goes in a chunk.
//!
//! A module becomes an immediately-invoked arrow function whose result is its
//! namespace object:
//!
//! ```js
//! const __uf_m0 = (() => {
//! const helper = __uf_m1.helper;
//! …the module, with its import and export statements blanked…
//! return {"default": Page};
//! })();
//! ```
//!
//! The wrapper is what makes renaming unnecessary: every module keeps its own
//! top-level scope, so two modules in one chunk can both declare `styles`
//! without colliding and without a scope-hoisting pass that would have to be
//! right about every binding in the language.
//!
//! Rewriting is a blank, never a delete. An erased `import` keeps its line
//! terminators, so the module's line *n* is still line *n* of the block and the
//! chunk's source map stays a per-line table. The binding lines are prepended
//! rather than substituted in place for the same reason ES modules hoist their
//! imports: a function declared in the module may run before the statement the
//! import was written on.

use compact_str::CompactString;
use uf_infra::FxHashMap;

use crate::graph::{BundleModule, Edge};
use crate::record::{ExportSource, ImportBinding, Patch, PatchText};
use crate::shake::UsedExports;

use super::reference::{ExternalKind, References, STAR_HELPER, is_identifier, quote};

/// One module, ready to be written into a chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleBody {
    /// Binding lines, hoisted to the top of the wrapper.
    pub bindings: Vec<String>,
    /// The module's own code, with its import and export statements blanked.
    pub code: String,
    /// The `return { … };` line.
    pub namespace: String,
    /// Whether the chunk needs the `export *` helper.
    pub needs_star_helper: bool,
}

/// Render one module against the names its chunk has chosen.
pub fn render_module(
    module: &BundleModule,
    used: &UsedExports,
    references: &mut References,
    chunk_of: &FxHashMap<usize, usize>,
) -> ModuleBody {
    let mut bindings = Vec::new();
    let mut sources: FxHashMap<usize, CompactString> = FxHashMap::default();
    let mut needs_star_helper = false;

    for (position, import) in module.record.imports.iter().enumerate() {
        if !import.form.is_linked() {
            continue;
        }
        let Some(edge) = module.edge(position) else {
            continue;
        };

        match edge {
            Edge::Module(target) => {
                let chunk = chunk_of.get(&target.get()).copied().unwrap_or_default();
                let base = references.module_reference(*target, chunk);
                for binding in &import.bindings {
                    bindings.push(binding_line(binding, &base));
                }
                sources.insert(position, base);
            }
            Edge::External(specifier) => {
                if import.bindings.is_empty() {
                    // A re-export needs the namespace object; a bare `import "…"`
                    // needs only the side effect, and asking for a namespace it
                    // never reads would keep a binding nothing uses.
                    if reexports_from(module, position) {
                        let base = references.external(specifier, ExternalKind::Namespace);
                        sources.insert(position, base);
                    } else {
                        references.external(specifier, ExternalKind::Bare);
                    }
                    continue;
                }
                for binding in &import.bindings {
                    let kind = match binding {
                        ImportBinding::Default { .. } => ExternalKind::Default,
                        ImportBinding::Namespace { .. } => ExternalKind::Namespace,
                        ImportBinding::Named { imported, .. } => {
                            ExternalKind::Named(imported.clone())
                        }
                    };
                    let alias = references.external(specifier, kind);
                    bindings.push(format!("const {} = {alias};\n", binding.local()));
                }
                // Only a re-export needs the namespace object; asking for one
                // otherwise would emit an `import * as` nothing reads.
                if reexports_from(module, position) {
                    let base = references.external(specifier, ExternalKind::Namespace);
                    sources.insert(position, base);
                }
            }
        }
    }

    let mut entries: Vec<String> = Vec::new();
    for import in &module.record.star_reexports {
        if let Some(base) = sources.get(import) {
            needs_star_helper = true;
            entries.push(format!("...{STAR_HELPER}({base})"));
        }
    }

    let mut named: Vec<(&str, String)> = Vec::new();
    for export in &module.record.exports {
        if !used.contains(&export.exported) {
            continue;
        }
        let value = match &export.source {
            ExportSource::Local { local } if is_identifier(local) => local.to_string(),
            ExportSource::Local { .. } => continue,
            ExportSource::Reexport { import, imported } => match sources.get(import) {
                Some(base) if imported == "*" => base.to_string(),
                Some(base) => format!("{base}[{}]", quote(imported)),
                None => continue,
            },
        };
        named.push((export.exported.as_str(), value));
    }
    named.sort_by(|left, right| left.0.cmp(right.0));
    named.dedup_by(|left, right| left.0 == right.0);
    entries.extend(
        named
            .into_iter()
            .map(|(name, value)| format!("{}: {value}", quote(name))),
    );

    ModuleBody {
        bindings,
        code: apply_patches(&module.code, &module.record.patches),
        namespace: format!("return {{{}}};\n", entries.join(", ")),
        needs_star_helper,
    }
}

/// Whether any export of `module` republishes names from import `position`.
fn reexports_from(module: &BundleModule, position: usize) -> bool {
    module.record.star_reexports.contains(&position)
        || module.record.exports.iter().any(|export| {
            matches!(&export.source, ExportSource::Reexport { import, .. } if *import == position)
        })
}

/// The `const` line one import binding turns into.
fn binding_line(binding: &ImportBinding, base: &str) -> String {
    match binding {
        ImportBinding::Namespace { local } => format!("const {local} = {base};\n"),
        ImportBinding::Default { local } => format!("const {local} = {base}[\"default\"];\n"),
        ImportBinding::Named { imported, local } => {
            format!("const {local} = {base}[{}];\n", quote(imported))
        }
    }
}

/// Apply a module's rewrites, blanking erased spans.
///
/// Overlapping patches keep the first, and a patch pointing past the end of the
/// source is dropped: the record and the code always come from the same string,
/// but the emitter must not be the thing that panics if that ever stops being
/// true.
#[must_use]
pub fn apply_patches(code: &str, patches: &[Patch]) -> String {
    let mut out = String::with_capacity(code.len());
    let mut cursor = 0usize;

    for patch in patches {
        if patch.start < cursor || patch.end > code.len() || patch.start > patch.end {
            continue;
        }
        out.push_str(&code[cursor..patch.start]);
        match patch.text {
            PatchText::Blank => blank_into(&mut out, &code[patch.start..patch.end]),
            PatchText::Text(text) => out.push_str(text),
        }
        cursor = patch.end;
    }
    out.push_str(&code[cursor..]);
    out
}

/// Overwrite a span with spaces, keeping every line terminator.
fn blank_into(out: &mut String, span: &str) {
    for character in span.chars() {
        match character {
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => out.push(character),
            _ => {
                for _ in 0..character.len_utf8() {
                    out.push(' ');
                }
            }
        }
    }
}
