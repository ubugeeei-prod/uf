//! uf's own build stages, wired into the plugin container.
//!
//! [`uf_plugin`] owns the hook contract and the run order, and it ships
//! descriptors for the six stages a uf build has. Until now those descriptors
//! were a plan nobody executed. This module hands each of them the closure that
//! does its work, so the pipeline `uf inspect --json` prints is the pipeline
//! that actually runs:
//!
//! | stage | what it now does |
//! |---|---|
//! | `uf:flow` | erases Flow types with [`uf_flow::strip_types`] |
//! | `uf:router` | resolves and generates the virtual route table |
//! | `uf:rsc` | blanks the `"use client"` / `"use server"` prologue |
//! | `uf:jsx` | lowers JSX with [`uf_jsx::transform`] |
//! | `uf:asset` | resolves non-JavaScript imports to a URL module |
//! | `uf:style` | placed; `uf_stylex::plugin` is not connected here yet |
//! | `uf:react-compiler` | placed; `uf_react_compiler::plugin` likewise |
//!
//! The last two are honest gaps rather than hidden ones: the descriptors are in
//! the container, in the right band, and both crates now ship the transform
//! that belongs behind them — connecting them needs a place for a stylesheet
//! and for findings to go, which is a change to the build rather than to this
//! table.

use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;
use uf_config::{PipelineMode, UniflowedConfig};
use uf_flow::scan::{TokenKind, starts_statement, tokenize};
use uf_jsx::JsxOptions;
use uf_plugin::{
    BuiltinPlugin, BuiltinSet, ContainerError, FnPlugin, HookFailure, HookOutcome, ModuleCode,
    Plugin, PluginContainer, ResolvedId, resolve_project_plugins,
};
use uf_router::Route;

/// The specifier the router stage claims.
pub const ROUTE_TABLE_SPECIFIER: &str = "@uniflowed/router/routes";

/// The virtual module the route table is generated into.
///
/// A path under `.uf/` rather than a `uf:` scheme, so it passes the same
/// containment guard every other module path does instead of needing an
/// exception.
pub const ROUTE_TABLE_MODULE: &str = ".uf/router-routes.js";

/// Extensions the asset stage claims, sorted for binary search.
pub const ASSET_EXTENSIONS: &[&str] = &[
    "avif", "css", "gif", "ico", "jpeg", "jpg", "json", "png", "svg", "txt", "wasm", "webp",
    "woff", "woff2",
];

/// Why a pipeline could not be assembled.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PipelineError {
    /// A plugin entry in `uf.config.js` names something outside the project.
    #[error(transparent)]
    Resolve(#[from] uf_plugin::ResolveError),
    /// The resolved plugins do not form a usable pipeline.
    #[error(transparent)]
    Container(#[from] ContainerError),
}

/// Build the container a `uf build` or `uf dev` runs through.
pub fn build_pipeline(
    config: &UniflowedConfig,
    root: &Utf8Path,
    mode: PipelineMode,
    routes: &[Route],
) -> Result<PluginContainer, PipelineError> {
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();

    for builtin in BuiltinSet::from_config(config).iter() {
        plugins.push(wire(builtin, config, root, routes));
    }
    for descriptor in resolve_project_plugins(config, root)? {
        plugins.push(Box::new(FnPlugin::new(descriptor)));
    }

    Ok(PluginContainer::build(mode, plugins)?)
}

/// Give one built-in descriptor the closure that does its work.
fn wire(
    builtin: BuiltinPlugin,
    config: &UniflowedConfig,
    root: &Utf8Path,
    routes: &[Route],
) -> Box<dyn Plugin> {
    let descriptor = builtin.descriptor();
    match builtin {
        BuiltinPlugin::Flow => Box::new(FnPlugin::new(descriptor).on_transform(strip_flow)),
        BuiltinPlugin::Jsx => Box::new(uf_jsx::plugin(JsxOptions::from_config(config))),
        BuiltinPlugin::Router => {
            let table = route_table(routes);
            Box::new(
                FnPlugin::new(descriptor)
                    .on_resolve_id(|input| {
                        Ok(if input.specifier == ROUTE_TABLE_SPECIFIER {
                            HookOutcome::Handled(ResolvedId::virtual_module(ROUTE_TABLE_MODULE))
                        } else {
                            HookOutcome::Passthrough
                        })
                    })
                    .on_load(move |input| {
                        Ok(if input.id == ROUTE_TABLE_MODULE {
                            HookOutcome::Handled(ModuleCode::new(table.clone()))
                        } else {
                            HookOutcome::Passthrough
                        })
                    }),
            )
        }
        BuiltinPlugin::Rsc => Box::new(FnPlugin::new(descriptor).on_transform(|input| {
            Ok(match blank_directive_prologue(input.code) {
                Some(code) => HookOutcome::Handled(ModuleCode::new(code)),
                None => HookOutcome::Passthrough,
            })
        })),
        BuiltinPlugin::Asset => {
            let root = root.to_path_buf();
            Box::new(
                FnPlugin::new(descriptor)
                    .on_resolve_id(move |input| {
                        Ok(resolve_asset(&root, input.specifier, input.importer))
                    })
                    .on_load(|input| {
                        Ok(match asset_extension(Utf8Path::new(input.id)) {
                            Some(_) => HookOutcome::Handled(ModuleCode::new(asset_module(
                                Utf8Path::new(input.id),
                            ))),
                            None => HookOutcome::Passthrough,
                        })
                    }),
            )
        }
        // Placed in the pipeline, with the work still to come. See the module
        // docs: an empty transform here is a gap that `uf inspect` can see.
        BuiltinPlugin::Style | BuiltinPlugin::ReactCompiler => Box::new(FnPlugin::new(descriptor)),
    }
}

/// The `uf:flow` transform: Flow in, JavaScript out.
fn strip_flow(input: uf_plugin::TransformInput<'_>) -> uf_plugin::HookResult<ModuleCode> {
    match uf_flow::strip_types(input.code) {
        Ok(stripped) if stripped.is_unchanged() => Ok(HookOutcome::Passthrough),
        Ok(stripped) => Ok(HookOutcome::Handled(ModuleCode::new(stripped.code))),
        Err(uf_flow::StripError::SourceTooLarge { bytes, limit }) => {
            Err(HookFailure::InputTooLarge { bytes, limit })
        }
    }
}

/// The generated route table module.
fn route_table(routes: &[Route]) -> String {
    let mut source = String::with_capacity(routes.len() * 96 + 64);
    source.push_str("// generated by uf:router\nexport const routes = [\n");
    for route in routes {
        source.push_str("  { path: ");
        source.push_str(&crate::emit::quote(&route.path));
        source.push_str(", page: ");
        source.push_str(&crate::emit::quote(route.page.as_str()));
        source.push_str(", layout: ");
        source.push_str(if route.has_layout { "true" } else { "false" });
        source.push_str(", middleware: ");
        source.push_str(if route.has_middleware {
            "true"
        } else {
            "false"
        });
        source.push_str(" },\n");
    }
    source.push_str("];\nexport default routes;\n");
    source
}

/// The `uf:asset` resolver: a relative import of a non-JavaScript file.
fn resolve_asset(
    root: &Utf8Path,
    specifier: &str,
    importer: Option<&str>,
) -> HookOutcome<ResolvedId> {
    let Some(importer) = importer else {
        return HookOutcome::Passthrough;
    };
    if asset_extension(Utf8Path::new(specifier)).is_none() {
        return HookOutcome::Passthrough;
    }
    let uf_rsc::SpecifierResolution::Relative(path) =
        uf_rsc::resolve_specifier(Utf8Path::new(importer), specifier)
    else {
        return HookOutcome::Passthrough;
    };
    if !uf_rsc::is_inside_project(&path) || !root.join(&path).is_file() {
        return HookOutcome::Passthrough;
    }
    HookOutcome::Handled(ResolvedId::bundled(path.as_str()))
}

/// The module an asset import becomes: its published URL, and nothing else.
#[must_use]
pub fn asset_module(path: &Utf8Path) -> String {
    format!("export default {};\n", crate::emit::quote(&asset_url(path)))
}

/// Where an asset is published, relative to the site root.
#[must_use]
pub fn asset_url(path: &Utf8Path) -> String {
    format!("/{}/{}", crate::emit::ASSET_DIR, asset_file_name(path))
}

/// The flattened name an asset is copied to inside the output directory.
///
/// Flattening rather than mirroring the source tree keeps two assets with the
/// same base name apart, and keeps the emitted URL derivable from the module
/// path alone — no counter, no build-order dependency.
#[must_use]
pub fn asset_file_name(path: &Utf8Path) -> String {
    let mut name = String::with_capacity(path.as_str().len());
    for byte in path.as_str().bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-' => name.push(byte as char),
            _ => name.push('-'),
        }
    }
    name
}

/// The asset extension of a path, if it has one uf handles.
#[must_use]
pub fn asset_extension(path: &Utf8Path) -> Option<&str> {
    let extension = path.extension()?;
    ASSET_EXTENSIONS.binary_search(&extension).ok()?;
    Some(extension)
}

/// Blank a `"use client"` or `"use server"` prologue, if there is one.
///
/// A directive that survives into a chunk sits inside a function body, where it
/// means nothing and reads as a stray expression statement. Blanking rather
/// than deleting keeps the module's line numbers, so the chunk's source map
/// stays exact.
#[must_use]
pub fn blank_directive_prologue(code: &str) -> Option<String> {
    let tokens = tokenize(code);
    let mut spans: Vec<(usize, usize)> = Vec::new();

    for (position, token) in tokens.iter().enumerate() {
        if !starts_statement(&tokens, position) {
            break;
        }
        if token.is_punct(b';') {
            continue;
        }
        if token.kind != TokenKind::String {
            break;
        }
        let text = token.text(code);
        let content = &text[1..text.len().saturating_sub(1)];
        if content != "use client" && content != "use server" {
            continue;
        }
        let end = match tokens.get(position + 1) {
            Some(next) if next.is_punct(b';') => next.end,
            _ => token.end,
        };
        spans.push((token.start, end));
    }

    if spans.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(code.len());
    let mut cursor = 0usize;
    for (start, end) in spans {
        out.push_str(&code[cursor..start]);
        // A directive holds no line terminator, so blanking it byte for byte
        // keeps both the module's offsets and its line numbers.
        for _ in start..end {
            out.push(' ');
        }
        cursor = end;
    }
    out.push_str(&code[cursor..]);
    Some(out)
}

/// The entry modules a build starts from: the config entries and every route.
#[must_use]
pub fn build_entries(
    config: &UniflowedConfig,
    root: &Utf8Path,
    routes: &[Route],
) -> Vec<Utf8PathBuf> {
    let mut entries: Vec<Utf8PathBuf> = Vec::new();

    for entry in &config.build.entries {
        let path = Utf8PathBuf::from(entry.as_str());
        if root.join(&path).is_file() {
            entries.push(path);
        }
    }
    for route in routes {
        if let Ok(relative) = route.page.strip_prefix(root) {
            entries.push(relative.to_path_buf());
        }
    }

    entries.sort();
    entries.dedup();
    entries
}
