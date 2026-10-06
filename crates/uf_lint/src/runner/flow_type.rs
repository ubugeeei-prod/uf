//! Flow built-ins that judge a type annotation: types too vague to check, types
//! Flow has renamed, types that belong to Flow's own internals, and object types
//! whose exactness the author never said out loud.

use uf_config::UniflowedConfig;
use uf_profiler::profile_span;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{
    FileScan, find_words, identifier_len, is_word_byte, next_non_space, prev_non_space,
    previous_word, starts_word, word_in_jsx_text,
};
use crate::{Diagnostic, Severity, push_at, push_in_code, severity};

/// Types Flow's `unclear-type` lint rejects, with the advice for each.
const UNCLEAR_TYPES: [(&str, &str); 3] = [
    (
        "any",
        "avoid `any`; use `mixed`, opaque types, or generated router/action types",
    ),
    ("Object", "avoid `Object`; describe the object's shape"),
    ("Function", "avoid `Function`; describe the call signature"),
];

pub(crate) fn run_flow_unclear_type(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    profile_span!("run_flow_unclear_type");
    let rule = FlowBuiltinLint::UnclearType.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    let mut enclosing = Enclosing::default();
    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        // What the *previous* lines left open, so a word on a continuation line
        // is judged by the call it stands in rather than by the fragment it
        // shares a line with.
        let outer = enclosing;
        enclosing = enclosing.after(code);
        for (needle, message) in UNCLEAR_TYPES {
            for at in find_words(code, needle) {
                // A sentence is not an annotation: `it("treats Object as any
                // non-null object", …)` names no type.
                if line.in_string(at) || word_in_jsx_text(scan, position, at, needle.len()) {
                    continue;
                }
                if names_a_value(code, at, needle.len(), outer) {
                    continue;
                }
                push_in_code(diagnostics, scan, rule, severity, position, at, message);
            }
        }
    }
}

/// Whether the word at `at` is an expression rather than a type annotation.
///
/// None of the three names this rule looks for is reserved. `Object` and
/// `Function` are global constructors, and all three are legal property names,
/// so the same word is a type on one line and an ordinary value on the next:
///
/// ```js
/// type Handler = Function;          // a type
/// case Function:                    // the constructor, matched against
/// expect.any(Function)              // the constructor, passed
/// obj.constructor === Object        // the constructor, compared
/// { any: asymmetric.any }           // a property called `any`
/// ```
///
/// Flow's own lint walks an AST and never has to ask. This one reads source
/// text — which is what `RuleRequirement::SourceText` records — so it asks the
/// only question source text answers: what stands either side of the word.
/// Each arm below is a shape a *type* cannot have, and every one of them was a
/// finding this rule reported against code that was already right:
/// `@uniflowed/test`'s `expect.any` is Jest's, Vitest's and Sinon's name for
/// the matcher, and the module that implements it has to both name the
/// property and switch on the constructor.
///
/// # What it still cannot see
///
/// A `(` that follows a name opens a call's argument list, a declaration's
/// parameter list or an `if`'s condition, and all three hold expressions. The
/// one place that is not true is `declare function f(Object): void`, where a
/// bare name in a libdef's parameter list *is* the parameter's type — so an
/// `Object` written that way is no longer reported. uf emits no libdefs and
/// `flow/syntax` is the only rule that reads `.flow` sidecars at all, so the
/// shape does not occur here; it is the price of telling `expect.any(Object)`
/// from `(Object) => void` without a parser, and it is stated rather than
/// discovered.
fn names_a_value(code: &str, at: usize, len: usize, outer: Enclosing) -> bool {
    let before = prev_non_space(code, at);
    let after = next_non_space(code, at + len);

    // `any < limit`, `count > any`, `foo(any + 1)`, and the letters inside
    // `<p>any</p>` are expressions. A type annotation is none of those.
    if beside_a_value_operator(code, before, after)
        || value_keyword_operand(code, at, len)
        || names_an_export_default(code, at)
        || extends_a_class(code, at)
        || names_a_declaration(code, at)
    {
        return true;
    }

    // `x.any` reads a property; `Object.keys(x)` and `new Function(src)` reach
    // for the global. A type is never on either side of a `.`, and never called.
    if before.is_some_and(|(_, byte)| byte == b'.') {
        return true;
    }
    if after.is_some_and(|(_, byte)| byte == b'.' || byte == b'(') {
        return true;
    }
    // `any`x`` tags a template. A type is never a tag.
    if after.is_some_and(|(_, byte)| byte == b'`') {
        return true;
    }

    // `case Object:` — Flow has no syntax that puts a type after `case`.
    if previous_word(code, at).is_some_and(|(_, word)| word == "case") {
        return true;
    }

    // `obj.constructor === Object` — the operand of an equality test. The lone
    // `=` is deliberately not one of these: `type Handler = Function` is the
    // shape this rule exists for.
    if follows_an_equality_operator(code, at) {
        return true;
    }

    // `{ any: … }` — a property key, which is the one place a *type* named in
    // an object type is not what the colon introduces. The opener has to be
    // named rather than assumed from the colon alone, because a conditional
    // type's `? any : never` puts a real annotation in front of one too, and it
    // arrives with a `?` in front instead.
    //
    // What may stand between the opener and the key is the whole reason this is
    // a list rather than one byte. `readonly any: …` and `+any: …` are the same
    // property with its variance written down, and reading the `y` of `readonly`
    // as "not an opener" reported Jest's `expect.any` matcher *name* as a type —
    // which cost `@uniflowed/test` a suppression it should never have needed
    // (ubugeeei-prod/uf#571). `any?: …` is the same key, optional.
    if names_a_property_key(code, at, len, before) {
        return true;
    }

    // `function test(any: string)` and `function test(any?: string)` — the name
    // before the colon is a binding. `(node: any)` still has the type after it.
    // `function test(any?)` is the same binding with no annotation after it.
    if names_a_parameter(code, at, len) || names_an_optional_parameter(code, at, len) {
        return true;
    }

    // `return any`, and `const any` / `let any` / `var any`.
    if introduced_as_a_value(code, at) {
        return true;
    }

    // `any;` and `class C { any; }` name a value. `type Slot = any` does not,
    // and neither does that alias when `=` is the last token of the line above.
    if names_a_bare_statement(code, at, len, outer) {
        return true;
    }

    // `const value = any`, `let ctor = Object`, and `ctor = Function`.
    // `type Handler = Function` stays a type, and so does `type Box<T = any>`
    // and a default continued onto the next line.
    if assignment_is_a_value(code, at, outer) {
        return true;
    }

    // `import { bool }` and `import { any }` bind a value. `import type { bool }`
    // and `import { type bool }` are types, and stay reported.
    if names_an_imported_value(code, at, outer) || names_a_default_import(code, at) {
        return true;
    }

    // `const { any } = obj` and `const x = { any }` name a value. `export type
    // { React$Node }` stays a type.
    if names_a_shorthand_binding(code, at, len) {
        return true;
    }

    // `const { a: any } = obj` renames a binding. `type T = { a: any }` does not.
    if names_a_renamed_binding(code, at) {
        return true;
    }

    // `const x = { a: any }` passes a value. `type T = { a: any }` does not.
    if names_an_object_value(code, at, outer) {
        return true;
    }

    // `const [any] = xs` and `function f([any])` bind a value.
    // `type T = [React$Node]` stays a type.
    if names_an_array_binding(code, at) {
        return true;
    }

    // `enum E { any }` names a member. `enum E of React$Node` names a type.
    if names_an_enum_member(code, at) {
        return true;
    }

    // `for (any of items)` binds a value. The name after `of` stays a type.
    if names_a_for_of_binding(code, at, len) {
        return true;
    }

    // `function f(value = Object)` passes a value. `type Box<T = any>` does not.
    if names_a_parameter_default(code, at, outer) {
        return true;
    }

    // `type Box<any>` and `function f<any>()` name a type parameter.
    // `Box<any>` and `f<any>(1)` still name a type.
    if names_a_type_parameter(code, at) {
        return true;
    }

    // `class C { #any; }` names a private field. `#` is not part of the word.
    // `class C { #x: any; }` still names a type.
    if names_a_private_name(code, at) {
        return true;
    }

    // `class C { @any method() {} }` names a decorator. `@` is not part of the
    // word. `class C { @dec x: any }` still names a type.
    if names_a_decorator(code, at) {
        return true;
    }

    // `for (any; i < n; i++)` names the initializer. The name after `of` stays
    // a type.
    if names_a_for_initializer(code, at, len) {
        return true;
    }

    // `for (;; any)` and `for (let i = 0; i < n; any)` name the update.
    if names_a_for_update(code, at, len) {
        return true;
    }

    // `expect.any(Function)` — the whole of an argument, in a list that is
    // being called rather than one that describes a function type.
    is_a_bare_argument(code, at, len, outer)
}

/// Whether the word is a default or namespace import binding.
///
/// `import any from`, `import any, { extra } from`, `import * as any`, and
/// `export * as any` bind a value. `import type any from` is a type: `type`
/// sits between `import` and the name.
fn names_a_default_import(code: &str, at: usize) -> bool {
    if previous_word(code, at).is_some_and(|(_, word)| word == "import") {
        return true;
    }
    let Some((as_at, "as")) = previous_word(code, at) else {
        return false;
    };
    let Some((star_at, b'*')) = prev_non_space(code, as_at) else {
        return false;
    };
    matches!(
        previous_word(code, star_at).map(|(_, word)| word),
        Some("import" | "export")
    )
}

/// Whether the word at `at` is a value imported or re-exported by name.
///
/// `import { bool } from "./postgresql.js"` names the codec, not the deprecated
/// alias, and `import { any }` names a binding. The list may break after `{`,
/// which is the shape the formatter writes. `import type { bool }` and
/// `import { type bool }` are types, so they are not this.
fn names_an_imported_value(code: &str, at: usize, outer: Enclosing) -> bool {
    if !in_value_specifier(code, at, outer) {
        return false;
    }
    if previous_word(code, at).is_some_and(|(_, word)| word == "type") {
        return false;
    }
    // `import { type Flag as bool }` — the local name is a type too.
    if let Some((as_at, "as")) = previous_word(code, at)
        && let Some((imported_at, _)) = previous_word(code, as_at)
        && previous_word(code, imported_at).is_some_and(|(_, word)| word == "type")
    {
        return false;
    }
    true
}

/// Whether `at` stands in a `{ … }` that imports or re-exports values.
///
/// The brace on this line decides it. A specifier continued from the line
/// above asks [`Enclosing::value_specifiers`], which is that brace carried
/// forward.
fn in_value_specifier(code: &str, at: usize, outer: Enclosing) -> bool {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = at;
    while index > 0 {
        index -= 1;
        match bytes[index] {
            b'}' | b')' | b']' => depth += 1,
            b'{' | b'(' | b'[' => {
                if depth == 0 {
                    return bytes[index] == b'{' && opens_value_specifiers(code, index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    outer.value_specifiers
}

/// Whether the `{` at `brace` opens `import { … }` or `export { … }`.
///
/// `import type { … }` and `export type { … }` have `type` in front of the
/// brace, so they open a list of types and this is false.
fn opens_value_specifiers(code: &str, brace: usize) -> bool {
    matches!(
        previous_word(code, brace).map(|(_, word)| word),
        Some("import" | "export")
    )
}

/// Whether the word is a shorthand property or a shorthand binding.
///
/// `const { any } = obj`, `function f({ any })`, `catch ({ any })`,
/// `({ any } = obj)`, and `const x = { any }` name a value. `export type
/// { React$Node }` names a type, because `type` stands in front of the brace.
/// `<p>{any}</p>` is a JSX expression: the tag's `>` is glued to `{`.
fn names_a_shorthand_binding(code: &str, at: usize, len: usize) -> bool {
    if !next_non_space(code, at + len).is_some_and(|(_, byte)| matches!(byte, b'}' | b',')) {
        return false;
    }
    let Some(brace) = (match prev_non_space(code, at) {
        Some((index, b'{')) => Some(index),
        Some((index, b',')) => enclosing_brace(code, index),
        _ => None,
    }) else {
        return false;
    };
    if brace > 0 && code.as_bytes()[brace - 1] == b'>' {
        return false;
    }
    !opens_a_type_specifier(code, brace)
}

/// Whether the name after `:` is a destructuring binding.
///
/// `const { a: any } = obj`, `function f({ a: any })`, and `({ a: any } = obj)`
/// bind a value. `type T = { a: any }`, `function f(): { a: any }`, and
/// `class C { x: any }` name a type. `const x = { a: any }` is a property
/// value, which is a different shape: its brace follows `=`.
fn names_a_renamed_binding(code: &str, at: usize) -> bool {
    let Some((colon, b':')) = prev_non_space(code, at) else {
        return false;
    };
    let Some((key, _)) = previous_word(code, colon) else {
        return false;
    };
    let Some(brace) = brace_of_key(code, key) else {
        return false;
    };
    brace_is_a_binding_pattern(code, brace)
}

/// The `{` that holds a property key, which may sit after `{` or `,`.
fn brace_of_key(code: &str, key: usize) -> Option<usize> {
    match prev_non_space(code, key) {
        Some((index, b'{')) => Some(index),
        Some((index, b',')) => enclosing_brace(code, index),
        _ => None,
    }
}

/// Whether `{` opens a value pattern rather than a type.
///
/// `const { … }`, `let { … }`, `var { … }`, `function f({ … })`,
/// `catch ({ … })`, and `({ … } = obj)` do. A `{` after `:` nests inside one
/// of those.
fn brace_is_a_binding_pattern(code: &str, brace: usize) -> bool {
    let Some((prev, byte)) = prev_non_space(code, brace) else {
        return false;
    };
    if is_word_byte(byte) {
        return matches!(
            previous_word(code, brace).map(|(_, word)| word),
            Some("const" | "let" | "var")
        );
    }
    if byte == b'(' {
        return paren_opens_a_binding(code, prev, brace);
    }
    if byte == b':' {
        let Some((key, _)) = previous_word(code, prev) else {
            return false;
        };
        return brace_of_key(code, key)
            .is_some_and(|outer| brace_is_a_binding_pattern(code, outer));
    }
    false
}

/// Whether `(` introduces the pattern at `brace`.
///
/// `function f({ … })` and `catch ({ … })` do. `({ a: any } = obj)` does,
/// because `=` follows the pattern. `type T = ({ a: any })` does not.
fn paren_opens_a_binding(code: &str, open: usize, brace: usize) -> bool {
    if previous_word(code, open).is_some_and(|(_, word)| word == "catch") {
        return true;
    }
    if let Some((name, _)) = previous_word(code, open)
        && previous_word(code, name).is_some_and(|(_, word)| word == "function")
    {
        return true;
    }
    let Some(close) = matching_close_brace(code, brace) else {
        return false;
    };
    next_non_space(code, close + 1).is_some_and(|(_, byte)| byte == b'=')
}

/// The `}` that closes the `{` at `open`, within this line.
fn matching_close_brace(code: &str, open: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0i32;
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => depth += 1,
            b'}' if depth == 0 => return Some(index),
            b'}' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    None
}

/// Whether `{` opens `import type { … }` or `export type { … }`.
fn opens_a_type_specifier(code: &str, brace: usize) -> bool {
    let Some((type_at, "type")) = previous_word(code, brace) else {
        return false;
    };
    matches!(
        previous_word(code, type_at).map(|(_, word)| word),
        Some("import" | "export")
    )
}

/// Whether the name at `at` is a property key in an object type.
///
/// A key is followed by `:`, or by `?:` when it is optional. In front of it may
/// stand the object's opener, a separator, a variance sigil, or `readonly` — all
/// of which say "a key comes next" and none of which is a type position.
fn names_a_property_key(code: &str, at: usize, len: usize, before: Option<(usize, u8)>) -> bool {
    let mut after = at + len;
    // `any?: T`, which is the same key with a `?` on it.
    if next_non_space(code, after).is_some_and(|(_, byte)| byte == b'?') {
        let (index, _) = next_non_space(code, after).unwrap_or((after, b'?'));
        after = index + 1;
    }
    if !next_non_space(code, after).is_some_and(|(_, byte)| byte == b':') {
        return false;
    }
    // `+any` and `-any` are the covariant and contravariant spellings of the
    // same key.
    if before.is_none_or(|(_, byte)| matches!(byte, b'{' | b',' | b';' | b'+' | b'-')) {
        return true;
    }
    // `readonly any: …`, which is the third spelling of it.
    previous_word(code, at).is_some_and(|(_, word)| word == "readonly")
}

/// Whether the name at `at` is an optional parameter with no annotation.
///
/// `function take(any?)` and `function take(first, any?)` bind a value. The
/// `?` is followed by `)` or `,`. `type T = any ? U : V` and
/// `type T = (any ? U : V)` follow the `?` with a type, so they stay types.
/// Whether `return`, `const`, `let`, or `var` introduces the name as a value.
///
/// `return React$Node` and `let React$Node` are expressions and bindings.
/// `type Slot = React$Node` and `function f(): React$Node` are not.
fn introduced_as_a_value(code: &str, at: usize) -> bool {
    previous_word(code, at)
        .is_some_and(|(_, word)| matches!(word, "return" | "const" | "let" | "var"))
}

fn names_an_optional_parameter(code: &str, at: usize, len: usize) -> bool {
    if !prev_non_space(code, at).is_some_and(|(_, byte)| matches!(byte, b'(' | b',')) {
        return false;
    }
    let Some((mark, b'?')) = next_non_space(code, at + len) else {
        return false;
    };
    // `??`, `?.` and `?.()` are values, and already classified as such.
    if matches!(code.as_bytes().get(mark + 1), Some(b'?' | b'.' | b'(')) {
        return false;
    }
    next_non_space(code, mark + 1).is_none_or(|(_, byte)| matches!(byte, b')' | b','))
}

/// Whether the name at `at` is a function parameter (`(any: string)`, `any?`).
///
/// A parameter is a word with `(` or `,` in front and `:` after it, with an
/// optional `?` between the name and the colon. The type is what follows the
/// colon, so `(node: any)` is not this shape.
fn names_a_parameter(code: &str, at: usize, len: usize) -> bool {
    if !prev_non_space(code, at).is_some_and(|(_, byte)| matches!(byte, b'(' | b',')) {
        return false;
    }
    let mut after = at + len;
    if next_non_space(code, after).is_some_and(|(_, byte)| byte == b'?') {
        let (index, _) = next_non_space(code, after).unwrap_or((after, b'?'));
        after = index + 1;
    }
    next_non_space(code, after).is_some_and(|(_, byte)| byte == b':')
}

/// Whether the word at `at` is the right-hand side of a value assignment.
///
/// `const value = any` and `ctor = Function` name a value. `type Handler =
/// Function`, `type Box<T = any>`, and `opaque type Box: Super = any` name a
/// type. A `<`, `,`, or `:` in front of the name on the left keeps it a type,
/// including when that byte is the end of the previous line (`type Box<\n T =
/// any`).
fn assignment_is_a_value(code: &str, at: usize, outer: Enclosing) -> bool {
    let Some((eq, b'=')) = prev_non_space(code, at) else {
        return false;
    };
    if eq > 0 && matches!(code.as_bytes()[eq - 1], b'=' | b'!') {
        return false;
    }
    equals_assigns_a_value(code, eq, outer)
}

/// Whether the `=` at `eq` assigns a value.
///
/// `const value =` and `ctor =` do. `type Handler =` and `type Box<T =` do not.
fn equals_assigns_a_value(code: &str, eq: usize, outer: Enclosing) -> bool {
    let Some((name_at, _)) = previous_word(code, eq) else {
        return false;
    };
    if matches!(
        previous_word(code, name_at).map(|(_, word)| word),
        Some("type" | "opaque")
    ) {
        return false;
    }
    if previous_word(code, name_at).is_some_and(|(_, word)| matches!(word, "const" | "let" | "var"))
    {
        return true;
    }
    // `ctor = Function`, where the name is the whole left-hand side.
    match prev_non_space(code, name_at) {
        // A continued type-parameter list. `T = any` has nothing in front of
        // it on its own line; the `<` or `,` that introduces it closed the
        // line above.
        None => !matches!(outer.last_byte, Some(b'<' | b',')),
        Some((_, byte)) => matches!(byte, b';' | b'}' | b'{'),
    }
}

/// Whether `for` or `for await` binds this name before `of`.
///
/// `for (any of items)` and `for await (any of items)` bind a value.
/// `for (const item of React$Node)` still names a type after `of`.
fn names_a_for_of_binding(code: &str, at: usize, len: usize) -> bool {
    let Some((next, _)) = next_non_space(code, at + len) else {
        return false;
    };
    if !is_word_byte(code.as_bytes()[next]) {
        return false;
    }
    let next_len = identifier_len(code, next);
    if &code[next..next + next_len] != "of" {
        return false;
    }
    let Some((paren, b'(')) = prev_non_space(code, at) else {
        return false;
    };
    if previous_word(code, paren).is_some_and(|(_, word)| word == "for") {
        return true;
    }
    let Some((await_at, "await")) = previous_word(code, paren) else {
        return false;
    };
    previous_word(code, await_at).is_some_and(|(_, word)| word == "for")
}

/// Whether `for (` introduces this name as the loop initializer.
///
/// `for (any; i < n; i++)` binds a value. `for (const item of React$Node)`
/// still names a type after `of`, and `for (any of items)` is a for-of binding.
fn names_a_for_initializer(code: &str, at: usize, len: usize) -> bool {
    if !next_non_space(code, at + len).is_some_and(|(_, byte)| byte == b';') {
        return false;
    }
    let Some((paren, b'(')) = prev_non_space(code, at) else {
        return false;
    };
    previous_word(code, paren).is_some_and(|(_, word)| word == "for")
}

/// Whether this name is the update clause of a `for` loop.
///
/// `for (;; any)` and `for (let i = 0; i < n; any)` name a value. A `for` whose
/// header has fewer than two semicolons is not one: `for (const item of
/// React$Node)` and `type F = (any) => void` stay types.
fn names_a_for_update(code: &str, at: usize, len: usize) -> bool {
    if !next_non_space(code, at + len).is_some_and(|(_, byte)| byte == b')') {
        return false;
    }
    let mut semis = 0usize;
    let mut paren = 0usize;
    let mut index = at;
    while let Some((prev, byte)) = prev_non_space(code, index) {
        match byte {
            b')' => paren += 1,
            b'(' if paren > 0 => paren -= 1,
            b'(' => {
                return semis == 2
                    && previous_word(code, prev).is_some_and(|(_, word)| word == "for");
            }
            b';' if paren == 0 => semis += 1,
            _ => {}
        }
        index = prev;
    }
    false
}

/// Whether the name is a function or arrow parameter's default.
///
/// `function f(value = Object)` and `function f(value: string = React$Node)`
/// pass a value. `type Box<T = React$Node>` and `function f<T = any>()` are
/// type-parameter defaults, and stay types. `type F = (value = any)` follows a
/// type alias, so that default stays a type too.
fn names_a_parameter_default(code: &str, at: usize, outer: Enclosing) -> bool {
    let Some((eq, b'=')) = prev_non_space(code, at) else {
        return false;
    };
    if eq > 0
        && matches!(
            code.as_bytes()[eq - 1],
            b'=' | b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^'
        )
    {
        return false;
    }
    parameter_list_is_a_value(code, eq, outer)
}

/// Whether the `=` at `eq` sits in a value parameter list.
///
/// A `<` before any `(` introduces a type parameter. A `(` belongs to
/// `function` or to an arrow, and an annotation (`value: Box<string>`) is
/// skipped on the way there, including its own generics.
fn parameter_list_is_a_value(code: &str, eq: usize, outer: Enclosing) -> bool {
    let bytes = code.as_bytes();
    let mut angle = 0usize;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;
    let mut index = eq;
    while let Some((prev, byte)) = prev_non_space(code, index) {
        let at_root = angle == 0 && paren == 0 && bracket == 0 && brace == 0;
        match byte {
            b'>' if prev > 0 && bytes[prev - 1] == b'=' => {}
            b'>' if paren == 0 && bracket == 0 && brace == 0 => angle += 1,
            b'<' if angle > 0 && paren == 0 && bracket == 0 && brace == 0 => angle -= 1,
            b'<' if at_root => return false,
            b')' => paren += 1,
            b'(' if paren > 0 => paren -= 1,
            b'(' if at_root => return paren_opens_a_value_parameter(code, prev, outer),
            b']' => bracket += 1,
            b'[' if bracket > 0 => bracket -= 1,
            b'}' => brace += 1,
            b'{' if brace > 0 => brace -= 1,
            _ => {}
        }
        index = prev;
    }
    false
}

/// Whether `(` opens the parameter list of a function or arrow.
///
/// `function f(` and `const f = (` do. `type F = (` is a type alias, and so is
/// a `(` continued from `type F =` on the line above.
fn paren_opens_a_value_parameter(code: &str, paren: usize, outer: Enclosing) -> bool {
    let Some((prev, byte)) = prev_non_space(code, paren) else {
        return !outer.continues_a_type;
    };
    if byte == b'=' {
        if prev > 0 && matches!(code.as_bytes()[prev - 1], b'=' | b'!') {
            return false;
        }
        return equals_assigns_a_value(code, prev, outer);
    }
    if byte == b'>' {
        let Some(open) = matching_open_angle(code, prev) else {
            return false;
        };
        return angle_opens_a_value_parameter(code, open, outer);
    }
    if matches!(byte, b':' | b'|' | b'&') {
        return false;
    }
    if is_word_byte(byte) {
        let Some((word_at, word)) = previous_word(code, paren) else {
            return false;
        };
        if word == "function"
            || previous_word(code, word_at).is_some_and(|(_, earlier)| earlier == "function")
        {
            return true;
        }
        if word == "async"
            && let Some((eq, b'=')) = prev_non_space(code, word_at)
            && (eq == 0 || !matches!(code.as_bytes()[eq - 1], b'=' | b'!'))
        {
            return equals_assigns_a_value(code, eq, outer);
        }
    }
    false
}

/// Whether `<` opens the type parameters of a value function or arrow.
fn angle_opens_a_value_parameter(code: &str, open: usize, outer: Enclosing) -> bool {
    let Some((prev, byte)) = prev_non_space(code, open) else {
        return !outer.continues_a_type;
    };
    if byte == b'=' {
        if prev > 0 && matches!(code.as_bytes()[prev - 1], b'=' | b'!') {
            return false;
        }
        return equals_assigns_a_value(code, prev, outer);
    }
    if !is_word_byte(byte) {
        return false;
    }
    let Some((word_at, word)) = previous_word(code, open) else {
        return false;
    };
    word == "function"
        || previous_word(code, word_at).is_some_and(|(_, earlier)| earlier == "function")
}

/// Whether the name is a declared type parameter.
///
/// `type Box<any>`, `function f<any>()`, and `type F = <any>(x: string) => void`
/// name a parameter. `Box<any>`, `f<any>(1)`, `type Box<T = any>`, and
/// `function f<T: any>()` still name a type.
fn names_a_type_parameter(code: &str, at: usize) -> bool {
    let Some(open) = (match prev_non_space(code, at) {
        Some((index, b'<')) => Some(index),
        Some((index, b',')) => matching_open_angle(code, index),
        _ => None,
    }) else {
        return false;
    };
    declares_type_parameters(code, open)
}

/// Whether `<` introduces type parameters rather than type arguments.
///
/// The word in front of `<` belongs to `type`, `function`, `class`,
/// `interface`, or `opaque`. `opaque type Box<` reads `type`. A `<` with no
/// word in front is a generic function type when its `>` is glued to `(`.
fn declares_type_parameters(code: &str, open: usize) -> bool {
    match prev_non_space(code, open) {
        Some((_, byte)) if is_word_byte(byte) => {
            let Some((name, _)) = previous_word(code, open) else {
                return false;
            };
            matches!(
                previous_word(code, name).map(|(_, word)| word),
                Some("type" | "function" | "class" | "interface" | "opaque")
            )
        }
        _ => generic_function_type(code, open),
    }
}

/// Whether `<` at `open` is `<T>(` — a generic function type, not a call.
fn generic_function_type(code: &str, open: usize) -> bool {
    let Some(close) = matching_close_angle(code, open) else {
        return false;
    };
    code.as_bytes().get(close + 1) == Some(&b'(')
}

/// The `>` that matches the `<` at `open`, within this line.
///
/// A `>` glued to `=` is an arrow, so it does not change the depth.
fn matching_close_angle(code: &str, open: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = open + 1;
    while index < bytes.len() {
        if bytes[index] == b'>' && index > 0 && bytes[index - 1] == b'=' {
            index += 1;
            continue;
        }
        match bytes[index] {
            b'<' => depth += 1,
            b'>' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Whether `#` is glued to the front of the word.
///
/// `class C { #any; }` and `class C { #React$Node: string; }` name a private
/// field. Flow has no private types, so the name is a value. `class C { #x:
/// any; }` still reports the annotation.
fn names_a_private_name(code: &str, at: usize) -> bool {
    at > 0 && code.as_bytes()[at - 1] == b'#'
}

/// Whether `@` is glued to the front of the word.
///
/// `class C { @any method() {} }` names a decorator. Flow has no decorator
/// types, so the name is a value. `class C { @dec x: any }` still reports the
/// annotation.
fn names_a_decorator(code: &str, at: usize) -> bool {
    at > 0 && code.as_bytes()[at - 1] == b'@'
}

/// Whether the name is an enum member.
///
/// `enum E { any }` and `enum E of string { React$Node }` name a member.
/// `enum E of React$Node` names the representation, and stays a type.
fn names_an_enum_member(code: &str, at: usize) -> bool {
    if previous_word(code, at).is_some_and(|(_, word)| word == "of") {
        return false;
    }
    let Some(brace) = (match prev_non_space(code, at) {
        Some((index, b'{')) => Some(index),
        Some((index, b',')) => enclosing_brace(code, index),
        _ => None,
    }) else {
        return false;
    };
    word_before(code, brace, "enum")
}

/// Whether `word` occurs before `from`, skipping other identifiers.
fn word_before(code: &str, from: usize, word: &str) -> bool {
    let mut at = from;
    for _ in 0..8 {
        let Some((word_at, found)) = previous_word(code, at) else {
            return false;
        };
        if found == word {
            return true;
        }
        at = word_at;
    }
    false
}

/// Whether the name is an element of an array pattern.
///
/// `const [any] = xs`, `function f([any])`, and `([any] = xs)` bind a value.
/// `type T = [React$Node]` and `function f(): [React$Node]` name a type.
fn names_an_array_binding(code: &str, at: usize) -> bool {
    let Some(bracket) = (match prev_non_space(code, at) {
        Some((index, b'[')) => Some(index),
        Some((index, b',')) => enclosing_bracket(code, index),
        _ => None,
    }) else {
        return false;
    };
    bracket_is_a_binding(code, bracket)
}

/// The `[` that contains `before`, within this line.
fn enclosing_bracket(code: &str, before: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = before;
    while index > 0 {
        index -= 1;
        match bytes[index] {
            b']' => depth += 1,
            b'[' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether `[` opens a binding pattern.
fn bracket_is_a_binding(code: &str, bracket: usize) -> bool {
    let Some((prev, byte)) = prev_non_space(code, bracket) else {
        return false;
    };
    if is_word_byte(byte) {
        return matches!(
            previous_word(code, bracket).map(|(_, word)| word),
            Some("const" | "let" | "var")
        );
    }
    if byte == b'(' {
        if previous_word(code, prev).is_some_and(|(_, word)| word == "catch") {
            return true;
        }
        if let Some((name, _)) = previous_word(code, prev)
            && previous_word(code, name).is_some_and(|(_, word)| word == "function")
        {
            return true;
        }
        let Some(close) = matching_close_bracket(code, bracket) else {
            return false;
        };
        return next_non_space(code, close + 1).is_some_and(|(_, byte)| byte == b'=');
    }
    if byte == b',' {
        return enclosing_bracket(code, prev)
            .is_some_and(|outer| bracket_is_a_binding(code, outer));
    }
    false
}

/// The `]` that closes the `[` at `open`, within this line.
fn matching_close_bracket(code: &str, open: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0i32;
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'[' => depth += 1,
            b']' if depth == 0 => return Some(index),
            b']' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    None
}

/// Whether the name is a property value in an object literal.
///
/// `const x = { a: any }` and `const x = { a: { b: any } }` pass a value.
/// `type T = { a: any }`, `function f(): { a: any }`, and `class C { x: any }`
/// name a type.
fn names_an_object_value(code: &str, at: usize, outer: Enclosing) -> bool {
    let Some((colon, b':')) = prev_non_space(code, at) else {
        return false;
    };
    let Some(brace) = enclosing_brace(code, colon) else {
        return false;
    };
    brace_holds_a_value(code, brace, outer)
}

/// Whether `{` opens an object literal rather than a type.
fn brace_holds_a_value(code: &str, brace: usize, outer: Enclosing) -> bool {
    let Some((prev, byte)) = prev_non_space(code, brace) else {
        return false;
    };
    if byte == b'=' && (prev == 0 || !matches!(code.as_bytes()[prev - 1], b'=' | b'!')) {
        return equals_assigns_a_value(code, prev, outer);
    }
    if byte == b':' {
        return enclosing_brace(code, prev)
            .is_some_and(|outer_brace| brace_holds_a_value(code, outer_brace, outer));
    }
    false
}

/// Whether the word sits next to an operator a type annotation cannot have.
///
/// `<` after the name is a comparison (`any < limit`). `>` *before* it is a
/// comparison (`count > any`) or the end of a JSX tag. The `>` of `=>` is not
/// one of those when it returns a type (`type T = () => any`, `type T = new ()
/// => Object`); `const f = () => any` is still a value. `<` *before* it is a
/// comparison too (`count < any`, `count < any && ready`) unless the rest of
/// the line is a type: `Array<any>`, `Foo<any, T>`, `Foo<any | T>`,
/// `Foo<any & T>` and `Foo<any = T>`. `>=` and `<=` are the same comparisons.
/// `>` *after* the name compares when an expression follows (`any > limit`,
/// `any >> 1`); `Array<any>` and `Array<any>>` have no expression there, so
/// they stay generics. `+`, `-`, `*`, `/`, `%` and `^` are arithmetic, on either
/// side (`foo(any + 1)`, `foo(1 + any)`, `foo(any ^ mask)`). `!` after the name is `!=` and
/// `!==`. `=` after the name is `==` and `===`, or an assignment (`any = 1`);
/// a single `=` with `<` in front is the default in `Foo<any = T>`.
///
/// `|` and a single `&` stay types, because they build a union and an
/// intersection. `&&`, `||`, `??` and `?.` are values, on either side. A
/// conditional's `?` is not one of them. The `=` *before*
/// `type Box = any` is not one of these.
fn beside_a_value_operator(
    code: &str,
    before: Option<(usize, u8)>,
    after: Option<(usize, u8)>,
) -> bool {
    if let Some((index, byte)) = after {
        match byte {
            b'+' | b'-' | b'*' | b'/' | b'%' | b'<' | b'!' | b'^' => return true,
            // `&&` and `||` are values. A single `&` or `|` is still a type.
            b'&' | b'|' if code.as_bytes().get(index + 1) == Some(&byte) => return true,
            // `??`, `?.` and `?.()` are values. `T extends any ? U : V` is not:
            // a conditional's `?` is followed by a type, not by `?`, `.` or `(`.
            b'?' if matches!(code.as_bytes().get(index + 1), Some(b'?' | b'.' | b'(')) => {
                return true;
            }
            b'=' => {
                let compared = code.as_bytes().get(index + 1) == Some(&b'=');
                // `Foo<any = T>` is a default type argument, not `any = 1`.
                // `const value: any = 1` is an annotation in front of an
                // initializer, not an assignment to `any`.
                let type_default = !compared && before.is_some_and(|(_, byte)| byte == b'<');
                let annotation = !compared && annotation_before_initializer(code, before);
                // `const made: () => Object = fn` — the `=` initializes the
                // binding, and `Object` is the arrow's return type.
                let arrow_return = !compared
                    && before.is_some_and(|(index, byte)| {
                        byte == b'>' && arrow_return_is_a_type(code, index)
                    });
                if !type_default && !annotation && !arrow_return {
                    return true;
                }
            }
            b'>' if angle_starts_a_comparison(code, index) => return true,
            _ => {}
        }
    }
    if let Some((index, byte)) = before {
        match byte {
            b'+' | b'-' | b'*' | b'/' | b'%' | b'!' | b'~' | b'^' => return true,
            // `count > any` is a comparison. `type T = () => any` is a return
            // type, so that `>` is not one. `const f = () => any` is a value.
            b'>' if !arrow_return_is_a_type(code, index) => return true,
            b'&' | b'|' if index > 0 && code.as_bytes()[index - 1] == byte => return true,
            b'?' if index > 0 && code.as_bytes()[index - 1] == b'?' => return true,
            b'<' if less_than_starts_a_comparison(code, after) => return true,
            b'=' if index > 0 && matches!(code.as_bytes()[index - 1], b'>' | b'<') => return true,
            _ => {}
        }
    }
    false
}

/// Whether a `:` in front of the name makes the following `=` an initializer.
///
/// `const value: any = 1`, `function take(value: any = 1)`, and
/// `class Box { value: any = 1 }` name a type. `label: any = 1` assigns, and
/// so does a label inside a function or a method. A comma inside
/// `{ a, b: any = 1 }` renames a property, so that name stays a value.
/// `Foo<any = T>` is a type default, which the caller tells apart by the `<`.
fn annotation_before_initializer(code: &str, before: Option<(usize, u8)>) -> bool {
    let Some((colon_at, b':')) = before else {
        return false;
    };
    let Some((bound_at, _)) = previous_word(code, colon_at) else {
        return false;
    };
    if previous_word(code, bound_at)
        .is_some_and(|(_, word)| matches!(word, "const" | "let" | "var" | "static"))
    {
        return true;
    }
    match prev_non_space(code, bound_at) {
        Some((_, b'(')) => true,
        Some((_, b',')) => comma_separates_a_parameter(code, bound_at),
        Some((at, b'{')) => brace_opens_a_class(code, at),
        Some((at, b';')) => {
            enclosing_brace(code, at).is_some_and(|brace| brace_opens_a_class(code, brace))
        }
        _ => false,
    }
}

/// Whether the comma before a binding separates parameters.
///
/// `function take(first, value: any = 1)` is enclosed by `(`. `const { a, b: any = 1 }`
/// and `function f({ a, b: any = 1 })` hit `{` first, and the colon renames `b`.
fn comma_separates_a_parameter(code: &str, bound_at: usize) -> bool {
    let bytes = code.as_bytes();
    let mut depth = 0i32;
    let mut index = bound_at;
    while index > 0 {
        index -= 1;
        match bytes[index] {
            b')' | b'}' | b']' => depth += 1,
            b'(' | b'{' | b'[' if depth == 0 => return bytes[index] == b'(',
            b'(' | b'{' | b'[' => depth -= 1,
            _ => {}
        }
    }
    false
}

/// Whether the `{` at `brace` opens a class body.
///
/// `class Box {`, `export default class {`, and `class Box<T> extends Super {`
/// do. `function f() {` does not.
fn brace_opens_a_class(code: &str, brace: usize) -> bool {
    match declaration_before(code, brace) {
        Some("class") => true,
        Some("extends") => extends_follows_a_class(code, brace),
        _ => false,
    }
}

/// `class Box extends Super {` — `extends` is only a class when `class` is
/// what it belongs to.
fn extends_follows_a_class(code: &str, brace: usize) -> bool {
    let (at, byte) = match prev_non_space(code, brace) {
        Some(found) => found,
        None => return false,
    };
    let super_at = if byte == b'>' {
        let Some(open) = matching_open_angle(code, at) else {
            return false;
        };
        let Some((word_at, _)) = previous_word(code, open) else {
            return false;
        };
        word_at
    } else {
        let Some((word_at, _)) = previous_word(code, brace) else {
            return false;
        };
        word_at
    };
    let Some((extends_at, "extends")) = previous_word(code, super_at) else {
        return false;
    };
    declaration_before(code, extends_at) == Some("class")
}

/// The `{` that contains `before`, within this line.
fn enclosing_brace(code: &str, before: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = before;
    while index > 0 {
        index -= 1;
        match bytes[index] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether a `<` immediately before a name is a comparison.
///
/// `count < any` and `count < any && ready` are. `Array<any>`, `Foo<any, T>`,
/// `Foo<any | T>`, `Foo<any & T>` and `Foo<any = T>` are the rest of a generic,
/// so `>`, `,`, `|`, `:`, a single `&` and a single `=` keep the name a type.
/// `==` after the name is still a comparison (`count < any == limit`).
/// Whether the `>` at `gt` closes a type arrow, so the following name is a
/// return type.
///
/// `type T = () => any`, `type T = (value: string) => any`, `type T = new () =>
/// Object` and `type Box<T = () => any>` are return types. `const f = () =>
/// any` and `count > any` are values; a comparison has no `=` glued to the
/// `>`.
fn arrow_return_is_a_type(code: &str, gt: usize) -> bool {
    if gt == 0 || code.as_bytes().get(gt - 1) != Some(&b'=') {
        return false;
    }
    let head = arrow_head(code, gt - 1);
    let mut introducer = head;
    if let Some((angle_close, b'>')) = prev_non_space(code, head)
        && let Some(open) = matching_open_angle(code, angle_close)
    {
        introducer = open;
    }
    if previous_word(code, introducer).is_some_and(|(_, word)| word == "new") {
        return true;
    }
    let Some((mark, byte)) = prev_non_space(code, introducer) else {
        return false;
    };
    if byte == b':' {
        return colon_annotates_a_binding(code, mark) || colon_is_a_function_return(code, mark);
    }
    if byte != b'=' {
        return false;
    }
    if mark > 0 && matches!(code.as_bytes()[mark - 1], b'=' | b'!') {
        return false;
    }
    equals_introduces_a_type(code, mark)
}

/// `let callback: () => any` and `function take(callback: () => any)` annotate
/// a binding. `{ callback: () => any }` and `cond ? 1 : () => any` are values.
fn colon_annotates_a_binding(code: &str, colon: usize) -> bool {
    let Some((name_at, _)) = previous_word(code, colon) else {
        return false;
    };
    if previous_word(code, name_at).is_some_and(|(_, word)| matches!(word, "const" | "let" | "var"))
    {
        return true;
    }
    matches!(
        prev_non_space(code, name_at).map(|(_, byte)| byte),
        Some(b'(' | b',')
    )
}

/// `function make(): () => any` and `make(): () => any` declare a return type.
/// `cond ? (1) : () => any` has no name in front of the `(`.
fn colon_is_a_function_return(code: &str, colon: usize) -> bool {
    let Some((close, b')')) = prev_non_space(code, colon) else {
        return false;
    };
    let Some(open) = matching_open_paren(code, close) else {
        return false;
    };
    prev_non_space(code, open).is_some_and(|(_, byte)| is_word_byte(byte))
}

/// The start of the parameter list in front of the `=` of `=>`.
fn arrow_head(code: &str, eq_of_arrow: usize) -> usize {
    let Some((before_at, before)) = prev_non_space(code, eq_of_arrow) else {
        return eq_of_arrow;
    };
    if before == b')' {
        return matching_open_paren(code, before_at).unwrap_or(before_at);
    }
    if is_word_byte(before) {
        let bytes = code.as_bytes();
        let mut start = before_at;
        while start > 0 && is_word_byte(bytes[start - 1]) {
            start -= 1;
        }
        return start;
    }
    before_at
}

/// The `(` that matches the `)` at `close`, within this line.
fn matching_open_paren(code: &str, close: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = close;
    while index > 0 {
        index -= 1;
        match bytes[index] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether the `=` at `eq` introduces a type rather than a value.
///
/// `type T =` and `type Box<T =` do. `const f =` and `ctor =` do not.
/// `const callback: Handler =` initializes a value. `opaque type Box: Super =`
/// and `type Box<T: Super =` still introduce a type.
fn equals_introduces_a_type(code: &str, eq: usize) -> bool {
    let Some((name_at, _)) = previous_word(code, eq) else {
        return false;
    };
    if matches!(
        previous_word(code, name_at).map(|(_, word)| word),
        Some("type" | "opaque")
    ) {
        return true;
    }
    if previous_word(code, name_at).is_some_and(|(_, word)| matches!(word, "const" | "let" | "var"))
    {
        return false;
    }
    match prev_non_space(code, name_at).map(|(_, byte)| byte) {
        Some(b'<' | b',') => true,
        Some(b':') => colon_before_is_a_type_bound(code, name_at),
        _ => false,
    }
}

/// Whether the `:` before the name left of `=` binds a type.
///
/// `opaque type Box: Super =` and `opaque type Box<T>: Super =` do, and so
/// does a type-parameter bound `type Box<T: Super =`. `const callback: Handler
/// =` and `function take(callback: Handler =` initialize a value.
fn colon_before_is_a_type_bound(code: &str, name_at: usize) -> bool {
    let Some((colon_at, b':')) = prev_non_space(code, name_at) else {
        return false;
    };
    let Some(bound_at) = name_before_colon(code, colon_at) else {
        return false;
    };
    if matches!(
        previous_word(code, bound_at).map(|(_, word)| word),
        Some("type" | "opaque")
    ) {
        return true;
    }
    matches!(
        prev_non_space(code, bound_at).map(|(_, byte)| byte),
        Some(b'<' | b',')
    )
}

/// The declared name in front of a `:`, skipping one generic list.
fn name_before_colon(code: &str, colon_at: usize) -> Option<usize> {
    let (at, byte) = prev_non_space(code, colon_at)?;
    if byte == b'>' {
        let open = matching_open_angle(code, at)?;
        return previous_word(code, open).map(|(word_at, _)| word_at);
    }
    previous_word(code, colon_at).map(|(start, _)| start)
}

fn less_than_starts_a_comparison(code: &str, after: Option<(usize, u8)>) -> bool {
    let Some((index, byte)) = after else {
        return true;
    };
    match byte {
        b'>' | b',' | b'|' | b':' => false,
        b'=' => code.as_bytes().get(index + 1) == Some(&b'='),
        b'&' => code.as_bytes().get(index + 1) == Some(&b'&'),
        _ => true,
    }
}

/// Whether the `>` at `gt` starts a comparison rather than closing a generic.
///
/// `any > limit`, `any >= limit`, `any >> 1` and `any >>> 0` are followed by
/// an expression. `Array<any>`, `Array<any>>` and `Foo<any> | Bar` are
/// followed by the end of the type, or by `|`, `&`, `,` or another closer.
/// `Map<string, any>()` is a call, so a `(` with nothing between it and the
/// `>` stays a generic; `any > (limit)` has a space, and that one compares.
/// `Foo<any> extends Bar` is a clause, not `any > extends`.
fn angle_starts_a_comparison(code: &str, gt: usize) -> bool {
    let bytes = code.as_bytes();
    if bytes.get(gt + 1) == Some(&b'=') {
        return true;
    }
    let mut index = gt;
    while bytes.get(index) == Some(&b'>') {
        index += 1;
    }
    let Some((next, byte)) = next_non_space(code, index) else {
        return false;
    };
    if byte == b'(' && next == index {
        return false;
    }
    // `class Box<T = () => any> extends Object` — `extends` follows the
    // generic, so the `>` is its closer. `any > limit` is still a comparison.
    if is_word_byte(byte) {
        let len = identifier_len(code, next);
        if &code[next..next + len] == "extends" {
            return false;
        }
    }
    is_word_byte(byte)
        || byte.is_ascii_digit()
        || matches!(
            byte,
            b'(' | b'!' | b'~' | b'+' | b'-' | b'\'' | b'"' | b'`' | b'[' | b'{'
        )
}

/// Whether a value keyword stands immediately beside the word.
///
/// `in` and `instanceof` are operators on either side. `typeof`, `void`,
/// `await`, `yield`, `new`, `throw`, `delete`, `break` and `continue` make the
/// name that follows them a value. `break any` is a label. `new Object` is a
/// constructor call even without parentheses.
/// A `|` between a keyword and the name keeps the name a type: `type U = void |
/// any` is not `void any`, and `new (x: any) => void` is not `new any`.
fn value_keyword_operand(code: &str, at: usize, len: usize) -> bool {
    if prev_non_space(code, at).is_some_and(|(_, byte)| is_word_byte(byte))
        && previous_word(code, at).is_some_and(|(_, word)| {
            matches!(
                word,
                "in" | "instanceof"
                    | "typeof"
                    | "void"
                    | "await"
                    | "yield"
                    | "new"
                    | "throw"
                    | "delete"
                    | "break"
                    | "continue"
            )
        })
    {
        return true;
    }
    let Some((next_at, _)) = next_non_space(code, at + len) else {
        return false;
    };
    if !is_word_byte(code.as_bytes()[next_at]) {
        return false;
    }
    let next_len = identifier_len(code, next_at);
    matches!(&code[next_at..next_at + next_len], "in" | "instanceof")
}

/// Whether `export default` stands immediately in front of the word.
///
/// The default export is a value. `export type Box = any` is not one: `type`
/// sits between `export` and the name, so the name stays an annotation.
fn names_an_export_default(code: &str, at: usize) -> bool {
    if !prev_non_space(code, at).is_some_and(|(_, byte)| is_word_byte(byte)) {
        return false;
    }
    let Some((default_at, "default")) = previous_word(code, at) else {
        return false;
    };
    previous_word(code, default_at).is_some_and(|(_, word)| word == "export")
}

/// Whether the word is a statement or an unannotated class field.
///
/// `any;`, `function f() { React$Node; }`, `class C { React$Node; }`,
/// `class C { React$Node }` and `class C { x: string; React$Node }` are values.
/// A `:` or `=` in front keeps a type: `class C { x: React$Node; }`,
/// `type Slot = React$Node`, and `declare function f(): React$Node;`. A line
/// break does not change the alias: `type Slot =\n  React$Node;` is still a
/// type, because the `=` closed the line above. A `}` closes a class field
/// only when the `{` opens the class, including after another member, so
/// `export type { React$Node }` stays a type.
fn names_a_bare_statement(code: &str, at: usize, len: usize, outer: Enclosing) -> bool {
    let prev = prev_non_space(code, at);
    let at_edge = matches!(prev, None | Some((_, b'{' | b'}' | b';')));
    if !at_edge {
        return false;
    }
    match next_non_space(code, at + len) {
        Some((_, b';')) => !(prev.is_none() && outer.continues_a_type),
        Some((_, b'}')) => match prev {
            Some((open, b'{')) => brace_opens_a_class(code, open),
            Some((semi, b';')) => {
                enclosing_brace(code, semi).is_some_and(|brace| brace_opens_a_class(code, brace))
            }
            _ => false,
        },
        _ => false,
    }
}

/// Whether this line's trailing `=` finishes a type alias.
///
/// `type Slot =`, `type Box<T> =`, and `opaque type Hidden: Super =` do.
/// `const value =`, `count +=`, and `x ==` do not. The scan reads one line at
/// a time, so the alias's right-hand side asks the next line about this one.
fn line_ends_as_a_type(code: &str) -> bool {
    let Some((eq, b'=')) = prev_non_space(code, code.len()) else {
        return false;
    };
    if eq > 0
        && matches!(
            code.as_bytes()[eq - 1],
            b'=' | b'!' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'<' | b'>'
        )
    {
        return false;
    }
    type_alias_before(code, eq)
}

/// Whether `type` or `opaque` declares the alias whose `=` is at `eq`.
fn type_alias_before(code: &str, eq: usize) -> bool {
    let Some(at) = name_before(code, eq) else {
        return false;
    };
    if previous_word(code, at).is_some_and(|(_, word)| matches!(word, "type" | "opaque")) {
        return true;
    }
    // `opaque type Hidden: Super =` — the `=` follows the super type, and the
    // keyword stands in front of the alias, before the `:`.
    let Some((colon, b':')) = prev_non_space(code, at) else {
        return false;
    };
    let Some(alias) = name_before(code, colon) else {
        return false;
    };
    previous_word(code, alias).is_some_and(|(_, word)| matches!(word, "type" | "opaque"))
}

/// The name that ends just before `from`, skipping one `<…>` generic list.
fn name_before(code: &str, from: usize) -> Option<usize> {
    let (at, byte) = prev_non_space(code, from)?;
    if byte == b'>' {
        let open = matching_open_angle(code, at)?;
        return previous_word(code, open).map(|(name, _)| name);
    }
    if is_word_byte(byte) {
        return previous_word(code, from).map(|(name, _)| name);
    }
    None
}

/// Whether `class`, `enum`, or `interface` declares this word.
///
/// `class any {}` names a class. `class C implements React$Node` and
/// `interface I extends React$Node` name a type, and the keyword in front of
/// that type is `implements` or `extends`, not the declaration itself.
fn names_a_declaration(code: &str, at: usize) -> bool {
    previous_word(code, at).is_some_and(|(_, word)| matches!(word, "class" | "enum" | "interface"))
}

/// Whether `class … extends` stands immediately in front of the word.
///
/// A class extends a value (`class Box extends Object`, and `class Box<T>
/// extends Object`). An interface extends a type, so `interface Box extends
/// Object` stays an annotation. The type-parameter list between the name and
/// `extends` is skipped, including when a default contains an arrow
/// (`class Box<T = () => any> extends Object`). A `>` that does not close one
/// keeps the name a type.
fn extends_a_class(code: &str, at: usize) -> bool {
    if !prev_non_space(code, at).is_some_and(|(_, byte)| is_word_byte(byte)) {
        return false;
    }
    let Some((extends_at, "extends")) = previous_word(code, at) else {
        return false;
    };
    declaration_before(code, extends_at) == Some("class")
}

/// The declaration keyword before `from`, skipping one generic list.
///
/// `class Box<T>` puts `>` in front of `extends`. `class Box` puts the name
/// there, and `class extends` puts the keyword itself there.
fn declaration_before(code: &str, from: usize) -> Option<&str> {
    let (at, byte) = prev_non_space(code, from)?;
    let name_at = if byte == b'>' {
        let open = matching_open_angle(code, at)?;
        previous_word(code, open)?.0
    } else if is_word_byte(byte) {
        let (word_at, word) = previous_word(code, from)?;
        if matches!(word, "class" | "interface" | "type" | "opaque" | "enum") {
            return Some(word);
        }
        word_at
    } else {
        return None;
    };
    previous_word(code, name_at).map(|(_, word)| word)
}

/// The `<` that matches the `>` at `close`, within this line.
///
/// A `>` glued to `=` is an arrow (`() =>`), not a generic closer, so it does
/// not change the depth. `class Box<T = () => any>` still matches `Box<`, and
/// a nested `class Box<Foo<T>>` still matches the outer `<`.
fn matching_open_angle(code: &str, close: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = close;
    while index > 0 {
        index -= 1;
        if bytes[index] == b'>' && index > 0 && bytes[index - 1] == b'=' {
            continue;
        }
        match bytes[index] {
            b'>' => depth += 1,
            b'<' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether the word at `at` is the right operand of `==`, `===`, `!=` or `!==`.
fn follows_an_equality_operator(code: &str, at: usize) -> bool {
    let Some((end, b'=')) = prev_non_space(code, at) else {
        return false;
    };
    end > 0 && matches!(code.as_bytes()[end - 1], b'=' | b'!')
}

/// Whether the word at `at` is an entire argument of a call.
///
/// `expect.any(Function)` passes the constructor; `type Sink = (Object) => void`
/// names a parameter's type, and the two are told apart by what stands before
/// the `(`. A `(` that follows a name, a `)` or a `]` belongs to something being
/// called or declared; a `(` that follows anything else opens a group or a
/// function type, which is where a bare type may stand.
///
/// The word has to be the *whole* argument, which is what keeps a type inside
/// one out: `Map<string, any>` is reached with a `<` or a `,` in front and a `>`
/// behind, and `(node: any)` with a `:` in front.
fn is_a_bare_argument(code: &str, at: usize, len: usize, outer: Enclosing) -> bool {
    // What stands before it, on this line or — for the first word on a
    // continuation line — at the end of the last one.
    let before = match prev_non_space(code, at) {
        Some((_, byte)) => Some(byte),
        None => outer.last_byte,
    };
    if !before.is_some_and(|byte| matches!(byte, b'(' | b',')) {
        return false;
    }
    if !next_non_space(code, at + len).is_some_and(|(_, byte)| matches!(byte, b')' | b',')) {
        return false;
    }
    match enclosing_open_paren(code, at) {
        Some(open) => paren_opens_a_call(code, open),
        // The list was opened on an earlier line, so the question "is this a
        // call or a function type" was answered there and carried here.
        None => outer.kind == Some(Opener::Call),
    }
}

/// Whether the `(` at `open` starts a call rather than a group or a type.
///
/// A name, a `)` or a `]` in front is a call. `foo?.(any)` is one too: the
/// `(` follows `?.`. `type F = (any) => void` follows `=`, so it stays a type.
fn paren_opens_a_call(code: &str, open: usize) -> bool {
    let Some((at, byte)) = prev_non_space(code, open) else {
        return false;
    };
    if is_word_byte(byte) || byte == b')' || byte == b']' {
        return true;
    }
    byte == b'.' && at > 0 && code.as_bytes()[at - 1] == b'?'
}

/// What an argument list looked like when the line above ended.
///
/// The scan reads one line at a time, and until this existed a call spread over
/// several lines was invisible to it: `enclosing_open_paren` found no opener on
/// the continuation line and the word was reported as a type. `expect.any(\n
/// Function,\n)` is exactly the shape `@uniflowed/test` is written in, and
/// `check:lib` is an error-level gate on CI now, so a false positive there is a
/// red build for correct code.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Enclosing {
    /// The innermost delimiter still open, or `None` at the top level.
    kind: Option<Opener>,
    /// The last byte of code before this line, which is what tells `(` from
    /// `,` for the first word on a continuation line.
    last_byte: Option<u8>,
    /// Whether the innermost opener is the `{` of `import { … }` or
    /// `export { … }`. A specifier continued onto the next line is a value.
    value_specifiers: bool,
    /// The previous line ended a type alias (`type Slot =`). A name at the
    /// start of this line is still that alias. A blank line keeps the flag.
    continues_a_type: bool,
}

/// Which kind of bracket is open. Only `(` needs telling apart, and only into
/// the two kinds that decide this rule.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opener {
    /// `f(` or `new C(` — an argument list, where a bare `Function` is a value.
    Call,
    /// `(` after anything else, `[` or `{` — a group, a function type, an
    /// array or an object, where a bare `Function` may well be a type.
    Other,
}

impl Enclosing {
    /// This state, advanced over one line of code.
    ///
    /// Strings and comments are already blanked out of `code` by the scan, so
    /// a bracket here is a bracket in the program.
    fn after(self, code: &str) -> Self {
        let continues_a_type = if code
            .as_bytes()
            .iter()
            .any(|byte| !byte.is_ascii_whitespace())
        {
            line_ends_as_a_type(code)
        } else {
            self.continues_a_type
        };
        // The stack lives for one line and only its top is kept. A `Vec` here
        // allocated once per line, and `flow/deprecated-type` walks every line
        // with it, which put the router runtime over the allocation budget.
        const CAP: usize = 32;
        let mut inline = [Opener::Other; CAP];
        let mut specifiers = [false; CAP];
        let mut depth = 0usize;
        if let Some(kind) = self.kind {
            inline[0] = kind;
            specifiers[0] = self.value_specifiers;
            depth = 1;
        }
        let bytes = code.as_bytes();
        let mut last = self.last_byte;
        for (index, byte) in bytes.iter().enumerate() {
            match byte {
                b'(' => {
                    let call = paren_opens_a_call(code, index);
                    if depth < CAP {
                        inline[depth] = if call { Opener::Call } else { Opener::Other };
                        specifiers[depth] = false;
                        depth += 1;
                    }
                }
                b'[' => {
                    if depth < CAP {
                        inline[depth] = Opener::Other;
                        specifiers[depth] = false;
                        depth += 1;
                    }
                }
                b'{' => {
                    if depth < CAP {
                        inline[depth] = Opener::Other;
                        specifiers[depth] = opens_value_specifiers(code, index);
                        depth += 1;
                    }
                }
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            if !byte.is_ascii_whitespace() {
                last = Some(*byte);
            }
        }
        Self {
            kind: if depth > 0 {
                Some(inline[depth - 1])
            } else {
                None
            },
            last_byte: last,
            value_specifiers: depth > 0 && specifiers[depth - 1],
            continues_a_type,
        }
    }
}

/// The `(` that opens the list the byte at `at` stands in, within this line.
///
/// `None` when the nearest unclosed opener is a `[` or a `{` — an array or an
/// object literal is not an argument list — and when the line holds no opener
/// at all, which is what a list continued from the line above looks like to a
/// scanner that reads one line at a time.
fn enclosing_open_paren(code: &str, at: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = at;
    while index > 0 {
        index -= 1;
        match bytes[index] {
            b')' | b']' | b'}' => depth += 1,
            b'[' | b'{' => depth = depth.checked_sub(1)?,
            b'(' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn run_flow_deprecated_type(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::DeprecatedType.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    let mut enclosing = Enclosing::default();
    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let outer = enclosing;
        enclosing = enclosing.after(code);
        for at in find_words(code, "bool") {
            if line.in_string(at)
                || word_in_jsx_text(scan, position, at, 4)
                || names_a_value(code, at, 4, outer)
            {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "the `bool` type alias is deprecated; write `boolean`",
            );
        }
    }
}

/// Flow types that exist only for the checker's own use.
///
/// Referencing them compiles today and breaks on the next Flow upgrade, which is
/// exactly what Flow's `internal-type` lint is for.
static INTERNAL_TYPES: phf::Set<&'static str> = phf::phf_set! {
    "$Flow$EnumProto",
    "$Flow$EnumValueRepresentationTypes",
    "$Flow$ModuleRef",
    "$TEMPORARY$array",
    "$TEMPORARY$bigint",
    "$TEMPORARY$number",
    "$TEMPORARY$object",
    "$TEMPORARY$string",
    "React$AbstractComponent",
    "React$Component",
    "React$ComponentType",
    "React$Context",
    "React$Element",
    "React$ElementConfig",
    "React$ElementProps",
    "React$ElementRef",
    "React$ElementType",
    "React$Key",
    "React$MixedElement",
    "React$Node",
    "React$Portal",
    "React$Ref",
    "React$StatelessFunctionalComponent",
};

pub(crate) fn run_flow_internal_type(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::InternalType.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    let mut enclosing = Enclosing::default();
    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let outer = enclosing;
        enclosing = enclosing.after(code);
        let mut at = 0usize;
        while at < code.len() {
            let len = identifier_len(code, at);
            if len == 0 {
                at += 1;
                continue;
            }
            if line.in_string(at) {
                at += len;
                continue;
            }
            // `<p>React$Node</p>` and `<p>hello React$Node there</p>` are text.
            // The same operators that make `any` a value make an internal name
            // one too.
            if beside_a_value_operator(
                code,
                prev_non_space(code, at),
                next_non_space(code, at + len),
            ) || word_in_jsx_text(scan, position, at, len)
                || value_keyword_operand(code, at, len)
                || names_an_export_default(code, at)
                || names_a_default_import(code, at)
                || names_an_optional_parameter(code, at, len)
                || introduced_as_a_value(code, at)
                || assignment_is_a_value(code, at, outer)
                || is_a_bare_argument(code, at, len, outer)
                || names_a_parameter(code, at, len)
                || prev_non_space(code, at).is_some_and(|(_, byte)| byte == b'.')
                || next_non_space(code, at + len)
                    .is_some_and(|(_, byte)| matches!(byte, b'.' | b'(' | b'`'))
                || previous_word(code, at).is_some_and(|(_, word)| word == "case")
                || names_a_property_key(code, at, len, prev_non_space(code, at))
                || follows_an_equality_operator(code, at)
                || names_an_imported_value(code, at, outer)
                || names_a_shorthand_binding(code, at, len)
                || names_a_renamed_binding(code, at)
                || names_an_object_value(code, at, outer)
                || names_an_array_binding(code, at)
                || names_an_enum_member(code, at)
                || names_a_for_of_binding(code, at, len)
                || names_a_parameter_default(code, at, outer)
                || names_a_type_parameter(code, at)
                || names_a_private_name(code, at)
                || names_a_decorator(code, at)
                || names_a_for_initializer(code, at, len)
                || names_a_for_update(code, at, len)
                || names_a_declaration(code, at)
                || names_a_bare_statement(code, at, len, outer)
                || extends_a_class(code, at)
            {
                at += len;
                continue;
            }
            if starts_word(code, at) && INTERNAL_TYPES.contains(&code[at..at + len]) {
                push_in_code(
                    diagnostics,
                    scan,
                    rule,
                    severity,
                    position,
                    at,
                    "this is a Flow-internal type; use the public equivalent",
                );
            }
            at += len;
        }
    }
}

pub(crate) fn run_flow_ambiguous_object_type(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::AmbiguousObjectType.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let Some(brace_at) = type_alias_object_start(line.code()) else {
            continue;
        };
        report_ambiguous_object(scan, severity, rule, position, brace_at, diagnostics);
    }
}

/// Offset of the `{` that opens a `type X = { ... }` right-hand side.
///
/// Deliberately narrow: only type-alias right-hand sides are recognised, because
/// a bare `: {` is indistinguishable from an object literal or a ternary without
/// a real parser, and a linter that guesses is worse than one that under-reports.
fn type_alias_object_start(code: &str) -> Option<usize> {
    let mut at = next_non_space(code, 0)?.0;
    loop {
        let len = identifier_len(code, at);
        if len == 0 {
            return None;
        }
        match &code[at..at + len] {
            "export" | "declare" | "opaque" => at = next_non_space(code, at + len)?.0,
            "type" => {
                at += len;
                break;
            }
            _ => return None,
        }
    }

    let equals = at + code[at..].find('=')?;
    let (brace_at, byte) = next_non_space(code, equals + 1)?;
    (byte == b'{').then_some(brace_at)
}

/// Walk one object type from its opening `{` and report every nested object type
/// that states neither exactness (`{| |}`) nor inexactness (`...`).
fn report_ambiguous_object(
    scan: &FileScan<'_>,
    severity: Severity,
    rule: &'static str,
    start_line: usize,
    start_in_code: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    /// One open `{`: where it is, and what it has told us so far.
    struct Open {
        line: usize,
        column: usize,
        exact: bool,
        spread: bool,
    }

    let mut stack: Vec<Open> = Vec::new();
    for (position, line) in scan.lines.iter().enumerate().skip(start_line) {
        let code = line.code();
        let bytes = code.as_bytes();
        let mut at = if position == start_line {
            start_in_code
        } else {
            0
        };
        while at < bytes.len() {
            match bytes[at] {
                b'{' => {
                    let exact = bytes.get(at + 1) == Some(&b'|');
                    stack.push(Open {
                        line: position,
                        column: line.code_offset() + at,
                        exact,
                        spread: false,
                    });
                    at += if exact { 2 } else { 1 };
                }
                b'}' => {
                    let Some(open) = stack.pop() else {
                        return;
                    };
                    if !open.exact && !open.spread {
                        push_at(
                            diagnostics,
                            scan,
                            rule,
                            severity,
                            open.line,
                            open.column,
                            "object type is neither exact (`{| |}`) nor explicitly inexact (`...`)",
                        );
                    }
                    if stack.is_empty() {
                        return;
                    }
                    at += 1;
                }
                b'.' if bytes[at..].starts_with(b"...") => {
                    if let Some(open) = stack.last_mut() {
                        open.spread = true;
                    }
                    at += 3;
                }
                _ => at += 1,
            }
        }
    }
}
