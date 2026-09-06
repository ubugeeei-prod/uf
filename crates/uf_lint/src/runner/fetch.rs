//! `fetch/no-global-override`: reassigning `globalThis.fetch` replaces the
//! instrumented client the rest of the toolchain relies on.
//!
//! *Reassigning*, which is what the rule is named after and was not what it
//! checked. It fired on any mention of the name in code, so
//!
//! ```js
//! const doFetch = settings.fetch ?? globalThis.fetch;
//! ```
//!
//! was reported as an override — in `@uniflowed/fetch` itself, where reading
//! the global is the only way to have a default, and where the rule is most
//! obviously not about that.

use uf_config::UniflowedConfig;

use crate::scan::{FileScan, next_non_space};
use crate::{Diagnostic, push_in_code, severity};

pub(crate) fn run_fetch_no_global_override(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "fetch/no-global-override") else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        // Not in a string: a sentence that names `globalThis.fetch` overrides
        // nothing, and this package's own tests are full of such sentences.
        let at = ["globalThis.fetch", "window.fetch", "global.fetch"]
            .into_iter()
            .filter_map(|needle| code.find(needle).map(|at| (at, needle.len())))
            .find(|&(at, len)| !line.in_string(at) && assigns_to(code, at + len));
        if let Some((at, _)) = at {
            push_in_code(
                diagnostics,
                scan,
                "fetch/no-global-override",
                severity,
                position,
                at,
                "do not override global fetch; use @uniflowed/fetch explicit clients",
            );
        }
    }
}

/// Whether what follows `at` assigns to the name that ends there.
///
/// `=` but not `==` or `===`, and the logical assignments — `??=`, `||=`,
/// `&&=` — because `globalThis.fetch ??= mine` installs one just as surely.
/// Everything else is a read: a default, a capability check, an argument.
///
/// It does not see `Object.defineProperty(globalThis, "fetch", …)`, which
/// overrides the global too and always did go unreported. That is a different
/// shape and wants its own test; catching a read was making the rule *noisier*
/// rather than more complete, which is the opposite trade.
fn assigns_to(code: &str, at: usize) -> bool {
    let Some((start, _)) = next_non_space(code, at) else {
        return false;
    };
    let rest = &code[start..];
    for operator in ["??=", "||=", "&&="] {
        if rest.starts_with(operator) {
            return true;
        }
    }
    rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>")
}
