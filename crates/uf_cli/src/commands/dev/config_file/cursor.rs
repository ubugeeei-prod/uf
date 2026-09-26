//! Where in the config object the cursor is, and what may be written there.
//!
//! The answer is a path from the config object — `test.coverage`, or
//! `plugins[]` for an element of a list — and a site: a key, or a value. The
//! path is what [`uf_config::schema::Schema::resolve`] takes; the site decides
//! whether keys or values are offered.
//!
//! # Between two characters
//!
//! A cursor sits between characters, so "on a word" means the cursor is inside
//! it or at its end: the end is where a cursor is while the word is being
//! typed. A hover names a character instead, and asks about the gap after it.
//!
//! # The gaps
//!
//! Most of the interesting positions are not on anything yet. After `{` or a
//! comma is where a key goes; after `key:` is where its value goes. A cursor on
//! the line after a finished entry is also where a key goes, because the comma
//! is usually typed after the next key rather than before it. A cursor on the
//! same line after a finished entry is nowhere — offering a key there would
//! write `quotes: "single" semicolons` — and so is a cursor in a comment.

use uf_config::schema::Step;

use super::outline::{self, Array, Object, Value, Word};

/// Where the cursor is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cursor<'s> {
    /// From the config object to the object or list the cursor is in; for a
    /// value, to the key the value belongs to.
    pub(super) path: Vec<Step<'s>>,
    /// What may be written at the cursor.
    pub(super) site: Site<'s>,
}

/// What may be written at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Site<'s> {
    /// A key of the object the path names.
    Key {
        /// The key the cursor is on, when it is on one.
        word: Option<Word>,
        /// Whether that key already has its `:`.
        colon: bool,
        /// The keys the object already has, apart from the one the cursor is
        /// on: a key already written is not offered again.
        present: Vec<&'s str>,
    },
    /// The value of the key the path ends with, or an element of the list it
    /// ends with.
    Value {
        /// The value the cursor is on, when it is on one.
        word: Option<Word>,
    },
}

/// Where `offset` is in `root`, or [`None`] when nothing may be written there.
pub(super) fn locate<'s>(source: &'s str, root: &Object, offset: usize) -> Option<Cursor<'s>> {
    if offset > source.len()
        || !source.is_char_boundary(offset)
        || !inside(root.open, root.close, offset)
    {
        return None;
    }
    in_object(source, root, offset, &mut Vec::new())
}

/// Whether `offset` is between a bracket and its closer, or after a bracket
/// that has none yet.
fn inside(open: usize, close: Option<usize>, offset: usize) -> bool {
    open < offset && close.is_none_or(|close| offset <= close)
}

fn in_object<'s>(
    source: &'s str,
    object: &Object,
    offset: usize,
    path: &mut Vec<Step<'s>>,
) -> Option<Cursor<'s>> {
    for (index, entry) in object.entries.iter().enumerate() {
        if let Some(key) = entry.key
            && key.holds(offset)
        {
            return Some(Cursor {
                path: std::mem::take(path),
                site: Site::Key {
                    word: Some(key),
                    colon: entry.colon.is_some(),
                    present: present(source, object, Some(index)),
                },
            });
        }

        let Some(value) = &entry.value else {
            continue;
        };
        // The step into a value needs a key that has been finished with a
        // colon; a spread or a computed key names nothing to look up.
        let named = || {
            entry
                .key
                .filter(|_| entry.colon.is_some())
                .map(|key| Step::Key(key.text(source)))
        };
        match value {
            Value::Object(inner) if inside(inner.open, inner.close, offset) => {
                path.push(named()?);
                return in_object(source, inner, offset, path);
            }
            Value::Array(inner) if inside(inner.open, inner.close, offset) => {
                path.push(named()?);
                return in_array(source, inner, offset, path);
            }
            Value::Word(word) if word.holds(offset) => {
                path.push(named()?);
                return Some(value_at(path, Some(*word)));
            }
            Value::Other(span) if span.start < offset && offset < span.end => return None,
            _ => {}
        }
    }

    // On nothing: in the gap after the last entry that starts before the
    // cursor, or after the `{` when there is none.
    let previous = object
        .entries
        .iter()
        .rev()
        .find(|entry| entry.start() < offset);
    let Some(previous) = previous else {
        outline::gap(source, object.open + 1, offset)?;
        return Some(key_at(source, object, path));
    };

    if let Some(colon) = previous.colon
        && colon < offset
        && previous
            .value
            .as_ref()
            .is_none_or(|value| offset <= value.start())
    {
        // `key: |` is where the value goes. `key: | value` is nowhere: the
        // value is already there.
        outline::gap(source, colon + 1, offset)?;
        if previous.value.is_some() {
            return None;
        }
        path.push(Step::Key(previous.key?.text(source)));
        return Some(value_at(path, None));
    }

    let gap = outline::gap(source, previous.end(), offset)?;
    (gap.comma || gap.newline).then(|| key_at(source, object, path))
}

fn in_array<'s>(
    source: &'s str,
    array: &Array,
    offset: usize,
    path: &mut Vec<Step<'s>>,
) -> Option<Cursor<'s>> {
    for element in &array.elements {
        match element {
            Value::Object(inner) if inside(inner.open, inner.close, offset) => {
                path.push(Step::Element);
                return in_object(source, inner, offset, path);
            }
            Value::Array(inner) if inside(inner.open, inner.close, offset) => {
                path.push(Step::Element);
                return in_array(source, inner, offset, path);
            }
            Value::Word(word) if word.holds(offset) => {
                path.push(Step::Element);
                return Some(value_at(path, Some(*word)));
            }
            Value::Other(span) if span.start < offset && offset < span.end => return None,
            _ => {}
        }
    }

    let previous = array
        .elements
        .iter()
        .rev()
        .find(|element| element.start() < offset);
    let from = previous.map_or(array.open + 1, Value::end);
    let gap = outline::gap(source, from, offset)?;
    if previous.is_none() || gap.comma || gap.newline {
        path.push(Step::Element);
        return Some(value_at(path, None));
    }
    None
}

fn key_at<'s>(source: &'s str, object: &Object, path: &mut Vec<Step<'s>>) -> Cursor<'s> {
    Cursor {
        path: std::mem::take(path),
        site: Site::Key {
            word: None,
            colon: false,
            present: present(source, object, None),
        },
    }
}

fn value_at<'s>(path: &mut Vec<Step<'s>>, word: Option<Word>) -> Cursor<'s> {
    Cursor {
        path: std::mem::take(path),
        site: Site::Value { word },
    }
}

/// The keys an object already has, apart from the entry at `skip`.
///
/// Only keys with their colon: a name with nothing after it is a key being
/// typed or a shorthand, and `uf.config.js` is read as data, where a shorthand
/// is not a key.
fn present<'s>(source: &'s str, object: &Object, skip: Option<usize>) -> Vec<&'s str> {
    object
        .entries
        .iter()
        .enumerate()
        .filter(|(index, entry)| Some(*index) != skip && entry.colon.is_some())
        .filter_map(|(_, entry)| entry.key.map(|key| key.text(source)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `locate` says about a document with `‸` where the cursor is: the
    /// path written as a hover writes it, and the site in words.
    fn at(marked: &str) -> Option<(String, String)> {
        let offset = marked.find('‸').expect("a cursor");
        let source = marked.replacen('‸', "", 1);
        let root = outline::read(&source)?;
        let cursor = locate(&source, &root, offset)?;
        let site = match cursor.site {
            Site::Key {
                word,
                colon,
                present,
            } => uf_infra::into_string(uf_infra::cstr!(
                "key {:?}{} present {present:?}",
                word.map(|word| word.text(&source)),
                if colon { " with colon" } else { "" },
            )),
            Site::Value { word } => uf_infra::into_string(uf_infra::cstr!(
                "value {:?}",
                word.map(|word| word.text(&source))
            )),
        };
        Some((super::super::dotted(&cursor.path), site))
    }

    fn expect(marked: &str, path: &str, site: &str) {
        assert_eq!(
            at(marked),
            Some((path.to_owned(), site.to_owned())),
            "{marked}"
        );
    }

    #[test]
    fn a_key_goes_after_a_brace_or_a_comma() {
        expect(
            "export default defineConfig({ ‸ })",
            "",
            "key None present []",
        );
        expect(
            "export default defineConfig({ fmt: {}, ‸ })",
            "",
            r#"key None present ["fmt"]"#,
        );
        expect(
            "export default {\n  fmt: {\n    ‸\n  },\n};\n",
            "fmt",
            "key None present []",
        );
    }

    #[test]
    fn a_half_typed_key_is_completed_under_its_parent() {
        expect(
            "export default defineConfig({\n  test: {\n    cov‸",
            "test",
            r#"key Some("cov") present []"#,
        );
        expect(
            "export default defineConfig({ app: { router: { ‸",
            "app.router",
            "key None present []",
        );
        expect(
            "export default defineConfig({ \"fm‸",
            "",
            r#"key Some("fm") present []"#,
        );
        // A key typed on its own line before the comma of the one above it.
        expect(
            "export default defineConfig({\n  fmt: {}\n  te‸\n})",
            "",
            r#"key Some("te") present ["fmt"]"#,
        );
    }

    #[test]
    fn a_key_being_renamed_is_on_its_own_word_and_not_present() {
        expect(
            "export default defineConfig({ tes‸t: {}, fmt: {} })",
            "",
            r#"key Some("test") with colon present ["fmt"]"#,
        );
    }

    #[test]
    fn a_value_goes_after_its_colon() {
        expect(
            "export default defineConfig({ fmt: { semicolons: ‸ } })",
            "fmt.semicolons",
            "value None",
        );
        expect(
            "export default defineConfig({ fmt: { semicolons: tr‸ } })",
            "fmt.semicolons",
            r#"value Some("tr")"#,
        );
        expect(
            "export default defineConfig({ fmt: { quotes: \"‸\" } })",
            "fmt.quotes",
            r#"value Some("")"#,
        );
        expect(
            "export default defineConfig({ fmt: { quotes: \"‸",
            "fmt.quotes",
            r#"value Some("")"#,
        );
        expect(
            "export default defineConfig({ dev: { host: \"loc‸alhost\" } })",
            "dev.host",
            r#"value Some("localhost")"#,
        );
    }

    #[test]
    fn lists_and_the_maps_a_project_names_are_paths_too() {
        expect(
            "export default defineConfig({ app: { targets: [\"web\", \"‸\"] } })",
            "app.targets[]",
            r#"value Some("")"#,
        );
        expect(
            "export default defineConfig({ app: { targets: [‸] } })",
            "app.targets[]",
            "value None",
        );
        expect(
            "export default defineConfig({ plugins: [{ na‸ }] })",
            "plugins[]",
            r#"key Some("na") present []"#,
        );
        expect(
            "export default defineConfig({ tasks: { build: { comm‸ } } })",
            "tasks.build",
            r#"key Some("comm") present []"#,
        );
    }

    #[test]
    fn a_line_break_after_a_finished_entry_is_where_the_next_key_goes() {
        expect(
            "export default defineConfig({ fmt: { quotes: \"single\"\n    ‸ } })",
            "fmt",
            r#"key None present ["quotes"]"#,
        );
    }

    #[test]
    fn nothing_is_offered_where_nothing_may_be_written() {
        for marked in [
            // Same line as a finished entry, with no comma.
            "export default defineConfig({ fmt: { quotes: \"single\" ‸ } })",
            // Between a colon and the value already there.
            "export default defineConfig({ fmt: { quotes: ‸ \"single\" } })",
            // After a name with no colon on the same line.
            "export default defineConfig({ runner ‸ })",
            // In a comment, of either kind.
            "export default defineConfig({\n  // fm‸t\n})",
            "export default defineConfig({ fmt: {}, /* ‸ */ })",
            // Outside the config object.
            "import { defineConfig } from \"@uniflowed/con‸fig\";\nexport default defineConfig({});",
            "export default defineConfig({})‸;",
            // Inside a value uf reads nothing from.
            "export default defineConfig({ tasks: { a: `echo ‸` } })",
        ] {
            assert_eq!(at(marked), None, "{marked}");
        }
    }

    /// The parser's reading is exact about braces the scanner would have to
    /// lex around; both readings put this cursor in `fmt`.
    #[test]
    fn braces_inside_strings_templates_and_comments_are_not_structure() {
        expect(
            "export default defineConfig({ vite: { define: { x: `${a}}`, y: \"{\" } }, // }\n  fmt: { ‸ } })",
            "fmt",
            "key None present []",
        );
        expect(
            "export default defineConfig({ vite: { define: { x: `${a}}`, y: \"{\" } }, // }\n  fmt: { ‸",
            "fmt",
            "key None present []",
        );
    }
}
