//! `fetch/no-global-override`: reassigning `globalThis.fetch` replaces the
//! instrumented client the rest of the toolchain relies on.
//!
//! *Reassigning*, which is what the rule is named after and was not what it
//! checked. It fired on the first mention of the name in code, so
//!
//! ```js
//! const doFetch = settings.fetch ?? globalThis.fetch;
//! ```
//!
//! was reported as an override — in `@uniflowed/fetch` itself, where reading
//! the global is the only way to have a default, and where the rule is most
//! obviously not about that.

use uf_config::UniflowedConfig;

use crate::scan::{FileScan, find_all, next_non_space, prev_non_space, starts_word};
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
        // The next line's code, for an assignment written across the break.
        let next = scan.lines.get(position + 1).map(|line| line.code());

        // Every occurrence, not the first: `const old = globalThis.fetch;
        // globalThis.fetch = polyfill;` keeps a handle and then installs one,
        // and stopping at the read would let the install through.
        for needle in ["globalThis.fetch", "window.fetch", "global.fetch"] {
            for at in find_all(code, needle) {
                // Not in a string: a sentence that names `globalThis.fetch`
                // overrides nothing, and this package's own tests are full of
                // such sentences.
                if line.in_string(at) || !names_the_global(code, at) {
                    continue;
                }
                if !assigns_to(code, at + needle.len(), next) {
                    continue;
                }
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
}

/// Whether the match at `at` is the global rather than a property of something
/// else.
///
/// `obj.globalThis.fetch = mine` assigns to a field of `obj` and overrides
/// nothing; a substring search cannot tell the two apart, and a rule that
/// reports the wrong one is a rule people turn off.
fn names_the_global(code: &str, at: usize) -> bool {
    starts_word(code, at) && prev_non_space(code, at).is_none_or(|(_, byte)| byte != b'.')
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
fn assigns_to(code: &str, at: usize, next: Option<&str>) -> bool {
    match next_non_space(code, at) {
        Some((start, _)) => is_assignment(&code[start..]),
        // Nothing after it on this line: either the operator is on the next
        // one, where the formatter puts it when the assignment is long, or the
        // statement ended and there is none.
        None => next
            .and_then(|next| next_non_space(next, 0).map(|(start, _)| &next[start..]))
            .is_some_and(is_assignment),
    }
}

/// Whether `rest` begins with an assignment operator.
fn is_assignment(rest: &str) -> bool {
    for operator in ["??=", "||=", "&&="] {
        if rest.starts_with(operator) {
            return true;
        }
    }
    rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>")
}
