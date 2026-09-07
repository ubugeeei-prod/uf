//! What a `uf.config.js` says a build may produce, as one answer.
//!
//! Two settings decide it and they were read by nothing until
//! ubugeeei-prod/uf#336 and ubugeeei-prod/uf#385:
//!
//! * `app.rendering.modes` — an allowlist of rendering strategies. A project
//!   that says `["ssg"]` has said it deploys to a static host; one that says
//!   `["ssr"]` has said it deploys a server and wants no prerendered
//!   documents; the default says both are acceptable and lets the build decide
//!   per route.
//! * `build.staticBuild` — documented as "prerender everything and emit no
//!   server bundle", which is the same claim made about the artefact rather
//!   than about the routes.
//!
//! Reading them separately at three call sites is how they would come apart,
//! so they are resolved once, here, into a [`RenderingPlan`] that answers the
//! only two questions the rest of the toolchain asks: **what gets prerendered**
//! and **is there a server**.
//!
//! # Why a plan and not two booleans
//!
//! Because the interesting case is the contradiction. `staticBuild: true` with
//! `modes: ["ssr"]` is a project that has asked for a build with no server and
//! for routes that need one; there is no behaviour that honours both, and the
//! guide's rule — "reject unsupported configurations clearly rather than
//! silently changing semantics" — says the answer is an error at the config
//! file rather than whichever of the two happened to be read last. Resolving
//! them together is what makes that check possible to write at all.

use camino::Utf8Path;

use crate::{ConfigError, RenderingMode, UniflowedConfig};

/// How much of the route table `uf build` prerenders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prerender {
    /// Every route, and a route that cannot be prerendered is an error naming
    /// it.
    ///
    /// What a project deploying to a static host asked for: there is no
    /// process to render the rest, so a route left out is a URL that 404s
    /// after the deploy and nowhere before it.
    Everything,
    /// Every route that can be, and the rest are rendered per request.
    ///
    /// The default, and what `uf build` did before any of this was read.
    Possible,
    /// No route at all: the server renders every request.
    ///
    /// `modes: ["ssr"]` and nothing else. Before ubugeeei-prod/uf#336 this
    /// setting was accepted and silently meant [`Self::Possible`], which is
    /// the one behaviour the list could not select.
    Nothing,
}

impl Prerender {
    /// The spelling passed to the builder as `--prerender`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Everything => "everything",
            Self::Possible => "possible",
            Self::Nothing => "nothing",
        }
    }
}

/// Which setting decided a plan, so a refusal can quote the line that caused
/// it.
///
/// A message that says "this build prerenders everything" and does not say
/// *why* leaves the reader looking for a flag they did not pass. Both spellings
/// are legitimate and they are not the same declaration, so the plan carries
/// which one it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanSource {
    /// `build.staticBuild: true`.
    StaticBuild,
    /// `app.rendering.modes`, narrowed by the project.
    Modes,
    /// Neither: the defaults, which allow everything.
    Default,
}

impl PlanSource {
    /// The config key to quote, or `None` when nothing was narrowed.
    #[must_use]
    pub const fn key(self) -> Option<&'static str> {
        match self {
            Self::StaticBuild => Some("build.staticBuild"),
            Self::Modes => Some("app.rendering.modes"),
            Self::Default => None,
        }
    }
}

/// What `uf build` may produce for one project.
///
/// Resolved from a validated config, so it is infallible: every combination
/// that has no honest answer is refused by [`check`] while the config file is
/// being loaded, and what reaches here is a configuration uf can build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderingPlan {
    prerender: Prerender,
    server: bool,
    source: PlanSource,
}

impl RenderingPlan {
    /// The plan `config` describes.
    #[must_use]
    pub fn resolve(config: &UniflowedConfig) -> Self {
        let modes = &config.app.rendering.modes;
        let per_request = modes.contains(&RenderingMode::Ssr);
        let prerendered = modes.contains(&RenderingMode::Ssg);

        // `staticBuild` first, because it is the stronger claim: it is about
        // the artefact rather than about the routes, and a project that has
        // said it emits no server has said the routes have nowhere to run
        // whatever else the allowlist permits. The two cannot disagree here —
        // `check` has already refused `staticBuild` alongside a `modes` that
        // does not allow `ssg` — so this is a precedence rule for a case that
        // is already consistent, not a tie-breaker for one that is not.
        if config.build.static_build {
            return Self {
                prerender: Prerender::Everything,
                server: false,
                source: PlanSource::StaticBuild,
            };
        }
        match (prerendered, per_request) {
            (true, true) => Self {
                prerender: Prerender::Possible,
                server: true,
                // Not `Modes` even when the project wrote the list out: this
                // is the behaviour of a project that narrowed nothing, and a
                // message quoting a key that changed no decision sends the
                // reader to a line that is not the problem.
                source: PlanSource::Default,
            },
            (true, false) => Self {
                prerender: Prerender::Everything,
                // The server bundle is still written. `modes: ["ssg"]` says
                // which routes may exist, and `build.staticBuild` says what is
                // emitted; they are different declarations, and a project that
                // made only the first one can still check its build with
                // `uf start`. Every document it serves is one the build wrote.
                server: true,
                source: PlanSource::Modes,
            },
            (false, true) => Self {
                prerender: Prerender::Nothing,
                server: true,
                source: PlanSource::Modes,
            },
            // Refused by `check`, which runs before any of this. Kept total
            // rather than `unreachable!()`: the fallback that cannot happen is
            // the one every project already had.
            (false, false) => Self {
                prerender: Prerender::Possible,
                server: true,
                source: PlanSource::Default,
            },
        }
    }

    /// How much of the route table is prerendered.
    #[must_use]
    pub const fn prerender(self) -> Prerender {
        self.prerender
    }

    /// Whether the build emits something that can answer a request.
    ///
    /// `false` only for `build.staticBuild`. The server bundle is still built
    /// as an intermediate — the prerender renders *through* it — and then
    /// removed, because the declaration is about what the build leaves behind.
    #[must_use]
    pub const fn emits_a_server(self) -> bool {
        self.server
    }

    /// Whether a route that can only be rendered per request is an error.
    #[must_use]
    pub const fn is_static_only(self) -> bool {
        matches!(self.prerender, Prerender::Everything)
    }

    /// Which declaration produced this plan.
    #[must_use]
    pub const fn source(self) -> PlanSource {
        self.source
    }

    /// The clause a refusal starts with: the setting, and what it asked for.
    ///
    /// One sentence rather than a formatted paragraph, so a caller in the CLI
    /// and a caller in the builder driver quote the same words for the same
    /// configuration.
    #[must_use]
    pub fn because(self) -> String {
        match self.source {
            PlanSource::StaticBuild => String::from(
                "`build.staticBuild` is true, so this build prerenders every route and emits \
                 no server",
            ),
            PlanSource::Modes => match self.prerender {
                Prerender::Nothing => String::from(
                    "`app.rendering.modes` does not allow `ssg`, so this build prerenders nothing",
                ),
                _ => String::from(
                    "`app.rendering.modes` does not allow `ssr`, so every route has to be \
                     prerendered",
                ),
            },
            PlanSource::Default => String::from("this build prerenders every route it can"),
        }
    }
}

/// Refuse a rendering configuration that has no build behind it.
///
/// Called while `uf.config.js` is loaded, beside the cache switches and for
/// the same reason: a project that asked for something uf will not do should
/// find out at the file it wrote rather than in the deployment where it was
/// not honoured.
///
/// Two refusals, and neither is "you named a mode uf has not written".
/// `modes` is an allowlist, so `["ssg", "isr"]` permits a strategy that never
/// gets selected, which changes nothing and is worth no error. What is refused
/// is a list that leaves the build with nothing it can do, and a `staticBuild`
/// that the same file's `modes` forbids.
pub(crate) fn check(path: &Utf8Path, config: &UniflowedConfig) -> Result<(), ConfigError> {
    let modes = &config.app.rendering.modes;
    if !modes.iter().any(|mode| mode.is_implemented()) {
        return Err(ConfigError::NoImplementedRenderingMode {
            path: path.to_path_buf(),
            modes: modes
                .iter()
                .map(|mode| mode.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        });
    }
    if config.build.static_build && !modes.contains(&RenderingMode::Ssg) {
        return Err(ConfigError::StaticBuildWithoutSsg {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
