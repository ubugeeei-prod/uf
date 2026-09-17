//! The translation, pinned case by case.
//!
//! Every directory under `tests/goldens` is one case: `input.js` is
//! translated, with any other file in the directory readable as part of the
//! same project, and the TypeScript printed for `input.js` must equal
//! `expected.d.ts` and its gaps `expected.gaps.json`.
//!
//! The expectations are reviewed output, not generated noise. Set
//! `UF_DECLARE_BLESS=1` to rewrite them from the current translation, then
//! read the diff before committing it: an expectation that changed is a
//! change to the declarations every uf library publishes.
//!
//! # Why one of these tests runs a parser
//!
//! [`every_declaration_file_parses_as_typescript`] is the check that a golden
//! cannot be blessed past. A `.d.ts` is read by a compiler that never sees the
//! code it describes, so the failure mode this crate has to design against is
//! not "the types are imprecise" but "the file does not compile" — and a test
//! that compares text to text would call that a pass as long as the text
//! matched. oxc's TypeScript parser is already in the workspace, so the
//! emitted file is parsed back with it, which is the same move `uf_check`'s
//! `declaration_goldens.rs` makes in the other direction.

use std::fs;
use std::path::{Path, PathBuf};

use uf_declare::{Construct, Gap};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

fn cases() -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = fs::read_dir(goldens())
        .expect("the goldens directory exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("input.js").is_file())
        .collect();
    cases.sort();
    assert!(
        cases.len() >= 10,
        "expected a broad golden corpus, found {} cases in {}",
        cases.len(),
        goldens().display()
    );
    cases
}

fn blessing() -> bool {
    std::env::var_os("UF_DECLARE_BLESS").is_some()
}

/// The translation of `input.js` in `case`, as the two files it is compared
/// with.
fn translate(case: &Path) -> (String, Vec<Gap>) {
    let root = case.to_path_buf();
    let mut read = move |path: &str| fs::read_to_string(root.join(path)).ok();
    let translation = uf_declare::translate(&["input.js"], &mut read);
    let module = translation
        .modules
        .into_iter()
        .find(|module| module.path == "input.js")
        .unwrap_or_else(|| panic!("{} produced no module for input.js", case.display()));
    (module.text.unwrap_or_default(), module.gaps)
}

fn gaps_json(gaps: &[Gap]) -> String {
    let mut json = serde_json::to_string_pretty(gaps).expect("gaps serialize");
    json.push('\n');
    json
}

#[test]
fn every_case_translates_to_its_expectation() {
    for case in cases() {
        let (declarations, gaps) = translate(&case);
        let gaps = gaps_json(&gaps);
        let declaration_path = case.join("expected.d.ts");
        let gaps_path = case.join("expected.gaps.json");
        if blessing() {
            fs::write(&declaration_path, &declarations).expect("the expectation is writable");
            fs::write(&gaps_path, &gaps).expect("the expectation is writable");
            continue;
        }
        let name = case.file_name().unwrap_or_default().to_string_lossy();
        let expected_declarations = fs::read_to_string(&declaration_path)
            .unwrap_or_else(|_| panic!("{name} has no expected.d.ts; bless it"));
        let expected_gaps = fs::read_to_string(&gaps_path)
            .unwrap_or_else(|_| panic!("{name} has no expected.gaps.json; bless it"));
        similar_asserts::assert_eq!(
            expected_declarations,
            declarations,
            "{name}: the declarations changed"
        );
        similar_asserts::assert_eq!(expected_gaps, gaps, "{name}: the gaps changed");
    }
    assert!(
        !blessing(),
        "expectations were rewritten; review the diff and rerun without UF_DECLARE_BLESS"
    );
}

/// Every emitted declaration file is one TypeScript can read.
///
/// The invariant that matters most, and the one text comparison cannot give.
/// A `.d.ts` with a syntax error in it does not degrade a consumer's types —
/// it stops their build, in a file they did not write and cannot fix.
#[test]
fn every_declaration_file_parses_as_typescript() {
    for case in cases() {
        let (declarations, _) = translate(&case);
        let allocator = oxc_allocator::Allocator::default();
        let parsed =
            oxc_parser::Parser::new(&allocator, &declarations, oxc_span::SourceType::d_ts())
                .parse();
        assert!(
            !parsed.fatal_error && parsed.diagnostics.is_empty(),
            "{}: the declarations do not parse as TypeScript:\n{declarations}\n{:?}",
            case.display(),
            parsed
                .diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        );
    }
}

/// Every construct a gap can name is shown by a golden.
///
/// A construct nothing exercises is a claim about Flow that nothing checks:
/// the reason string could be wrong, the line could be wrong, or the arm
/// could be unreachable. Adding a variant means adding the case that produces
/// it.
#[test]
fn every_construct_that_can_be_a_gap_has_a_golden_showing_it() {
    let mut seen = std::collections::BTreeSet::new();
    for case in cases() {
        let Ok(json) = fs::read_to_string(case.join("expected.gaps.json")) else {
            continue;
        };
        let gaps: Vec<Gap> = serde_json::from_str(&json).expect("expected gaps parse");
        seen.extend(gaps.into_iter().map(|gap| gap.construct));
    }
    let missing: Vec<&str> = Construct::ALL
        .into_iter()
        // A module that does not parse has no declarations to pin, so the
        // unit tests in `unit.rs` cover it instead of a golden.
        .filter(|construct| *construct != Construct::ParseError)
        // The same: a missing file is a fact about the project around the
        // module rather than about anything written in it.
        .filter(|construct| *construct != Construct::MissingFile)
        .filter(|construct| !seen.contains(construct))
        .map(Construct::as_str)
        .collect();
    assert!(
        missing.is_empty() || blessing(),
        "no golden shows these gaps: {missing:?}"
    );
}

#[test]
fn a_translation_is_a_function_of_the_modules_it_reads() {
    for case in cases() {
        assert_eq!(translate(&case), translate(&case), "{}", case.display());
    }
}
