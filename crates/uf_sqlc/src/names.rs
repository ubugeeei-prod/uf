//! Identifiers: SQL names to Flow ones.

/// Words a generated binding cannot be called: JavaScript's reserved words,
/// and Flow's contextual keywords where they would read as syntax.
const RESERVED: &[&str] = &[
    "arguments",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "component",
    "const",
    "continue",
    "debugger",
    "declare",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "hook",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "match",
    "new",
    "null",
    "opaque",
    "package",
    "private",
    "protected",
    "public",
    "renders",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Whether `name` cannot be a binding.
#[must_use]
pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name)
}

/// Split a SQL or Go-style name into lowercase words: `created_at`,
/// `CreatedAt`, `createdAt` and `CREATED_AT` all give `["created", "at"]`;
/// `ListAuthorsByID` gives `["list", "authors", "by", "id"]`.
fn words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        if !ch.is_alphanumeric() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        if ch.is_uppercase() && !current.is_empty() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|next| next.is_lowercase());
            // `aB` starts a word; so does the `B` of `ABc` (`HTTPServer` is
            // `http` + `server`), but not the `B` of `AB`.
            if prev.is_lowercase() || prev.is_ascii_digit() || (prev.is_uppercase() && next_lower) {
                out.push(std::mem::take(&mut current));
            }
        }
        current.extend(ch.to_lowercase());
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `created_at` → `createdAt`; `GetAuthor` → `getAuthor`; `ID` → `id`.
#[must_use]
pub fn camel(name: &str) -> String {
    let words = words(name);
    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        if i == 0 {
            out.push_str(word);
        } else {
            out.push_str(&capitalize(word));
        }
    }
    identifier(out)
}

/// `book_type` → `BookType`.
#[must_use]
pub fn pascal(name: &str) -> String {
    identifier(words(name).iter().map(|word| capitalize(word)).collect())
}

/// `get_author` → `GET_AUTHOR`.
#[must_use]
pub fn constant(name: &str) -> String {
    identifier(
        words(name)
            .iter()
            .map(|word| word.to_uppercase())
            .collect::<Vec<_>>()
            .join("_"),
    )
}

/// Make `name` a valid identifier: never empty and never starting with a digit.
fn identifier(name: String) -> String {
    if name.is_empty() {
        return "_".to_owned();
    }
    if name.starts_with(|ch: char| ch.is_ascii_digit()) {
        return uf_infra::into_string(uf_infra::cstr!("_{name}"));
    }
    name
}

/// A field name as written, or as camel case.
#[must_use]
pub fn field(name: &str, camel_case: bool) -> String {
    if camel_case {
        return camel(name);
    }
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '_' || ch == '$' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    identifier(cleaned)
}

/// Whether `name` can be written as an object key without quotes.
#[must_use]
pub fn is_plain_key(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_alphabetic() || ch == '_' || ch == '$')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '$')
}

const UNCOUNTABLE: &[&str] = &[
    "equipment",
    "information",
    "rice",
    "money",
    "species",
    "series",
    "fish",
    "sheep",
    "jeans",
    "police",
    "news",
    "data",
    "metadata",
    "media",
    "feedback",
    "staff",
    "status",
];

const IRREGULAR: &[(&str, &str)] = &[
    ("people", "person"),
    ("men", "man"),
    ("women", "woman"),
    ("children", "child"),
    ("teeth", "tooth"),
    ("feet", "foot"),
    ("geese", "goose"),
    ("mice", "mouse"),
    ("oxen", "ox"),
    ("moves", "move"),
];

/// Suffix rules, first match wins: `(plural ending, singular ending)`.
const SUFFIXES: &[(&str, &str)] = &[
    ("databases", "database"),
    ("quizzes", "quiz"),
    ("matrices", "matrix"),
    ("vertices", "vertex"),
    ("indices", "index"),
    ("aliases", "alias"),
    ("statuses", "status"),
    ("buses", "bus"),
    ("shoes", "shoe"),
    ("movies", "movie"),
    ("analyses", "analysis"),
    ("theses", "thesis"),
    ("crises", "crisis"),
    ("axes", "axis"),
    ("octopi", "octopus"),
    ("viri", "virus"),
    ("hives", "hive"),
    ("tives", "tive"),
    ("sses", "ss"),
    ("shes", "sh"),
    ("ches", "ch"),
    ("xes", "x"),
    ("zes", "ze"),
    ("oes", "o"),
    ("ves", "f"),
];

/// The singular of an English table name, the way sqlc-gen-go names row
/// structs: `authors` → `author`, `people` → `person`, `categories` →
/// `category`. A name that is not a recognisable plural is kept.
#[must_use]
pub fn singular(word: &str) -> String {
    let lower = word.to_lowercase();
    if UNCOUNTABLE.iter().any(|item| lower.ends_with(item)) {
        return word.to_owned();
    }
    for (plural, single) in IRREGULAR {
        if lower.ends_with(plural) {
            return uf_infra::into_string(uf_infra::cstr!(
                "{}{single}",
                &word[..word.len() - plural.len()]
            ));
        }
    }
    if let Some(stem) = lower.strip_suffix("ies")
        && stem.chars().last().is_some_and(|ch| !"aeiou".contains(ch))
    {
        return uf_infra::into_string(uf_infra::cstr!("{}y", &word[..word.len() - 3]));
    }
    for (plural, single) in SUFFIXES {
        if lower.ends_with(plural) {
            return uf_infra::into_string(uf_infra::cstr!(
                "{}{single}",
                &word[..word.len() - plural.len()]
            ));
        }
    }
    if lower.ends_with('s')
        && !lower.ends_with("ss")
        && !lower.ends_with("us")
        && !lower.ends_with("is")
        && lower.len() > 1
    {
        return word[..word.len() - 1].to_owned();
    }
    word.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_cases_sql_and_go_names() {
        assert_eq!(camel("created_at"), "createdAt");
        assert_eq!(camel("GetAuthor"), "getAuthor");
        assert_eq!(camel("ListAuthorsByID"), "listAuthorsById");
        assert_eq!(camel("HTTPServer"), "httpServer");
        assert_eq!(camel("column_1"), "column1");
        assert_eq!(camel("1st"), "_1st");
        assert_eq!(camel("ID"), "id");
    }

    #[test]
    fn pascal_cases_type_names() {
        assert_eq!(pascal("book_type"), "BookType");
        assert_eq!(pascal("people_mood"), "PeopleMood");
    }

    #[test]
    fn singularises_like_sqlc_gen_go() {
        for (plural, single) in [
            ("authors", "author"),
            ("people", "person"),
            ("categories", "category"),
            ("boxes", "box"),
            ("statuses", "status"),
            ("status", "status"),
            ("venues", "venue"),
            ("cities", "city"),
            ("pilots", "pilot"),
            ("jets", "jet"),
            ("books", "book"),
            ("address", "address"),
            ("data", "data"),
            ("user_profiles", "user_profile"),
        ] {
            assert_eq!(singular(plural), single, "{plural}");
        }
    }
}
