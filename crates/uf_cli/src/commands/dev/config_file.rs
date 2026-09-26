//! Completion and hover in `uf.config.js`.
//!
//! # Where the answers come from
//!
//! Every key, its documentation, its type, and the values a key accepts come
//! from `@uniflowed/config`'s Flow type, read by [`uf_config::schema`]. That is
//! the reader `crates/uf_config/tests/flow_schema.rs` holds to the loader, so a
//! key offered here is a key uf reads, and the words shown for it are the ones
//! written above it in `packages/config/internal/schema.js`. The schema is
//! compiled into the binary and parsed once per process, by the first request
//! that needs it.
//!
//! # Where the cursor is
//!
//! [`outline`] reads the document's config object — with the Flow parser when
//! the document parses, with a scanner when it is half-typed and does not —
//! and [`cursor`] turns an offset into a path from that object and a site: a
//! key, or a value. Neither needs the file to be valid, which is the whole
//! difficulty: the moment somebody wants a completion is the moment the file
//! is least likely to be a program.
//!
//! # What is offered
//!
//! * **Keys**: those the schema declares at the path, less the ones the object
//!   already has, each with its documentation and its type.
//! * **Values**: the members of a string-, number- or boolean-literal union,
//!   `true` and `false` for `boolean`, `null` where the type allows it. Inside
//!   quotes only strings are offered, because `"true"` is not a boolean. A
//!   string written without quotes is written in the project's own
//!   `fmt.quotes`.
//! * **Tool specs**, in a key typed by one of `@uniflowed/config`'s four spec
//!   aliases: the names the key takes, and after the `@` that tool's versions,
//!   newest first, from the release list `uf_env` caches. See [`tools`], and
//!   [`releases`] for why a request never waits for a list.
//!
//! # What is not
//!
//! * Anything under `vite`. Its type is a map of `mixed` on purpose — uf does
//!   not re-declare Vite's options (red line 2 in `docs/red-lines.md`) — so
//!   there is nothing here to offer, and an invented list would be the
//!   re-declaration that key exists to avoid.
//! * Values of a free `string` or `number`. There is no list, and a guess is
//!   worse than nothing.
//! * Any file but `uf.config.js`, `uf.config.mjs` and `uf.config.cjs`. The
//!   trigger characters arrive from every file the editor hands this server,
//!   and every other file is answered with nothing, as fast as comparing a
//!   file name can say so.

mod cursor;
pub(crate) mod outline;
mod releases;
mod tools;

use uf_config::QuoteStyle;
use uf_config::schema::{Key, Schema, Shape, Step};

use cursor::Site;
use outline::{Span, Word};
pub(super) use releases::ReleaseLists;
use releases::Releases;

/// The names a config file answers completion under.
const FILE_NAMES: [&str; 3] = ["uf.config.js", "uf.config.mjs", "uf.config.cjs"];

/// Whether `path` names a config file.
pub(super) fn is_config_file(path: &str) -> bool {
    path.rsplit(['/', '\\'])
        .next()
        .is_some_and(|name| FILE_NAMES.contains(&name))
}

/// What a completion item is, for the icon an editor puts beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A key.
    Key,
    /// A member of a literal union.
    Literal,
    /// `true`, `false` or `null`.
    Constant,
    /// A version of a tool, after the `@` of a spec.
    Version,
}

impl Kind {
    /// The protocol's `CompletionItemKind`: `Property`, `EnumMember`,
    /// `Constant`, `Value`.
    pub(super) fn protocol(self) -> u8 {
        match self {
            Self::Key => 10,
            Self::Literal => 20,
            Self::Constant => 21,
            Self::Version => 12,
        }
    }
}

/// What completion answers.
#[derive(Debug, Default)]
pub(super) struct Completion {
    pub(super) items: Vec<Item>,
    /// Whether the list is not the whole answer yet: a tool's release list is
    /// being fetched, so the editor should ask again as the user types rather
    /// than filter this one.
    pub(super) incomplete: bool,
}

/// One completion, in byte offsets; the protocol layer turns them into UTF-16.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Item {
    /// What the list shows.
    pub(super) label: String,
    pub(super) kind: Kind,
    /// The key's type, on one line.
    pub(super) detail: Option<String>,
    /// Markdown.
    pub(super) documentation: Option<String>,
    /// The bytes accepting the item replaces.
    pub(super) replace: Span,
    /// What it replaces them with.
    pub(super) new_text: String,
    /// What an editor filters on, when that is not the label.
    pub(super) filter_text: Option<String>,
    /// The order to show values in, which is the order the schema declares
    /// them in: `"off" | "warn" | "error"` means something in that order.
    pub(super) sort_text: Option<String>,
}

/// A hover over a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hover {
    /// Markdown.
    pub(super) markdown: String,
    /// The key, quotes included.
    pub(super) span: Span,
}

/// A document's config object, read once per text.
///
/// Opaque outside this module: the protocol layer keeps one beside each open
/// document, the way it keeps the diagnostics, and hands it back with every
/// question about that text. Reading it runs the Flow parser over the whole
/// file, and a hover is asked on mouse-move.
#[derive(Debug)]
pub(super) struct Outline(Option<outline::Object>);

impl Outline {
    /// The config object `source` exports, if it exports one.
    pub(super) fn read(source: &str) -> Self {
        Self(outline::read(source))
    }
}

/// What may be written at `offset`, in the `source` that `outline` was read
/// from. A tool's versions come from `releases`, which never waits.
pub(super) fn complete(
    schema: &Schema,
    source: &str,
    outline: &Outline,
    offset: usize,
    quotes: QuoteStyle,
    releases: &mut dyn Releases,
) -> Completion {
    let Some(root) = &outline.0 else {
        return Completion::default();
    };
    let Some(cursor) = cursor::locate(source, root, offset) else {
        return Completion::default();
    };
    match cursor.site {
        Site::Key {
            word,
            colon,
            present,
        } => Completion {
            items: keys(schema, &cursor.path, offset, word, colon, &present),
            incomplete: false,
        },
        Site::Value { word } => {
            let mut completion = Completion {
                items: values(schema, &cursor.path, offset, word, quotes),
                incomplete: false,
            };
            let shapes = schema.resolve(&cursor.path);
            if let Some(role) = tools::Role::of(&shapes) {
                let described = described(schema, &cursor.path, offset, word);
                tools::complete(
                    tools::Request {
                        role,
                        described: &described,
                        source,
                        offset,
                        word,
                        quotes,
                        releases,
                    },
                    &mut completion,
                );
            }
            completion
        }
    }
}

/// The key under the character at `offset`, explained.
pub(super) fn hover(
    schema: &Schema,
    source: &str,
    outline: &Outline,
    offset: usize,
) -> Option<Hover> {
    let root = outline.0.as_ref()?;
    // A hover names the character under the pointer, and a cursor names the
    // gap between two characters; the gap after this character is the one on
    // the same word.
    let after = offset + source.get(offset..)?.chars().next()?.len_utf8();
    let cursor = cursor::locate(source, root, after)?;
    let Site::Key {
        word: Some(word), ..
    } = cursor.site
    else {
        return None;
    };

    let mut path = cursor.path;
    path.push(Step::Key(word.text(source)));
    let key = schema.key(&path)?;
    let mut markdown = uf_infra::into_string(uf_infra::cstr!(
        "**`{}`** · `{}`",
        dotted(&path),
        key.type_text()
    ));
    if let Some(documentation) = key.documentation() {
        markdown.push_str("\n\n");
        markdown.push_str(documentation);
    }
    Some(Hover {
        markdown,
        span: word.span,
    })
}

fn keys(
    schema: &Schema,
    path: &[Step<'_>],
    offset: usize,
    word: Option<Word>,
    colon: bool,
    present: &[&str],
) -> Vec<Item> {
    let replace = word.map_or(Span::at(offset), |word| word.contents());
    let mut offered: Vec<&str> = Vec::new();
    let mut items = Vec::new();

    for object in schema.resolve(path).into_iter().flat_map(Shape::objects) {
        for key in object.keys() {
            if present.contains(&key.name()) || offered.contains(&key.name()) {
                continue;
            }
            offered.push(key.name());
            items.push(Item {
                label: key.name().to_owned(),
                kind: Kind::Key,
                detail: Some(key.type_text().to_owned()),
                documentation: key.documentation().map(str::to_owned),
                replace,
                new_text: key_text(key.name(), word, colon),
                filter_text: None,
                sort_text: None,
            });
        }
    }
    items
}

/// What accepting a key writes.
///
/// A bare name gets its `: ` when it has none yet, so the next thing typed is
/// the value — and a `"` typed there is a trigger that asks for the values. A
/// name inside quotes is only the name, plus the closing quote and the colon
/// when the quote is still open.
fn key_text(name: &str, word: Option<Word>, colon: bool) -> String {
    let mut text = name.to_owned();
    let open_quote = word
        .filter(|word| !word.terminated)
        .and_then(|word| word.quote);
    match (word.and_then(|word| word.quote), open_quote) {
        (Some(_), None) => return text,
        (Some(_), Some(quote)) => text.push(char::from(quote)),
        (None, _) => {}
    }
    if !colon {
        text.push_str(": ");
    }
    text
}

fn values(
    schema: &Schema,
    path: &[Step<'_>],
    offset: usize,
    word: Option<Word>,
    quotes: QuoteStyle,
) -> Vec<Item> {
    let described = described(schema, path, offset, word);
    let quoted = word.and_then(|word| word.quote);
    let open_quote = word
        .filter(|word| !word.terminated)
        .and_then(|word| word.quote);
    let mut items = Vec::new();

    for member in schema.resolve(path).into_iter().flat_map(Shape::members) {
        match (member, quoted) {
            (Shape::StringLiteral(value), Some(quote)) => {
                let mut text = escape(value, quote);
                text.extend(open_quote.map(char::from));
                offer(
                    &mut items,
                    &described,
                    value.to_string(),
                    text,
                    None,
                    Kind::Literal,
                );
            }
            (Shape::StringLiteral(value), None) => {
                let quote = quote_byte(quotes);
                let written = uf_infra::cstr!("{0}{1}{0}", char::from(quote), escape(value, quote))
                    .into_string();
                offer(
                    &mut items,
                    &described,
                    written.clone(),
                    written,
                    Some(value.to_string()),
                    Kind::Literal,
                );
            }
            (Shape::NumberLiteral(raw), None) => {
                offer(
                    &mut items,
                    &described,
                    raw.to_string(),
                    raw.to_string(),
                    None,
                    Kind::Literal,
                );
            }
            (Shape::BooleanLiteral(value), None) => {
                let text = value.to_string();
                offer(
                    &mut items,
                    &described,
                    text.clone(),
                    text,
                    None,
                    Kind::Constant,
                );
            }
            (Shape::Boolean, None) => {
                for text in ["true", "false"] {
                    offer(
                        &mut items,
                        &described,
                        text.into(),
                        text.into(),
                        None,
                        Kind::Constant,
                    );
                }
            }
            (Shape::Null, None) => {
                offer(
                    &mut items,
                    &described,
                    "null".into(),
                    "null".into(),
                    None,
                    Kind::Constant,
                );
            }
            _ => {}
        }
    }
    items
}

/// What every value offered for one key shares.
struct Described {
    detail: Option<String>,
    documentation: Option<String>,
    replace: Span,
}

/// The key at `path`'s type and documentation, and the word a value replaces.
fn described(schema: &Schema, path: &[Step<'_>], offset: usize, word: Option<Word>) -> Described {
    let key = schema.key(path);
    Described {
        detail: key.map(|key| key.type_text().to_owned()),
        documentation: key.and_then(Key::documentation).map(str::to_owned),
        replace: word.map_or(Span::at(offset), |word| word.contents()),
    }
}

/// The quote a string written without one is written in.
fn quote_byte(quotes: QuoteStyle) -> u8 {
    match quotes {
        QuoteStyle::Single => b'\'',
        QuoteStyle::Double => b'"',
    }
}

fn offer(
    items: &mut Vec<Item>,
    described: &Described,
    label: String,
    new_text: String,
    filter_text: Option<String>,
    kind: Kind,
) {
    if items.iter().any(|item| item.new_text == new_text) {
        return;
    }
    let sort_text = Some(uf_infra::into_string(uf_infra::cstr!("{:04}", items.len())));
    items.push(Item {
        label,
        kind,
        detail: described.detail.clone(),
        documentation: described.documentation.clone(),
        replace: described.replace,
        new_text,
        filter_text,
        sort_text,
    });
}

/// A string's contents, escaped for the quote it is written inside.
fn escape(value: &str, quote: u8) -> String {
    let mut escaped = String::with_capacity(value.len());
    for letter in value.chars() {
        if letter == '\\' || letter == char::from(quote) {
            escaped.push('\\');
        }
        escaped.push(letter);
    }
    escaped
}

/// A path the way the configuration reference writes one: `test.coverage`,
/// `plugins[].name`.
fn dotted(path: &[Step<'_>]) -> String {
    let mut out = String::new();
    for step in path {
        match step {
            Step::Key(name) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(name);
            }
            Step::Element => out.push_str("[]"),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Completion at `‸`, with the double-quote style.
    fn complete_at(marked: &str) -> (String, Vec<Item>) {
        complete_with(marked, QuoteStyle::Double)
    }

    fn complete_with(marked: &str, quotes: QuoteStyle) -> (String, Vec<Item>) {
        let offset = marked.find('‸').expect("a cursor");
        let source = marked.replacen('‸', "", 1);
        let outline = Outline::read(&source);
        let completion = complete(
            Schema::embedded(),
            &source,
            &outline,
            offset,
            quotes,
            &mut NoReleases,
        );
        (source, completion.items)
    }

    /// No release lists, and none coming: keys and values need none.
    struct NoReleases;

    impl Releases for NoReleases {
        fn lookup(&mut self, _: uf_env::Tool) -> releases::Lookup<'_> {
            releases::Lookup::Unavailable
        }
    }

    /// The hover for the character at `at`.
    fn hover_on(source: &str, at: usize) -> Option<Hover> {
        hover(Schema::embedded(), source, &Outline::read(source), at)
    }

    fn labels(items: &[Item]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    fn item<'a>(items: &'a [Item], label: &str) -> &'a Item {
        items
            .iter()
            .find(|item| item.label == label)
            .unwrap_or_else(|| panic!("no `{label}` in {:?}", labels(items)))
    }

    /// Accept an item the way an editor does.
    fn accept(source: &str, item: &Item) -> String {
        uf_infra::cstr!(
            "{}{}{}",
            &source[..item.replace.start],
            item.new_text,
            &source[item.replace.end..]
        )
        .into_string()
    }

    #[test]
    fn the_keys_of_an_object_are_offered_with_their_documentation_and_type() {
        let (source, items) = complete_at("export default defineConfig({ fmt: {}, ‸ })");

        assert!(labels(&items).contains(&"test"), "{:?}", labels(&items));
        // Already written, so not offered again.
        assert!(!labels(&items).contains(&"fmt"));

        let ignore = item(&items, "ignore");
        assert_eq!(ignore.kind, Kind::Key);
        assert_eq!(ignore.detail.as_deref(), Some("$ReadOnlyArray<string>"));
        assert!(
            ignore
                .documentation
                .as_deref()
                .is_some_and(|text| text.contains("Paths no command walks into")),
            "{ignore:?}"
        );
        assert_eq!(
            accept(&source, ignore),
            "export default defineConfig({ fmt: {}, ignore:  })"
        );
    }

    #[test]
    fn a_half_typed_key_is_replaced_whole() {
        let (source, items) = complete_at("export default defineConfig({\n  test: {\n    cov‸");

        let coverage = item(&items, "coverage");
        assert!(
            coverage
                .documentation
                .as_deref()
                .is_some_and(|text| text.contains("What `uf test --coverage` measures")),
            "{coverage:?}"
        );
        assert_eq!(
            accept(&source, coverage),
            "export default defineConfig({\n  test: {\n    coverage: "
        );
    }

    #[test]
    fn a_key_keeps_the_colon_and_the_quotes_it_already_has() {
        let (source, items) =
            complete_at("export default defineConfig({ fmt: { quo‸tes: \"single\" } })");
        assert_eq!(
            accept(&source, item(&items, "quotes")),
            "export default defineConfig({ fmt: { quotes: \"single\" } })"
        );

        let (source, items) = complete_at("export default defineConfig({ \"fm‸");
        assert_eq!(
            accept(&source, item(&items, "fmt")),
            "export default defineConfig({ \"fmt\": "
        );

        let (source, items) = complete_at("export default defineConfig({ \"fm‸\" })");
        assert_eq!(
            accept(&source, item(&items, "fmt")),
            "export default defineConfig({ \"fmt\" })"
        );
    }

    #[test]
    fn a_string_union_offers_its_members_in_declaration_order() {
        let (source, items) =
            complete_at("export default defineConfig({ fmt: { quotes: \"‸\" } })");
        assert_eq!(labels(&items), ["single", "double"]);
        assert_eq!(items[0].kind, Kind::Literal);
        assert_eq!(items[0].detail.as_deref(), Some(r#""single" | "double""#));
        assert_eq!(
            accept(&source, &items[1]),
            "export default defineConfig({ fmt: { quotes: \"double\" } })"
        );
        assert!(items[0].sort_text < items[1].sort_text);

        // No closing quote yet: accepting writes it.
        let (source, items) = complete_at("export default defineConfig({ fmt: { quotes: \"si‸");
        assert_eq!(
            accept(&source, &items[0]),
            "export default defineConfig({ fmt: { quotes: \"single\""
        );

        // No quotes at all: written in the project's quote style.
        let (source, items) = complete_with(
            "export default defineConfig({ fmt: { quotes: ‸ } })",
            QuoteStyle::Single,
        );
        assert_eq!(labels(&items), ["'single'", "'double'"]);
        assert_eq!(items[0].filter_text.as_deref(), Some("single"));
        assert_eq!(
            accept(&source, &items[0]),
            "export default defineConfig({ fmt: { quotes: 'single' } })"
        );
    }

    /// `lint.rules` is a map whose keys are rule ids, and its value type,
    /// `RuleLevel`, mixes every kind of literal a value can be: strings,
    /// numbers, and `boolean`.
    #[test]
    fn a_rule_level_offers_every_kind_of_literal_it_declares() {
        let (_, items) = complete_at(
            "export default defineConfig({ lint: { rules: { \"flow/unclear-type\": ‸ } } })",
        );
        assert_eq!(
            labels(&items),
            [
                "\"off\"",
                "\"warn\"",
                "\"error\"",
                "0",
                "1",
                "2",
                "true",
                "false"
            ]
        );
        assert_eq!(items[3].kind, Kind::Literal);
        assert_eq!(items[6].kind, Kind::Constant);
        // In declaration order, which is severity order.
        assert!(
            items
                .windows(2)
                .all(|pair| pair[0].sort_text < pair[1].sort_text)
        );
    }

    #[test]
    fn a_boolean_offers_true_and_false_and_nothing_inside_quotes() {
        let (_, items) = complete_at("export default defineConfig({ fmt: { semicolons: ‸ } })");
        assert_eq!(labels(&items), ["true", "false"]);
        assert_eq!(items[0].kind, Kind::Constant);

        let (_, items) = complete_at("export default defineConfig({ fmt: { semicolons: \"‸\" } })");
        assert!(items.is_empty(), "{:?}", labels(&items));
    }

    #[test]
    fn an_element_of_a_list_offers_the_members_of_its_element_type() {
        let (_, items) =
            complete_at("export default defineConfig({ app: { targets: [\"web\", \"‸\"] } })");
        assert!(
            labels(&items).contains(&"react-native"),
            "{:?}",
            labels(&items)
        );
        assert!(
            item(&items, "hermes")
                .detail
                .as_deref()
                .is_some_and(|detail| detail.starts_with("$ReadOnlyArray<")),
        );
    }

    #[test]
    fn the_keys_of_a_map_a_project_names_come_from_its_value_type() {
        let (_, items) = complete_at("export default defineConfig({ tasks: { build: { ‸ } } })");
        assert!(labels(&items).contains(&"command"), "{:?}", labels(&items));
        assert!(labels(&items).contains(&"dependsOn"));
        assert!(labels(&items).contains(&"args"));
    }

    #[test]
    fn a_task_argument_offers_its_keys_and_their_documentation() {
        let (_, items) = complete_at(
            "export default defineConfig({ tasks: { deploy: { command: \"x\", args: [{ ‸ }] } } })",
        );
        for key in ["name", "description", "choices", "default", "required"] {
            assert!(labels(&items).contains(&key), "{key}: {:?}", labels(&items));
        }
        assert!(
            item(&items, "choices")
                .documentation
                .as_deref()
                .is_some_and(|text| text.contains("the list to pick from")),
            "{:?}",
            item(&items, "choices")
        );

        let (_, items) = complete_at(
            "export default defineConfig({ tasks: { deploy: { args: [{ required: ‸ }] } } })",
        );
        assert_eq!(labels(&items), ["true", "false"]);
    }

    /// Red line 2: uf does not re-declare Vite's options, so it has none to
    /// offer, and an empty list is the honest answer.
    #[test]
    fn nothing_is_offered_under_vite_or_for_a_free_string() {
        let (_, items) = complete_at("export default defineConfig({ vite: { ‸ } })");
        assert!(items.is_empty(), "{:?}", labels(&items));

        let (_, items) = complete_at("export default defineConfig({ dev: { host: \"‸\" } })");
        assert!(items.is_empty(), "{:?}", labels(&items));
    }

    #[test]
    fn hovering_a_key_shows_what_completion_shows() {
        let source = "export default defineConfig({ test: { coverage: {} } })";
        let offset = source.find("coverage").unwrap();

        for at in [offset, offset + 4, offset + "coverage".len() - 1] {
            let hover = hover_on(source, at).expect("a hover");
            assert!(
                hover.markdown.starts_with("**`test.coverage`** · `{ … }`"),
                "{}",
                hover.markdown
            );
            assert!(
                hover
                    .markdown
                    .contains("What `uf test --coverage` measures")
            );
            assert_eq!(&source[hover.span.start..hover.span.end], "coverage");
        }

        // Not a key: the value, the punctuation after the key, a key nobody
        // declared.
        assert!(hover_on(source, offset + "coverage".len()).is_none());
        let source = "export default defineConfig({ fmt: { quotes: \"single\" }, nope: 1 })";
        assert!(hover_on(source, source.find("single").unwrap()).is_none());
        assert!(hover_on(source, source.find("nope").unwrap()).is_none());
    }

    #[test]
    fn only_a_config_file_is_one() {
        for path in [
            "/p/uf.config.js",
            "/p/uf.config.mjs",
            "/p/uf.config.cjs",
            "uf.config.js",
            "C:\\p\\uf.config.js",
        ] {
            assert!(is_config_file(path), "{path}");
        }
        for path in [
            "/p/my.uf.config.js",
            "/p/uf.config.ts",
            "/p/uf.config.js/x.js",
            "/p/app.js",
        ] {
            assert!(!is_config_file(path), "{path}");
        }
    }
}
