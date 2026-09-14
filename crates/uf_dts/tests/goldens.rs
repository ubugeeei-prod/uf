//! The translation, pinned case by case.
//!
//! Every directory under `tests/goldens` is one case: `input.d.ts` is
//! translated, with any other file in the directory readable as part of the
//! same package, and the Flow printed for `input.d.ts` must equal
//! `expected.js.flow` and its holes `expected.holes.json`.
//!
//! The expectations are reviewed output, not generated noise. Set
//! `UF_DTS_BLESS=1` to rewrite them from the current translation, then read
//! the diff before committing it: an expectation that changed is a change to
//! what every uf project is typed against. `uf_check`'s
//! `tests/declaration_goldens.rs` checks each expectation with Flow itself, so
//! a golden that Flow would reject cannot be blessed into place.

use std::fs;
use std::path::{Path, PathBuf};

use uf_dts::{Construct, Hole};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn cases() -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = fs::read_dir(goldens())
        .expect("the goldens directory exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("input.d.ts").is_file())
        .collect();
    cases.sort();
    assert!(
        cases.len() >= 15,
        "expected a broad golden corpus, found {} cases in {}",
        cases.len(),
        goldens().display()
    );
    cases
}

fn blessing() -> bool {
    std::env::var_os("UF_DTS_BLESS").is_some()
}

/// The translation of `input.d.ts` in `case`, as the two files it is compared
/// with.
fn translate(case: &Path) -> (String, Vec<Hole>) {
    let translation = uf_dts::translate(&["input.d.ts"], &mut |path| {
        fs::read_to_string(case.join(path)).ok()
    });
    let module = translation
        .modules
        .into_iter()
        .find(|module| module.path == "input.d.ts")
        .unwrap_or_else(|| panic!("{} produced no module for input.d.ts", case.display()));
    (module.flow.unwrap_or_default(), module.holes)
}

fn holes_json(holes: &[Hole]) -> String {
    let mut json = serde_json::to_string_pretty(holes).expect("holes serialize");
    json.push('\n');
    json
}

#[test]
fn every_case_translates_to_its_expectation() {
    for case in cases() {
        let (flow, holes) = translate(&case);
        let holes = holes_json(&holes);
        let flow_path = case.join("expected.js.flow");
        let holes_path = case.join("expected.holes.json");
        if blessing() {
            fs::write(&flow_path, &flow).expect("the expectation is writable");
            fs::write(&holes_path, &holes).expect("the expectation is writable");
            continue;
        }
        let name = case.file_name().unwrap_or_default().to_string_lossy();
        let expected_flow = fs::read_to_string(&flow_path)
            .unwrap_or_else(|_| panic!("{name} has no expected.js.flow; bless it"));
        let expected_holes = fs::read_to_string(&holes_path)
            .unwrap_or_else(|_| panic!("{name} has no expected.holes.json; bless it"));
        similar_asserts::assert_eq!(expected_flow, flow, "{name}: the Flow changed");
        similar_asserts::assert_eq!(expected_holes, holes, "{name}: the holes changed");
    }
    assert!(
        !blessing(),
        "expectations were rewritten; review the diff and rerun without UF_DTS_BLESS"
    );
}

/// The names an output line declares, by the keyword before each one.
///
/// Deliberately simple: the goldens are the only input, and a word after
/// `type`, `interface`, `class`, `function`, `const`, `let`, `var`, `enum` or
/// `namespace` that is not `{` is a declared name in all of them. A name the
/// translation made up — `ZodType$value`, `$Default` — is traced back to the
/// name it stands for, or skipped when it stands for none.
fn declared_names(line: &str) -> Vec<String> {
    const KEYWORDS: [&str; 9] = [
        "type",
        "interface",
        "class",
        "function",
        "const",
        "let",
        "var",
        "enum",
        "namespace",
    ];
    let words: Vec<&str> = line.split_whitespace().collect();
    let mut names = Vec::new();
    for (index, word) in words.iter().enumerate() {
        // What follows `declare module.exports` on its line is the namespace's
        // types re-declared for importers, which the source never wrote there.
        if *word == "module.exports:" {
            break;
        }
        if !KEYWORDS.contains(word) {
            continue;
        }
        // `import type { … }` and `export type { … }` declare nothing.
        if index > 0 && words[index - 1] == "import" {
            continue;
        }
        let Some(next) = words.get(index + 1) else {
            continue;
        };
        let name: String = next
            .chars()
            .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '$'))
            .collect();
        let name = name
            .strip_suffix("$value")
            .or_else(|| name.strip_suffix("$type"))
            .unwrap_or(&name)
            .to_owned();
        if name.is_empty() || name.starts_with('$') {
            continue;
        }
        names.push(name);
    }
    names
}

#[test]
fn every_translated_declaration_is_on_its_source_line() {
    // A location in a translation is read as a location in the declaration
    // file, so a declaration the translation prints must be printed on the
    // line its name is written on — the printer pads forward to it, and never
    // prints anything ahead of where it was written.
    for case in cases() {
        let source = fs::read_to_string(case.join("input.d.ts")).expect("readable");
        let source: Vec<&str> = source.lines().collect();
        let (flow, _) = translate(&case);
        for (index, line) in flow.lines().enumerate() {
            for name in declared_names(line) {
                assert!(
                    source
                        .get(index)
                        .is_some_and(|written| written.contains(&name)),
                    "{}: output line {} declares `{name}`, which source line {} does not name",
                    case.display(),
                    index + 1,
                    index + 1
                );
            }
        }
    }
}

#[test]
fn a_declared_name_is_read_off_a_line_the_way_the_invariant_needs() {
    assert_eq!(
        declared_names(
            "declare const ZodType$value: $constructor<ZodType>; export { ZodType$value as ZodType };"
        ),
        ["ZodType"]
    );
    assert_eq!(
        declared_names("import type { Options } from \"./model.d.ts.flow\";"),
        Vec::<String>::new()
    );
    assert_eq!(
        declared_names(
            "export type { Kind } from \"./types.d.ts.flow\"; export type Lazy = $Import0.Kind;"
        ),
        ["Lazy"]
    );
    assert_eq!(
        declared_names("declare export default class $Default {"),
        Vec::<String>::new()
    );
}

#[test]
fn every_construct_that_can_be_a_hole_has_a_golden_showing_it() {
    let mut seen = std::collections::BTreeSet::new();
    for case in cases() {
        let Ok(json) = fs::read_to_string(case.join("expected.holes.json")) else {
            continue;
        };
        let holes: Vec<Hole> = serde_json::from_str(&json).expect("expected holes parse");
        seen.extend(holes.into_iter().map(|hole| hole.construct));
    }
    let missing: Vec<&str> = Construct::ALL
        .into_iter()
        // A file that does not parse has no Flow to pin, so the unit tests in
        // `unit.rs` cover it instead of a golden.
        .filter(|construct| *construct != Construct::ParseError)
        .filter(|construct| !seen.contains(construct))
        .map(Construct::as_str)
        .collect();
    assert!(
        missing.is_empty() || blessing(),
        "no golden shows these holes: {missing:?}"
    );
}

#[test]
fn a_translation_is_a_function_of_the_files_it_reads() {
    for case in cases() {
        assert_eq!(translate(&case), translate(&case), "{}", case.display());
    }
}
