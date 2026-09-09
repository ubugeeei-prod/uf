//! Which browser a browser run drives, and what it says when there is none.
//!
//! `uf test --browser` is the one mode whose dependency is not a JavaScript
//! runtime. Node and Bun are a package manager away and a project already has
//! one; a browser is two hundred megabytes of somebody else's release
//! engineering, and uf does not download it, does not vendor it, and does not
//! pin a version of it. It drives a browser that is already on the machine.
//!
//! That is the `docs/red-lines.md` position applied to a heavier dependency
//! than usual: **uf owns orchestration, not implementation**, and *every
//! built-in provider must be replaceable*. So this module is a lookup, not an
//! installer, and every part of it can be overruled by naming a binary:
//!
//! * `UF_BROWSER` is an exact path and is never second-guessed. It is checked
//!   for existence and used.
//! * Otherwise [`CANDIDATES`] is tried in order — the Chromium family, because
//!   the switches a headless run needs (`--headless=new`, `--user-data-dir`)
//!   are that family's, and a browser uf cannot start headless is a browser
//!   uf cannot use.
//! * Otherwise the run is **refused, by name**: the message lists every
//!   candidate that was looked for and says how to name one. A browser mode
//!   that quietly fell back to the DOM shim would report a green run of the
//!   exact tests it was asked to stop trusting.
//!
//! # Why the list is Chromium-shaped and says so
//!
//! Firefox and Safari can each run a page; neither can be told from a command
//! line to open one headlessly, keep it open, and take a clean profile — the
//! three things a run needs — without a driver protocol uf would then own.
//! Naming that limit here is better than a list that looks browser-agnostic
//! and fails on two thirds of it. `docs/hosts.md` carries the same row.

use camino::{Utf8Path, Utf8PathBuf};

/// The environment variable that names a browser outright.
pub const BROWSER_VARIABLE: &str = "UF_BROWSER";

/// Browsers looked for on `PATH`, in order.
///
/// Names rather than paths, because a name is what every platform's package
/// manager puts on `PATH`, and because a path list would be a table of
/// somebody's installer conventions that goes stale between releases. macOS
/// application bundles are the exception and are covered by [`BUNDLES`].
pub const CANDIDATES: [&str; 6] = [
    "chromium",
    "chrome",
    "google-chrome",
    "google-chrome-stable",
    "microsoft-edge",
    "brave-browser",
];

/// macOS application bundles, tried after `PATH`.
///
/// A Mac with Chrome installed usually has nothing on `PATH` at all: the
/// binary lives inside the bundle and nobody links it. Looking here is the
/// difference between "works on a developer's laptop" and a refusal that is
/// technically correct.
pub const BUNDLES: [&str; 3] = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
];

/// Where the browser a run will drive came from.
///
/// Carried into the report so `uf test --browser` can say which binary
/// answered. A run whose result depends on a browser and does not name it is a
/// run nobody can reproduce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browser {
    /// The executable to start.
    pub program: Utf8PathBuf,
    /// How it was found, for the run header.
    pub found: Found,
}

/// How [`find_browser`] arrived at a binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// Named outright by `UF_BROWSER`.
    Named,
    /// Found on `PATH` under one of [`CANDIDATES`].
    OnPath,
    /// Found at one of [`BUNDLES`].
    Bundle,
}

impl Browser {
    /// One line for the run header.
    #[must_use]
    pub fn describe(&self) -> String {
        match self.found {
            Found::Named => format!("{} (named)", self.program),
            Found::OnPath => format!("{} (on PATH)", self.program),
            Found::Bundle => format!("{} (installed)", self.program),
        }
    }
}

/// Find the browser a run should drive.
///
/// `named` is the exact binary the environment asked for; `on_path` is the
/// `PATH` lookup, and `exists` decides whether a path is a file — both injected
/// so the decision can be tested without a browser installed, which is most
/// machines that run this crate's tests.
///
/// # Errors
///
/// [`NoBrowser`] when nothing answered, carrying everything that was tried so
/// the message can name it.
pub fn find_browser(
    named: Option<&str>,
    on_path: &dyn Fn(&str) -> Option<Utf8PathBuf>,
    exists: &dyn Fn(&Utf8Path) -> bool,
) -> Result<Browser, NoBrowser> {
    if let Some(named) = named.filter(|named| !named.trim().is_empty()) {
        let named = Utf8Path::new(named.trim());
        // A named binary is checked and never silently replaced. Falling back
        // to a discovered browser when the named one is missing would run the
        // suite on a browser nobody chose and report it under the name that was
        // asked for, which is the defect `uf test`'s own binary resolution
        // exists to avoid one layer down.
        if exists(named) {
            return Ok(Browser {
                program: named.to_path_buf(),
                found: Found::Named,
            });
        }
        return Err(NoBrowser {
            named: Some(named.to_string()),
        });
    }
    for candidate in CANDIDATES {
        if let Some(program) = on_path(candidate) {
            return Ok(Browser {
                program,
                found: Found::OnPath,
            });
        }
    }
    for bundle in BUNDLES {
        let bundle = Utf8Path::new(bundle);
        if exists(bundle) {
            return Ok(Browser {
                program: bundle.to_path_buf(),
                found: Found::Bundle,
            });
        }
    }
    Err(NoBrowser { named: None })
}

/// No browser was found, and the run has to stop.
///
/// A refusal rather than a fallback. `uf test --browser` was asked for because
/// the DOM shim's answer was not trusted; running on the shim anyway and
/// reporting a pass would answer a question nobody asked, in the one place
/// where a wrong green is most expensive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoBrowser {
    /// The binary that was named and is not there, when one was.
    pub named: Option<String>,
}

impl std::fmt::Display for NoBrowser {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(named) = &self.named {
            return write!(
                formatter,
                "`uf test --browser` was told to use `{named}`, and there is no such file. \
                 uf does not download a browser and will not quietly pick a different one: \
                 correct `{BROWSER_VARIABLE}`, or unset it to look for an installed browser."
            );
        }
        write!(
            formatter,
            "`uf test --browser` needs a browser on this machine, and found none. uf drives a \
             browser you already have rather than downloading one — it looked on PATH for {}, \
             and for an installed {}. Install one, or set `{BROWSER_VARIABLE}` to the binary \
             to drive. Without `--browser` the suite runs on Node's DOM shim, which is what it \
             was already doing.",
            quoted(&CANDIDATES),
            quoted(&["Google Chrome", "Chromium", "Microsoft Edge"]),
        )
    }
}

impl std::error::Error for NoBrowser {}

/// `a`, `b` and `c`, quoted, for a message.
fn quoted(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nothing_on_path(_: &str) -> Option<Utf8PathBuf> {
        None
    }

    fn nothing_exists(_: &Utf8Path) -> bool {
        false
    }

    #[test]
    fn a_named_browser_wins_over_everything_installed() {
        let found = find_browser(
            Some("/opt/browsers/chrome"),
            &|_| Some(Utf8PathBuf::from("/usr/bin/chromium")),
            &|path| path == "/opt/browsers/chrome",
        )
        .expect("a named binary that exists is the answer");

        assert_eq!(found.program, "/opt/browsers/chrome");
        assert_eq!(found.found, Found::Named);
    }

    #[test]
    fn a_named_browser_that_is_missing_is_a_refusal_rather_than_a_fallback() {
        // The important half. Falling back would run the suite on a browser
        // nobody chose while the report carried the name that was asked for.
        let error = find_browser(
            Some("/opt/browsers/chrome"),
            &|_| Some(Utf8PathBuf::from("/usr/bin/chromium")),
            &nothing_exists,
        )
        .expect_err("a named binary that is not there stops the run");

        assert_eq!(error.named.as_deref(), Some("/opt/browsers/chrome"));
        let message = error.to_string();
        assert!(message.contains("/opt/browsers/chrome"), "{message}");
        assert!(message.contains(BROWSER_VARIABLE), "{message}");
    }

    #[test]
    fn an_empty_name_is_not_a_name() {
        // `UF_BROWSER=` exported by a shell means the variable is unset, not
        // that the browser is called "".
        let found = find_browser(
            Some("  "),
            &|name| (name == "chromium").then(|| Utf8PathBuf::from("/usr/bin/chromium")),
            &nothing_exists,
        )
        .expect("an empty name falls through to discovery");

        assert_eq!(found.found, Found::OnPath);
    }

    #[test]
    fn path_is_searched_in_order_and_beats_an_installed_bundle() {
        let found = find_browser(
            None,
            &|name| (name == "google-chrome").then(|| Utf8PathBuf::from("/usr/bin/google-chrome")),
            &|_| true,
        )
        .expect("a browser on PATH answers");

        assert_eq!(found.program, "/usr/bin/google-chrome");
        assert_eq!(found.found, Found::OnPath);
    }

    #[test]
    fn a_mac_bundle_answers_when_nothing_is_on_path() {
        // The ordinary shape of a developer's Mac: Chrome installed, nothing
        // linked onto PATH.
        let found = find_browser(None, &nothing_on_path, &|path| path.as_str() == BUNDLES[0])
            .expect("an installed bundle answers");

        assert_eq!(found.program, BUNDLES[0]);
        assert_eq!(found.found, Found::Bundle);
    }

    #[test]
    fn no_browser_at_all_names_everything_it_looked_for() {
        let error = find_browser(None, &nothing_on_path, &nothing_exists)
            .expect_err("a machine with no browser cannot run a browser suite");

        assert_eq!(error.named, None);
        let message = error.to_string();
        for candidate in CANDIDATES {
            assert!(
                message.contains(candidate),
                "{candidate} missing from {message}"
            );
        }
        assert!(message.contains(BROWSER_VARIABLE), "{message}");
        // And it says what the run would otherwise have been, because a person
        // reading this asked for a browser precisely because they stopped
        // believing the shim.
        assert!(message.contains("DOM shim"), "{message}");
    }

    #[test]
    fn a_browser_says_where_it_came_from() {
        for (found, expected) in [
            (Found::Named, "(named)"),
            (Found::OnPath, "(on PATH)"),
            (Found::Bundle, "(installed)"),
        ] {
            let browser = Browser {
                program: Utf8PathBuf::from("/usr/bin/chromium"),
                found,
            };
            let described = browser.describe();
            assert!(described.starts_with("/usr/bin/chromium"), "{described}");
            assert!(described.ends_with(expected), "{described}");
        }
    }

    #[test]
    fn a_list_of_names_reads_as_a_sentence() {
        assert_eq!(quoted(&[]), "");
        assert_eq!(quoted(&["a"]), "`a`");
        assert_eq!(quoted(&["a", "b"]), "`a` or `b`");
        assert_eq!(quoted(&["a", "b", "c"]), "`a`, `b` or `c`");
    }
}
