//! The `_uf.*` reserved file-name grammar.
//!
//! `uf` reserves the `_uf.` prefix inside the router root so a project cannot
//! accidentally shadow a framework file. A reserved name is
//! `_uf.<role>[.<variant>].js`, where the role is what the file does for the
//! router and the variant narrows which build it applies to.
//!
//! This is the single source of truth for that grammar. `uf create` generates
//! these names, `discover_routes` looks for them, and `uf lint`'s
//! `router/reserved-files` rule rejects the ones that do not fit — so all three
//! read it from here rather than each spelling out the same `matches!`.

use std::str::FromStr;

/// What a reserved file does for the router.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReservedRole {
    /// Wraps a route subtree.
    Layout,
    /// Renders a route.
    Page,
    /// Runs before a route resolves.
    Middleware,
    /// Answers a request instead of rendering a page.
    Route,
}

impl ReservedRole {
    /// The role segment as it appears in a file name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Layout => "layout",
            Self::Page => "page",
            Self::Middleware => "middleware",
            Self::Route => "route",
        }
    }

    /// Every role, in declaration order.
    #[must_use]
    pub const fn all() -> [Self; 3] {
        [Self::Layout, Self::Page, Self::Middleware]
    }
}

impl FromStr for ReservedRole {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "layout" => Ok(Self::Layout),
            "page" => Ok(Self::Page),
            "middleware" => Ok(Self::Middleware),
            "route" => Ok(Self::Route),
            _ => Err(()),
        }
    }
}

/// Which build a reserved file applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReservedVariant {
    /// Applies to every target. This is the file the router resolves.
    Default,
    /// Applies to React Native on every platform.
    Native,
    /// Applies to iOS only.
    Ios,
    /// Applies to Android only.
    Android,
    /// Applies to the web target only.
    Web,
    /// A test colocated with the route it covers.
    Test,
}

impl ReservedVariant {
    /// The variant segment as it appears in a file name, or [`None`] for the
    /// default variant, which has no segment.
    #[must_use]
    pub const fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Native => Some("native"),
            Self::Ios => Some("ios"),
            Self::Android => Some("android"),
            Self::Web => Some("web"),
            Self::Test => Some("test"),
        }
    }

    /// Whether this variant is the one the router resolves as the route itself.
    ///
    /// Platform variants and colocated tests are companions to a route, never
    /// routes of their own, which is why `discover_routes` only matches the
    /// default variant.
    #[must_use]
    pub const fn is_route_entry(self) -> bool {
        matches!(self, Self::Default)
    }

    /// Every variant, in declaration order.
    #[must_use]
    pub const fn all() -> [Self; 6] {
        [
            Self::Default,
            Self::Native,
            Self::Ios,
            Self::Android,
            Self::Web,
            Self::Test,
        ]
    }
}

impl FromStr for ReservedVariant {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "native" => Ok(Self::Native),
            "ios" => Ok(Self::Ios),
            "android" => Ok(Self::Android),
            "web" => Ok(Self::Web),
            "test" => Ok(Self::Test),
            _ => Err(()),
        }
    }
}

/// A recognized `_uf.*` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReservedFile {
    /// What the file does.
    pub role: ReservedRole,
    /// Which build it applies to.
    pub variant: ReservedVariant,
}

impl ReservedFile {
    /// Render the file name this describes.
    #[must_use]
    pub fn file_name(self) -> String {
        match self.variant.as_str() {
            Some(variant) => format!("_uf.{}.{variant}.js", self.role.as_str()),
            None => format!("_uf.{}.js", self.role.as_str()),
        }
    }
}

/// How a file name relates to the reserved grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedName {
    /// The name does not use the `_uf.` prefix, so the project owns it.
    NotReserved,
    /// A name `uf` defines.
    Recognized(ReservedFile),
    /// Uses the reserved prefix but is not a name `uf` defines.
    Unknown,
}

impl ReservedName {
    /// The recognized file, if the name is one.
    #[must_use]
    pub const fn recognized(self) -> Option<ReservedFile> {
        match self {
            Self::Recognized(file) => Some(file),
            _ => None,
        }
    }

    /// Whether the name claims the reserved prefix without being a name `uf`
    /// defines. This is what `router/reserved-files` reports.
    #[must_use]
    pub const fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Classify a bare file name against the reserved grammar.
///
/// Takes a file name, not a path: callers already have one, and accepting a
/// path here would invite a caller to pass an unnormalized one and get a
/// different answer than the router did.
#[must_use]
pub fn classify_reserved_file(file_name: &str) -> ReservedName {
    let Some(rest) = file_name.strip_prefix("_uf.") else {
        return ReservedName::NotReserved;
    };
    let Some(rest) = rest.strip_suffix(".js") else {
        return ReservedName::Unknown;
    };

    let mut segments = rest.split('.');
    let Some(Ok(role)) = segments.next().map(ReservedRole::from_str) else {
        return ReservedName::Unknown;
    };
    let variant = match segments.next() {
        None => ReservedVariant::Default,
        Some(segment) => match ReservedVariant::from_str(segment) {
            Ok(variant) => variant,
            Err(()) => return ReservedName::Unknown,
        },
    };
    if segments.next().is_some() {
        // `_uf.page.native.test.js` and friends: one variant, not a stack of them.
        return ReservedName::Unknown;
    }

    ReservedName::Recognized(ReservedFile { role, variant })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recognized(file_name: &str) -> ReservedFile {
        classify_reserved_file(file_name)
            .recognized()
            .unwrap_or_else(|| panic!("{file_name} should be recognized"))
    }

    #[test]
    fn plain_project_files_are_not_reserved() {
        for name in [
            "page.js",
            "Counter.js",
            "index.js",
            "_private.js",
            "uf.config.js",
            "_ufo.page.js",
        ] {
            assert_eq!(
                classify_reserved_file(name),
                ReservedName::NotReserved,
                "{name}"
            );
        }
    }

    #[test]
    fn the_three_router_roles_are_recognized() {
        assert_eq!(
            recognized("_uf.layout.js"),
            ReservedFile {
                role: ReservedRole::Layout,
                variant: ReservedVariant::Default
            }
        );
        assert_eq!(
            recognized("_uf.page.js"),
            ReservedFile {
                role: ReservedRole::Page,
                variant: ReservedVariant::Default
            }
        );
        assert_eq!(
            recognized("_uf.middleware.js"),
            ReservedFile {
                role: ReservedRole::Middleware,
                variant: ReservedVariant::Default
            }
        );
    }

    #[test]
    fn platform_variants_are_recognized() {
        for (name, variant) in [
            ("_uf.page.native.js", ReservedVariant::Native),
            ("_uf.page.ios.js", ReservedVariant::Ios),
            ("_uf.page.android.js", ReservedVariant::Android),
            ("_uf.page.web.js", ReservedVariant::Web),
        ] {
            assert_eq!(recognized(name).variant, variant, "{name}");
            assert_eq!(recognized(name).role, ReservedRole::Page, "{name}");
        }
    }

    #[test]
    fn colocated_tests_are_recognized() {
        assert_eq!(
            recognized("_uf.page.test.js").variant,
            ReservedVariant::Test
        );
        assert_eq!(
            recognized("_uf.layout.test.js").variant,
            ReservedVariant::Test
        );
    }

    #[test]
    fn every_role_and_variant_pair_round_trips_through_its_file_name() {
        for role in ReservedRole::all() {
            for variant in ReservedVariant::all() {
                let file = ReservedFile { role, variant };
                assert_eq!(recognized(&file.file_name()), file);
            }
        }
    }

    #[test]
    fn only_the_default_variant_is_a_route_entry() {
        assert!(ReservedVariant::Default.is_route_entry());
        for variant in ReservedVariant::all()
            .into_iter()
            .filter(|variant| *variant != ReservedVariant::Default)
        {
            assert!(!variant.is_route_entry(), "{variant:?}");
        }
    }

    #[test]
    fn an_unknown_role_is_rejected() {
        for name in [
            "_uf.handler.js",
            "_uf.loader.js",
            "_uf.js",
            "_uf.page",
            "_uf.PAGE.js",
        ] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn an_unknown_variant_is_rejected() {
        for name in [
            "_uf.page.server.js",
            "_uf.page.windows.js",
            "_uf.page.NATIVE.js",
        ] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn variants_do_not_stack() {
        assert!(classify_reserved_file("_uf.page.native.test.js").is_unknown());
        assert!(classify_reserved_file("_uf.page.ios.android.js").is_unknown());
    }

    #[test]
    fn a_reserved_prefix_without_a_js_extension_is_rejected() {
        for name in [
            "_uf.page.ts",
            "_uf.page.jsx",
            "_uf.page.js.flow",
            "_uf.page",
        ] {
            assert!(
                classify_reserved_file(name).is_unknown(),
                "{name} should be unknown"
            );
        }
    }

    #[test]
    fn classification_does_not_panic_on_odd_input() {
        for name in ["", "_uf.", "_uf..js", "_uf....js", "_uf.\u{1f600}.js"] {
            let _ = classify_reserved_file(name);
        }
    }
}
