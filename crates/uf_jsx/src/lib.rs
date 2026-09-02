#![deny(missing_docs)]
//! JSX lowering for uniflowed.
//!
//! `uf` projects are written in Flow with JSX and shipped as JavaScript. Flow's
//! grammar accepts JSX, so a Flow parser is no help in telling whether a
//! module has been lowered — a build that checks its own output with one will
//! pass while emitting something no browser can load. This crate is what does
//! the lowering, and [`uf_flow::scan::TokenKind::is_jsx`] is what proves it
//! happened.
//!
//! # What it emits
//!
//! React's automatic runtime, which is what React 17 and later use:
//!
//! ```text
//! <div a={b}>{c}</div>   ->  _jsx("div", { a: b, children: c })
//! <ul>{a}{b}</ul>        ->  _jsxs("ul", { children: [a, b] })
//! <>{a}</>               ->  _jsx(_Fragment, { children: a })
//! <li key={k}>x</li>     ->  _jsx("li", { children: "x" }, k)
//! ```
//!
//! The helpers are imported once per module from
//! [`JSX_RUNTIME_SPECIFIER`], and only the ones a module turned out to need.
//!
//! # Line counts
//!
//! Every transform in the uf pipeline keeps a module's line count, because the
//! bundler's source maps are a per-line table. Lowering JSX is the hardest
//! case — an element written over eight lines becomes one call — and it is
//! done as span rewrites that never reflow. Even the runtime import is placed
//! *in front of* line one rather than above it, so the module gains a
//! statement without gaining a line. [`Transformed::code`] is checked against
//! that invariant before it is returned.

use compact_str::CompactString;
use thiserror::Error;
use uf_config::UniflowedConfig;

mod edit;
mod parse;
pub mod plugin;
mod render;
pub mod text;

#[cfg(test)]
mod tests;

pub use plugin::plugin;
pub use render::{FRAGMENT_LOCAL, Helpers, JSX_LOCAL, JSXS_LOCAL, element_type, import_offset};

/// Where the automatic runtime's helpers come from.
///
/// `@uniflowed/*` rather than `react/jsx-runtime`: a uf project never names
/// React directly, so neither does the code uf generates for it.
pub const JSX_RUNTIME_SPECIFIER: &str = "@uniflowed/jsx-runtime";

/// Longest source the transform will look at, in bytes.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Most elements one module may hold.
pub const MAX_ELEMENTS: usize = 100_000;

/// Which React runtime a project's JSX lowers to.
///
/// Named for [`flow_common::options::ReactRuntime`], the enum Flow's own
/// options use and which `uf_check` already pins to `Automatic`, so the
/// checker and the bundler cannot disagree about what a component compiles to.
///
/// [`flow_common::options::ReactRuntime`]: https://github.com/facebook/flow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReactRuntime {
    /// `_jsx(type, props, key)`, imported per module. React 17 and later.
    #[default]
    Automatic,
    /// `React.createElement(type, props, …children)`. React 16 and earlier.
    Classic,
}

impl ReactRuntime {
    /// The runtime a declared React version implies.
    ///
    /// The automatic runtime arrived in React 17, so anything earlier is
    /// classic. An unparseable version is read as the current one rather than
    /// as the oldest: `app.react.version` is a project's own declaration, and
    /// defaulting a typo to a runtime uf cannot emit would turn it into a
    /// build failure.
    #[must_use]
    pub fn from_version(version: &str) -> Self {
        let major: u32 = version
            .trim_start_matches(['^', '~', '>', '=', '<', ' '])
            .split(['.', '-', ' '])
            .next()
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(u32::MAX);
        if major >= 17 {
            Self::Automatic
        } else {
            Self::Classic
        }
    }

    /// Stable identifier, matching Flow's own spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Classic => "classic",
        }
    }
}

/// How one project's JSX is lowered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsxOptions {
    /// Which runtime to emit.
    pub runtime: ReactRuntime,
    /// Where the automatic runtime's helpers are imported from.
    pub import_source: CompactString,
    /// Most elements one module may hold.
    pub max_elements: usize,
}

impl Default for JsxOptions {
    fn default() -> Self {
        Self {
            runtime: ReactRuntime::Automatic,
            import_source: CompactString::const_new(JSX_RUNTIME_SPECIFIER),
            max_elements: MAX_ELEMENTS,
        }
    }
}

impl JsxOptions {
    /// Read the options a project already declares.
    ///
    /// There is no JSX section in `uf.config.js` and there should not be one:
    /// which runtime a project compiles to follows from the React version it
    /// declares, and inventing a second knob would let the two disagree.
    #[must_use]
    pub fn from_config(config: &UniflowedConfig) -> Self {
        Self {
            runtime: ReactRuntime::from_version(&config.app.react.version),
            ..Self::default()
        }
    }
}

/// Why a module could not be lowered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JsxError {
    /// The source is larger than [`MAX_SOURCE_BYTES`].
    #[error("source is {bytes} bytes, over the {limit} byte ceiling")]
    SourceTooLarge {
        /// Size of the rejected source.
        bytes: usize,
        /// The ceiling.
        limit: usize,
    },
    /// The module holds more elements than the transform will lower.
    #[error("module holds more than {limit} JSX elements")]
    TooManyElements {
        /// The ceiling.
        limit: usize,
    },
    /// The project declares a React version whose runtime uf does not emit.
    ///
    /// The classic runtime calls `React.createElement`, which needs `React` in
    /// scope — and a uf project never imports it, because `@uniflowed/react`
    /// is the only React it has. Emitting classic output would produce a
    /// module referring to a binding that does not exist, so this refuses
    /// instead.
    #[error(
        "app.react.version is {version}, which needs the classic JSX runtime; uf emits the \
         automatic runtime, which needs React 17 or later"
    )]
    ClassicRuntimeUnsupported {
        /// The version the project declared.
        version: CompactString,
    },
    /// Lowering changed the module's line count.
    ///
    /// Every uf transform keeps line counts so the bundler's source maps stay
    /// a per-line table. This is the guard on that invariant rather than a
    /// failure a project can cause.
    #[error("lowering changed the line count from {before} to {after}")]
    LineCountChanged {
        /// Lines the module had.
        before: usize,
        /// Lines it would have had.
        after: usize,
    },
}

/// Source with its JSX lowered to runtime calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transformed {
    /// The JavaScript to emit.
    pub code: String,
    /// Which runtime helpers the module needed.
    pub helpers: Helpers,
    /// How many elements were lowered.
    pub elements: usize,
}

impl Transformed {
    /// Whether the module held no JSX.
    #[must_use]
    pub const fn is_unchanged(&self) -> bool {
        self.elements == 0
    }
}

/// Lower every JSX element in `source`.
pub fn transform(source: &str, options: &JsxOptions) -> Result<Transformed, JsxError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(JsxError::SourceTooLarge {
            bytes: source.len(),
            limit: MAX_SOURCE_BYTES,
        });
    }
    // A module with no `<` at all cannot hold an element, and most modules in
    // a build are that module.
    if !source.contains('<') {
        return Ok(unchanged(source));
    }

    let tokens = uf_flow::scan::tokenize_jsx(source);
    if !tokens.iter().any(|token| token.kind.is_jsx()) {
        return Ok(unchanged(source));
    }

    if options.runtime == ReactRuntime::Classic {
        return Err(JsxError::ClassicRuntimeUnsupported {
            version: CompactString::const_new("16 or earlier"),
        });
    }

    let mut renderer = render::Renderer::new(&tokens, source, options.max_elements);
    renderer.collect(0, tokens.len(), 0);
    if renderer.overflowed() {
        return Err(JsxError::TooManyElements {
            limit: options.max_elements,
        });
    }
    let (mut edits, helpers, elements) = renderer.finish();

    if elements == 0 {
        return Ok(unchanged(source));
    }
    if helpers.any() {
        edits.push(edit::Edit::insert(
            render::import_offset(source),
            runtime_import(helpers, &options.import_source),
        ));
    }

    let code = edit::apply(source, &mut edits);
    if !render::preserves_lines(source, &code) {
        return Err(JsxError::LineCountChanged {
            before: edit::newlines(source),
            after: edit::newlines(&code),
        });
    }

    Ok(Transformed {
        code,
        helpers,
        elements,
    })
}

fn unchanged(source: &str) -> Transformed {
    Transformed {
        code: source.to_string(),
        helpers: Helpers::default(),
        elements: 0,
    }
}

/// The import statement a module's helpers need.
#[must_use]
pub fn runtime_import(helpers: Helpers, source: &str) -> String {
    let mut names: Vec<&str> = Vec::with_capacity(3);
    if helpers.jsx {
        names.push("jsx as _jsx");
    }
    if helpers.jsxs {
        names.push("jsxs as _jsxs");
    }
    if helpers.fragment {
        names.push("Fragment as _Fragment");
    }
    format!(
        "import {{ {} }} from {};",
        names.join(", "),
        text::quote(source)
    )
}
