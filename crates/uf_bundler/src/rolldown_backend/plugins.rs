//! uf's build stages, as Rolldown plugins.
//!
//! Each one is the same closure the hand-written container ran, moved onto
//! `rolldown_plugin::Plugin` so Rolldown drives it. The hook names line up
//! because Rolldown implements the Rollup/Vite contract, which is the contract
//! uf's own container was modelled on.
//!
//! Every transform filters on the module id before touching anything: Rolldown
//! synthesises runtime modules of its own, and handing those to a Flow type
//! eraser turns its own helpers into syntax errors.

use std::borrow::Cow;

use camino::{Utf8Path, Utf8PathBuf};
use rolldown_plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
    HookResolveIdReturn, HookTransformArgs, HookTransformOutput, HookTransformReturn, HookUsage,
    Plugin, PluginContext, SharedLoadPluginContext, SharedTransformPluginContext,
};
use uf_jsx::JsxOptions;

use crate::pipeline::{
    ASSET_EXTENSIONS, ROUTE_TABLE_MODULE, ROUTE_TABLE_SPECIFIER, asset_module,
    blank_directive_prologue,
};

/// Whether a module id is one of the project's own JavaScript files.
///
/// Rolldown's synthesised modules are not on disk and must reach its parser
/// exactly as it wrote them.
fn is_project_module(id: &str) -> bool {
    id.ends_with(".js") && !id.starts_with('\0')
}

/// `uf:flow` — Flow in, JavaScript out.
#[derive(Debug)]
pub(crate) struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("uf:flow")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::Transform
    }

    async fn transform(
        &self,
        _ctx: SharedTransformPluginContext,
        args: &HookTransformArgs<'_>,
    ) -> HookTransformReturn {
        if !is_project_module(args.id) {
            return Ok(None);
        }
        let stripped = uf_flow::strip_types(args.code)
            .map_err(|error| anyhow::anyhow!("{}: {error}", args.id))?;
        if stripped.is_unchanged() {
            return Ok(None);
        }
        Ok(Some(HookTransformOutput {
            code: Some(stripped.code),
            ..Default::default()
        }))
    }
}

/// `uf:rsc` — blank the `"use client"` / `"use server"` prologue.
///
/// The directives are read by `uf_rsc` before the build and mean nothing to a
/// JavaScript engine, so they are blanked rather than deleted: every byte after
/// them keeps its offset, and so does every source map entry.
#[derive(Debug)]
pub(crate) struct RscPlugin;

impl Plugin for RscPlugin {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("uf:rsc")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::Transform
    }

    async fn transform(
        &self,
        _ctx: SharedTransformPluginContext,
        args: &HookTransformArgs<'_>,
    ) -> HookTransformReturn {
        if !is_project_module(args.id) {
            return Ok(None);
        }
        Ok(
            blank_directive_prologue(args.code).map(|code| HookTransformOutput {
                code: Some(code),
                ..Default::default()
            }),
        )
    }
}

/// `uf:jsx` — lower JSX to the React automatic runtime.
#[derive(Debug)]
pub(crate) struct JsxPlugin {
    pub(crate) options: JsxOptions,
}

impl Plugin for JsxPlugin {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("uf:jsx")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::Transform
    }

    async fn transform(
        &self,
        _ctx: SharedTransformPluginContext,
        args: &HookTransformArgs<'_>,
    ) -> HookTransformReturn {
        if !is_project_module(args.id) {
            return Ok(None);
        }
        let transformed = uf_jsx::transform(args.code, &self.options)
            .map_err(|error| anyhow::anyhow!("{}: {error}", args.id))?;
        if transformed.code == args.code.as_str() {
            return Ok(None);
        }
        Ok(Some(HookTransformOutput {
            code: Some(transformed.code),
            ..Default::default()
        }))
    }
}

/// `uf:router` — serve the generated route table as a virtual module.
#[derive(Debug)]
pub(crate) struct RouterPlugin {
    pub(crate) table: String,
}

impl Plugin for RouterPlugin {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("uf:router")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load
    }

    async fn resolve_id(
        &self,
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> HookResolveIdReturn {
        Ok(
            (args.specifier == ROUTE_TABLE_SPECIFIER).then(|| HookResolveIdOutput {
                id: ROUTE_TABLE_MODULE.into(),
                ..Default::default()
            }),
        )
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        Ok((args.id == ROUTE_TABLE_MODULE).then(|| HookLoadOutput {
            code: self.table.clone().into(),
            ..Default::default()
        }))
    }
}

/// `uf:asset` — turn a non-JavaScript import into a module exporting its URL.
#[derive(Debug)]
pub(crate) struct AssetPlugin {
    pub(crate) root: Utf8PathBuf,
}

impl AssetPlugin {
    fn is_asset(id: &str) -> bool {
        Utf8Path::new(id)
            .extension()
            .is_some_and(|extension| ASSET_EXTENSIONS.binary_search(&extension).is_ok())
    }
}

impl Plugin for AssetPlugin {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("uf:asset")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::Load
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        if !Self::is_asset(args.id) {
            return Ok(None);
        }
        let relative = Utf8Path::new(args.id)
            .strip_prefix(&self.root)
            .unwrap_or(Utf8Path::new(args.id));
        Ok(Some(HookLoadOutput {
            code: asset_module(relative).into(),
            ..Default::default()
        }))
    }
}
