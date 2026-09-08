//! What `uf build` produces for a project that is not an application.
//!
//! `app.router.enabled: false` has meant "this project is a library" since the
//! config reference was written, and three places in Rust honour it — the
//! built-in plugin set, `uf prepare`'s route-type step and `uf routes`. The
//! builder did not: `uf build` had one build in it, the application build, so
//! a library scaffolded by `uf create lib` failed at the first pass with
//! `Could not resolve '<root>/app.js'` — a file a library does not have and
//! never had. See ubugeeei-prod/uf#268.
//!
//! So this module is the same move [`crate::rendering`] made for
//! `app.rendering.modes` and `build.staticBuild`: the settings that decide
//! what a build *is* are resolved once, in Rust, into a plan the builder is
//! handed. A [`LibraryPlan`] is [`Some`] exactly when the project is a
//! library, and a caller that has one is building a library — there is no
//! second place where the question is asked and no way for two answers to
//! disagree.
//!
//! # Why the switch is `app.router.enabled` and not a new key
//!
//! Because a project that has turned the file-system router off has already
//! said the thing. Adding a `build.lib.enabled` beside it would be a second
//! declaration of one fact, and the interesting failure would then be the two
//! disagreeing — a project with a router and a library build, or an
//! application whose entry uf refuses to look for. `build.lib` exists, and
//! holds the settings a library build needs that an application build has no
//! use for, but it never decides *whether* the build is a library one:
//! declaring it in a project whose router is on is refused by [`check`]
//! rather than read as a request.

use camino::Utf8Path;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::{ConfigError, UniflowedConfig};

/// The module format one pass of a library build writes.
///
/// Two are implemented and two are named. A format uf has not written is
/// refused by [`check`] with the reason, rather than accepted and silently
/// producing nothing — which is the shape of the bug this whole module is a
/// fix for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryFormat {
    /// ES modules: `dist/<entry>.js`. The default, and the only format every
    /// bundler, every modern Node and every browser reads without a wrapper.
    Es,
    /// CommonJS: `dist/<entry>.cjs`, for a consumer that still calls
    /// `require`.
    Cjs,
    /// **Planned.** A UMD bundle.
    Umd,
    /// **Planned.** An IIFE bundle for a `<script>` tag.
    Iife,
}

impl LibraryFormat {
    /// The spelling passed to the builder as `--format`, and the one written
    /// in `uf.config.js`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Es => "es",
            Self::Cjs => "cjs",
            Self::Umd => "umd",
            Self::Iife => "iife",
        }
    }

    /// Whether `uf build` writes this format today.
    ///
    /// `umd` and `iife` are not "not done yet" in the sense of a missing
    /// rollup option. Each needs a **global name** per entry — the identifier
    /// the bundle assigns itself on `window` — and what a Flow library's
    /// global should be is a decision nobody has made; guessing one from the
    /// package name would publish an API surface the author never wrote down.
    /// Neither format is reachable from a bundler or from Node, which is where
    /// a library published from uf is consumed, so the two are named and
    /// refused rather than half-written.
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        matches!(self, Self::Es | Self::Cjs)
    }

    /// The file extension a written module gets.
    ///
    /// `.cjs` rather than `.js` for CommonJS, and it is not cosmetic: the
    /// scaffolded manifest says `"type": "module"`, so a `.js` file Node loads
    /// through `require` is a `SyntaxError` about `module is not defined` at
    /// the consumer, several packages away from anything they wrote.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Es => "js",
            Self::Cjs => "cjs",
            // Never written: [`Self::is_implemented`] is false for both and
            // `check` refuses a config naming one. The arm exists because an
            // extension is a total function of a format and a `match` that is
            // total cannot go stale when one of them is implemented.
            Self::Umd | Self::Iife => "js",
        }
    }
}

/// The settings a library build has and an application build does not.
///
/// Every field has a default that builds the project `uf create lib`
/// scaffolds, so a library needs none of this in `uf.config.js`: the switch is
/// `app.router.enabled: false` and the rest follows. What the key is for is
/// the library that has more than one entry, needs a `require`-able build, or
/// depends on something its manifest does not declare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct LibraryConfig {
    /// The modules to build, relative to the project root.
    ///
    /// Not `build.entries`, and the distinction is worth the second key.
    /// `build.entries` is the *application's* entry — `[0]` of it is where the
    /// router looks for `routerView()` when `app.router.entry` is unset — and
    /// its default is `app.js`. A key whose meaning flips depending on another
    /// key is a key nobody can read, so a library says its entries here and
    /// the two never have to be told apart.
    ///
    /// Each entry names its own output: `index.js` is written to
    /// `dist/index.js`, and `internal/parse.js` to `dist/internal/parse.js`.
    /// The path, rather than the basename, so two entries can share one and
    /// there is no collision to resolve at the moment a build would have to
    /// overwrite a file.
    pub entries: Vec<CompactString>,
    /// The module formats to write, one pass each.
    pub formats: Vec<LibraryFormat>,
    /// Package names to leave as imports beyond the ones the manifest
    /// declares.
    ///
    /// A library build externalises everything in the project's
    /// `dependencies`, `peerDependencies` and `optionalDependencies`, plus the
    /// host's built-in modules — that is the default and it is the opposite of
    /// the application build's, which inlines what it can. This list is for
    /// what a manifest cannot say: a peer a consumer supplies under a
    /// different name, an import that resolves through an alias.
    pub external: Vec<CompactString>,
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            entries: vec![CompactString::const_new("index.js")],
            formats: vec![LibraryFormat::Es],
            external: Vec::new(),
        }
    }
}

/// What `uf build` writes for one library.
///
/// Resolved from a validated config, so it is infallible: [`check`] has
/// already refused every declaration with no build behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPlan {
    entries: Vec<CompactString>,
    formats: Vec<LibraryFormat>,
    external: Vec<CompactString>,
    /// Whether the project wrote `build.lib` itself, so a message can quote
    /// the key that decided rather than a default nobody typed.
    declared: bool,
}

impl LibraryPlan {
    /// The plan `config` describes, or [`None`] for an application.
    ///
    /// The only place the question "is this a library" is answered. Callers
    /// branch on the `Option` rather than re-reading `app.router.enabled`,
    /// which is what stops the builder and the command from ever disagreeing
    /// about which build ran.
    #[must_use]
    pub fn resolve(config: &UniflowedConfig) -> Option<Self> {
        if config.app.router.enabled {
            return None;
        }
        let declared = config.build.lib.is_some();
        let lib = config.build.lib.clone().unwrap_or_default();
        Some(Self {
            entries: lib.entries,
            formats: lib.formats,
            external: lib.external,
            declared,
        })
    }

    /// The modules this build bundles, in declaration order.
    #[must_use]
    pub fn entries(&self) -> &[CompactString] {
        &self.entries
    }

    /// The formats it writes, in declaration order.
    #[must_use]
    pub fn formats(&self) -> &[LibraryFormat] {
        &self.formats
    }

    /// Package names to externalise beyond the manifest's dependencies.
    #[must_use]
    pub fn external(&self) -> &[CompactString] {
        &self.external
    }

    /// The clause that says which build ran and why.
    ///
    /// `uf explain build` prints it and so does the build's own summary, so
    /// the two quote one sentence — ubugeeei-prod/uf#166's rule applied to the
    /// question this plan answers, which is the one a reader asks when `dist/`
    /// does not hold what they expected.
    #[must_use]
    pub fn because(&self) -> String {
        match self.declared {
            true => String::from(
                "`app.router.enabled` is false, so this project is a library and `build.lib` \
                 says what it builds",
            ),
            false => String::from(
                "`app.router.enabled` is false, so this project is a library rather than an \
                 application",
            ),
        }
    }
}

/// Refuse a library configuration that has no build behind it.
///
/// Called while `uf.config.js` is loaded, beside [`crate::rendering::check`]
/// and for the same reason: a project that asked for something uf will not do
/// should find out at the file it wrote.
///
/// Three refusals. `build.lib` in a project whose router is on is the
/// contradiction — one key says "application", the other describes a library
/// build — and it is refused rather than resolved by precedence, because
/// whichever were read second would silently win. An empty entry list is a
/// library build with nothing to build. A format uf has not written is the
/// third, and it is refused by name rather than accepted and skipped: a
/// project that asked for `umd` and got a directory with no UMD file in it
/// would find out from a consumer.
pub(crate) fn check(path: &Utf8Path, config: &UniflowedConfig) -> Result<(), ConfigError> {
    let Some(lib) = &config.build.lib else {
        return Ok(());
    };
    if config.app.router.enabled {
        return Err(ConfigError::LibraryBuildInAnApplication {
            path: path.to_path_buf(),
        });
    }
    if lib.entries.is_empty() {
        return Err(ConfigError::LibraryWithoutEntries {
            path: path.to_path_buf(),
        });
    }
    let unwritten = lib
        .formats
        .iter()
        .filter(|format| !format.is_implemented())
        .map(|format| format.as_str())
        .collect::<Vec<_>>();
    if !unwritten.is_empty() {
        return Err(ConfigError::LibraryFormatNotImplemented {
            path: path.to_path_buf(),
            formats: unwritten.join(", "),
        });
    }
    if lib.formats.is_empty() {
        return Err(ConfigError::LibraryWithoutFormats {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
