//! What may not be read while a component renders.
//!
//! Everything here is reported only in *render position*: directly inside a
//! `component`, `hook` or `useX` body, blocks and JSX containers included, and
//! never inside a function nested in one.
//!
//! That boundary is deliberate, and it is where the honest limit of a
//! source-only validator sits. `useEffect(() => { document.title = t; })` and
//! `onClick={() => { document.title = t; }}` are both a closure inside a
//! component, and telling the first from the second means knowing what the
//! closure is passed to — the callee's type, not its spelling. So uf reports
//! the case it can decide, which is code that certainly runs during render, and
//! says nothing about closures rather than guessing at them.

use uf_rsc::CLIENT_ONLY_GLOBALS;

use crate::rule::Finding;

use super::Walk;

/// Reads whose value changes without React being told. Sorted for binary search.
const UNSTABLE_READS: &[(&str, &str)] = &[("Date", "now"), ("Math", "random")];

impl<'a> Walk<'a> {
    /// Check one identifier read that is not a call, a write, or a hook.
    pub(super) fn render_read(&mut self, index: usize, word: &'a str) {
        if !self.stack.in_render() {
            return;
        }
        let facts = self.bindings.get(word);

        if facts.ref_object {
            if self.punct(index + 1, b'.') && self.ident(index + 2) == Some("current") {
                self.report(index, Finding::RefReadDuringRender, Some(word));
            }
            return;
        }

        // A binding of the same name shadows the global, and `typeof window` is
        // a guard rather than a read: neither one touches the DOM.
        if facts.declared || self.previous == Some("typeof") {
            return;
        }

        if word == "console" && self.punct(index + 1, b'.') {
            self.report(index, Finding::ConsoleDuringRender, Some(word));
            return;
        }
        if CLIENT_ONLY_GLOBALS.binary_search(&word).is_ok() {
            self.report(index, Finding::DomAccessDuringRender, Some(word));
            return;
        }
        if let Ok(at) = UNSTABLE_READS.binary_search_by_key(&word, |entry| entry.0)
            && self.punct(index + 1, b'.')
            && self.ident(index + 2) == Some(UNSTABLE_READS[at].1)
            && self.punct(index + 3, b'(')
        {
            self.report(index, Finding::UnstableReadDuringRender, Some(word));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unstable_read_table_is_sorted_for_binary_search() {
        assert!(UNSTABLE_READS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn the_browser_global_table_is_sorted_for_binary_search() {
        assert!(CLIENT_ONLY_GLOBALS.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
