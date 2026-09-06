//! When the `useX` naming convention means React.
//!
//! Two halves, and the smaller one is the fix: `useFakeTimers` in a module with
//! no React in it is not a hook. Everything else here is a way a module *does*
//! mention React, asserting that the same function is still held to the same
//! rules — because the failure this change could cause is the silent one. A
//! module misread as plain JavaScript loses `react/hooks-rules` and
//! `react/no-render-side-effects` entirely, and nothing in the output says so.

use uf_rsc::tokenize;

use super::{accepts, findings};
use crate::convention::{ReactSignal, react_signal};
use crate::rule::Finding;

/// `@uniflowed/test`'s fake timers, reduced to the shape that was reported.
///
/// A `useX` function that replaces a global and writes module state, under a
/// `prelude` that decides whether the module has anything to do with React.
/// `packages/test/internal/timers.js` is this module with more of it: the name
/// is the one Jest, Vitest and Sinon all use, the writes are the point of the
/// function, and there is no React in the file.
fn fake_timers(prelude: &str) -> String {
    format!(
        "{prelude}let installed = null;\n\
         let now = 0;\n\
         export function useFakeTimers(): void {{\n\
         \x20 installed = host().setTimeout;\n\
         \x20 now = Date.now();\n\
         }}\n"
    )
}

/// What the reduced module is reported for while it counts as React: both
/// writes to module state, and the clock read on the second of them.
const IN_RENDER: [Finding; 3] = [
    Finding::ModuleBindingAssigned,
    Finding::ModuleBindingAssigned,
    Finding::UnstableReadDuringRender,
];

#[test]
fn a_use_function_in_a_module_with_no_react_is_not_a_hook() {
    accepts(&fake_timers(""));
}

#[test]
fn a_react_import_keeps_a_use_function_checked() {
    assert_eq!(
        findings(&fake_timers("import { useState } from \"react\";\n")),
        IN_RENDER
    );
}

#[test]
fn a_uniflowed_react_import_keeps_a_use_function_checked() {
    assert_eq!(
        findings(&fake_timers(
            "import { useState } from \"@uniflowed/react\";\n"
        )),
        IN_RENDER
    );
}

#[test]
fn a_component_declaration_keeps_a_use_function_checked() {
    assert_eq!(
        findings(&fake_timers("component Page() {\n  return null;\n}\n")),
        IN_RENDER
    );
}

#[test]
fn a_hook_declaration_keeps_a_use_function_checked() {
    assert_eq!(
        findings(&fake_timers(
            "hook useSomething(): number {\n  return 1;\n}\n"
        )),
        IN_RENDER
    );
}

#[test]
fn jsx_keeps_a_use_function_checked() {
    // A uf module may hold JSX and import nothing at all: the automatic runtime
    // needs no `React` in scope. So JSX has to be a signal in its own right, or
    // every module written that way would quietly lose the rules.
    assert_eq!(
        findings(&fake_timers("function Icon() {\n  return <svg />;\n}\n")),
        IN_RENDER
    );
}

#[test]
fn a_call_to_another_use_function_keeps_a_use_function_checked() {
    // The signal that carries a real hook written without imports. A hook that
    // imports nothing from React is a hook *because it calls one*, and that
    // call is inside the function under test — which is where a hook's calls
    // are.
    assert_eq!(
        findings(
            "let now = 0;\nexport function useClock(): number {\n  useEffect(() => {});\n  now = Date.now();\n  return now;\n}\n"
        ),
        [
            Finding::ModuleBindingAssigned,
            Finding::UnstableReadDuringRender
        ]
    );
}

/// What a module says about React, for the tests that care which signal fired.
fn signal(source: &str) -> Option<ReactSignal> {
    react_signal(source, &tokenize(source))
}

#[test]
fn a_declaration_named_use_something_is_not_a_call() {
    // The whole of the difficulty: `function useFakeTimers()` and
    // `useFakeTimers()` differ only in the word in front of the name, and only
    // the second is evidence of anything.
    assert_eq!(signal("export function useFakeTimers(): void {}\n"), None);
    assert_eq!(signal("const useFakeTimers = (): void => {};\n"), None);
}

#[test]
fn a_member_call_named_use_something_is_not_a_hook_call() {
    // `jest.useFakeTimers()` and `vi.useFakeTimers()` are as common as they are
    // unrelated to React, and the walk has never counted a property read as a
    // hook call either.
    assert_eq!(signal("jest.useFakeTimers();\n"), None);
}

#[test]
fn a_module_the_walk_finds_a_hook_call_in_is_a_react_module() {
    // The property that keeps the fix from switching a rule off where it had
    // something to say. The walk reports a misplaced hook only where it has
    // found a call, and a call is itself a signal — so there is no module in
    // which `react/hooks-rules` is dormant *and* a hook is misplaced.
    let source = "function helper() {\n  return useState(0);\n}\n";
    assert_eq!(signal(source), Some(ReactSignal::HookCall));
    assert_eq!(findings(source), [Finding::HookOutsideComponent]);
}

#[test]
fn a_self_closing_tag_is_jsx() {
    assert_eq!(
        signal("export const icon = <svg />;\n"),
        Some(ReactSignal::Jsx)
    );
}

#[test]
fn a_closing_tag_the_lexer_read_as_a_regex_is_still_jsx() {
    // `<div><span></span></div>` gives the lexer a `/` in a position where a
    // regular expression may start — the token in front of it is `<` — and a
    // second `/` on the same line to end it, so `/span></div` lexes as one
    // regex token. Asking for the punctuation `/` found no JSX in a module that
    // is nothing but JSX; asking for a token that begins with `/` finds it.
    assert_eq!(
        signal("export const page = <div><span></span></div>;\n"),
        Some(ReactSignal::Jsx)
    );
}

#[test]
fn a_comparison_written_without_spaces_is_not_jsx() {
    assert_eq!(signal("export const ok = a<b/c>d;\n"), None);
}

#[test]
fn a_react_subpath_import_is_an_import() {
    assert_eq!(
        signal("import { createRoot } from \"react-dom/client\";\n"),
        Some(ReactSignal::Import)
    );
    assert_eq!(
        signal("const { renderHook } = require(\"@testing-library/react\");\n"),
        Some(ReactSignal::Import)
    );
}

#[test]
fn a_component_declaration_is_a_declaration() {
    assert_eq!(
        signal("component Page() {\n  return null;\n}\n"),
        Some(ReactSignal::Declaration)
    );
    assert_eq!(
        signal("hook useThing(): number {\n  return 1;\n}\n"),
        Some(ReactSignal::Declaration)
    );
}

#[test]
fn a_devtools_hook_written_to_at_the_start_of_a_line_is_not_a_declaration() {
    // React's own Fast Refresh runtime holds the DevTools global in a variable
    // called `hook`. It is not a declaration there, and the signal reads it
    // with the same predicate the walk does.
    assert_eq!(signal("hook.inject = renderer;\n"), None);
}
