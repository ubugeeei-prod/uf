//! One set of parse options, and the three things that keep it one.
//!
//! uf hands source to Meta's Flow parser from four places — [`uf_flow::parse`]
//! for anything that rewrites a module, [`uf_flow::validate_source`] for the
//! linter's "does this parse", [`uf_transform::estree::parse`] for the
//! transform, and `uf_check`'s checker. Each of the first three used to carry
//! its own `ParseOptions` literal, equal to the others member for member, each
//! with a comment saying it mirrored one of them. Nothing compared them.
//!
//! A drift there does not look like a bug in the parser. It looks like a file
//! that lints clean and will not format, or transforms and will not check —
//! and the reader has no way to tell which of the two commands is right.
//!
//! That is not hypothetical. The fourth entry point arrived after these tests
//! were written and was not covered by them, and it drifted: `uf_check` passed
//! the port's own `PERMISSIVE_PARSE_OPTIONS`, which turns
//! `esproposal_decorators` on, so `@decorate class Thing {}` was a file
//! `uf check` accepted and the other three refused (ubugeeei-prod/uf#430).
//!
//! So there is one constant, [`uf_flow::PARSE_OPTIONS`], and these tests are
//! what stop a second one:
//!
//! * [`every_entry_point_parses_the_same_syntax`] runs the samples through all
//!   four and requires the same answer, which is the property a reader
//!   actually depends on;
//! * [`only_uf_flow_reaches_the_port_s_parser`] reads the crates and requires
//!   that no *fifth* place calls the port's parser or builds a second option
//!   set. The list of entry points comes out of the source tree rather than
//!   out of the array above it, which is the difference between a test that
//!   covers what uf has and one that covers what someone remembered to add;
//! * [`every_option_decides_a_sample`] requires each member of `ParseOptions`
//!   to change the answer for at least one sample, which is what makes the
//!   first test complete. Without it a member could be dropped from one copy
//!   and no sample would notice.
//!
//! The load-bearing half is not any of them: [`uf_flow::module::parse`] takes
//! no options argument, so an entry point has nothing to disagree with. These
//! tests are what catches a caller that goes around it.
//!
//! This test lives in `uf_transform` because it is the crate that can see
//! every side — `uf_check` is a dev-dependency for exactly that, and for
//! nothing else.

use std::fs;
use std::path::{Path, PathBuf};

use flow_parser::ParseOptions;
use uf_check::{CheckLimits, DiagnosticKind, Source};

/// A source, and the option whose value decides what the parser makes of it.
///
/// "Decides" and not "decides whether it parses": for most of them the option
/// is the difference between a tree and a syntax error, but `f<T>(x)` and
/// `'m#./other'` parse either way and come out as different trees, which is
/// why [`fingerprint`] compares trees.
///
/// The sources are the smallest thing that reaches the option, and
/// deliberately not idiomatic uf — this `component` takes no props and returns
/// `null`, because what is under test is the grammar the parser was handed,
/// not the code anyone would write.
struct Sample {
    /// The `ParseOptions` member this source exercises.
    option: &'static str,
    /// The source, as a whole module.
    source: &'static str,
}

const SAMPLES: &[Sample] = &[
    Sample {
        option: "components",
        source: "component Greeting() { return null; }",
    },
    Sample {
        option: "enums",
        source: "enum Status { Active, Off }",
    },
    Sample {
        option: "pattern_matching",
        source: "const chosen = match (value) { 1 => 'one', _ => 'other' };",
    },
    Sample {
        option: "records",
        source: "const point = Point { x: 1, y: 2 };",
    },
    Sample {
        option: "esproposal_decorators",
        source: "@decorate class Thing {}",
    },
    Sample {
        option: "types",
        source: "const count: number = 1;",
    },
    Sample {
        option: "ambiguous_types",
        source: "const named = identity<string>(value);",
    },
    Sample {
        option: "enable_types_in_comments",
        source: "const count /*: number */ = 1;",
    },
    Sample {
        option: "use_strict",
        source: "var permissions = 0755;",
    },
    Sample {
        option: "assert_operator",
        source: "const definitely = maybe!;",
    },
    Sample {
        option: "module_ref_prefix",
        source: "const other = 'm#./other';",
    },
    Sample {
        option: "ambient",
        source: "function signatureOnly(): void;",
    },
    Sample {
        option: "allow_return_outside_function",
        source: "return 1;",
    },
];

/// What the parser makes of `source` under `options`: its errors and its tree.
///
/// Not "does it parse": several of these options change the *shape* of a tree
/// the parser accepts either way. `ambiguous_types` decides whether `f<T>(x)`
/// is a parameterized call or two comparisons, and `module_ref_prefix` decides
/// whether a string is a string or a module reference — both parse with the
/// option off, and a test that only asked whether they parsed would report
/// that neither option does anything.
fn fingerprint(options: &ParseOptions, source: &str) -> String {
    let (program, errors) =
        flow_parser::parse_program_without_file(false, None, Some(options.clone()), Ok(source));
    format!("{errors:?}\n{program:?}")
}

/// The same source through every entry point uf has, and the same answer.
#[test]
fn every_entry_point_parses_the_same_syntax() {
    for sample in SAMPLES {
        let by_parse = uf_flow::parse(sample.source)
            .expect("a sample is far under every ceiling")
            .is_ok();
        let by_validate = uf_flow::validate_source(sample.source)
            .expect("the parser backend is always available")
            .is_ok();
        // The transform reports the first syntax error as an error rather than
        // carrying diagnostics, so "accepted" is "produced a tree".
        let by_transform = uf_transform::estree::parse(sample.source).is_ok();

        assert_eq!(
            by_parse, by_validate,
            "`{}` parses differently for `uf fmt` and `uf lint`: {}",
            sample.option, sample.source
        );
        assert_eq!(
            by_parse, by_transform,
            "`{}` parses differently for `uf fmt` and the transform: {}",
            sample.option, sample.source
        );

        let Some(by_check) = parses_for_the_checker(sample.source) else {
            // A build with `uf_check`'s `upstream-typecheck` feature off has no
            // checker to compare, and therefore no fourth entry point: every
            // `uf_check` call returns `Unavailable` without reading the source.
            // The three above are still compared, which is what this test was
            // before the checker existed.
            continue;
        };
        assert_eq!(
            by_parse, by_check,
            "`{}` parses differently for `uf fmt` and `uf check`: {}",
            sample.option, sample.source
        );
    }
}

/// Whether `uf check` read `source` without a syntax error, or [`None`] when
/// this build has no checker.
///
/// The checker carries diagnostics rather than stopping at the first error, and
/// most of these samples also *fail to type check* — `Point` is not declared,
/// `identity` is not declared. `DiagnosticKind::Parse` is the question being
/// asked, so it is the only kind counted.
fn parses_for_the_checker(source: &str) -> Option<bool> {
    let checked = uf_check::check_source(
        Source::new("sample.js", source),
        // Not the wall clock: a loaded CI box is not a syntax error. The
        // samples are one line each, so no other limit is near.
        &CheckLimits::default().without_timeout(),
    );
    match checked {
        Ok(diagnostics) => Some(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == DiagnosticKind::Parse),
        ),
        Err(error) if error.is_unavailable() => None,
        Err(error) => panic!("the checker failed on a one-line sample: {error}"),
    }
}

/// Nothing but [`uf_flow::module`] reaches the port's parser or builds a second
/// option set.
///
/// [`every_entry_point_parses_the_same_syntax`] compares the entry points this
/// file knows about, and that is exactly how `uf_check` drifted: it became one
/// after the list was written, and a list nobody updated agreed with itself.
/// So the list is read out of the crates instead. A fifth caller of
/// `flow_parser`'s parse functions, or a second `ParseOptions` literal, fails
/// here by path and line the day it is written, whether or not anyone
/// remembers this file.
///
/// It reads `crates/*/src`, which is what ships. A test may reach the port
/// directly — [`fingerprint`] below does, because comparing option sets is
/// what it is for — and a test is not a command anyone runs over their code.
#[test]
fn only_uf_flow_reaches_the_port_s_parser() {
    // Each needle, and the one file allowed to contain it. `None` is "nowhere":
    // `PERMISSIVE_PARSE_OPTIONS` is the port's set for its own tooling, and
    // uf choosing it was ubugeeei-prod/uf#430.
    const OWNED: [(&str, Option<&str>); 5] = [
        ("parse_program_file", Some("uf_flow/src/module.rs")),
        ("parse_program_without_file", Some("uf_flow/src/module.rs")),
        ("parse_module_body_with_directives", None),
        ("PERMISSIVE_PARSE_OPTIONS", None),
        ("ParseOptions {", Some("uf_flow/src/parse.rs")),
    ];

    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("`crates/uf_transform` has a parent")
        .to_owned();
    let mut sources = Vec::new();
    collect_rust_sources(&crates, &mut sources);
    sources.sort();
    assert!(
        sources.len() > 1,
        "the crates were not found under {}",
        crates.display()
    );

    let mut wrong = Vec::new();
    for source in &sources {
        let relative = source
            .strip_prefix(&crates)
            .unwrap_or(source)
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(source)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source.display()));
        for (number, line) in text.lines().enumerate() {
            // Comments are prose about the parser, not a call to it — this
            // very rule is explained in three of these files by name. Anything
            // that is not a comment counts, including a line that only
            // mentions the needle in a string, which is a false positive worth
            // having: a name reached by string is a name reached.
            if line.trim_start().starts_with("//") {
                continue;
            }
            for (needle, owner) in OWNED {
                if line.contains(needle) && owner != Some(relative.as_str()) {
                    wrong.push(format!("{relative}:{}: {needle}", number + 1));
                }
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "these reach the Flow parser outside `uf_flow::module`, so they can \
         parse uf's source with options of their own:\n{}",
        wrong.join("\n")
    );
}

/// Every `.rs` file under `<crate>/src`, for [`only_uf_flow_reaches_the_port_s_parser`].
///
/// `src` and not the whole crate: `tests/` and `benches/` do not ship, and this
/// file is in one of them.
fn collect_rust_sources(crates: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(crates)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", crates.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read an entry: {error}"));
        let path = entry.path().join("src");
        if path.is_dir() {
            collect_recursively(&path, out);
        }
    }
}

fn collect_recursively(directory: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read an entry: {error}"));
        let path = entry.path();
        if path.is_dir() {
            collect_recursively(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Every member of `ParseOptions` changes the answer for at least one sample.
///
/// The point is coverage of the *struct*, not of the samples: the destructuring
/// below has no `..`, so a member added to `ParseOptions` upstream stops this
/// file compiling until someone decides what it means for uf and writes a
/// sample for it. A member nothing exercises is a member that can be dropped
/// from one copy of the options without any test noticing, which is exactly
/// how the three literals stayed equal by coincidence.
#[test]
fn every_option_decides_a_sample() {
    let base = uf_flow::PARSE_OPTIONS;
    let chosen: Vec<String> = SAMPLES
        .iter()
        .map(|sample| fingerprint(&base, sample.source))
        .collect();

    for option in members() {
        let flipped = flip(option);
        let changed: Vec<&str> = SAMPLES
            .iter()
            .zip(&chosen)
            .filter(|(sample, was)| &fingerprint(&flipped, sample.source) != *was)
            .map(|(sample, _)| sample.option)
            .collect();

        assert!(
            !changed.is_empty(),
            "no sample notices `{option}` changing, so nothing would notice it drifting"
        );
        assert!(
            changed.contains(&option),
            "`{option}` changes {changed:?} and not its own sample, which is the wrong sample"
        );
    }
}

/// Each member of `ParseOptions`, by name.
///
/// Written out rather than derived: there is no reflection here, and a list
/// that has to be kept in step by hand is only safe because [`flip`] destructures
/// the struct exhaustively and will not compile if the two disagree in length.
fn members() -> [&'static str; 13] {
    [
        "components",
        "enums",
        "pattern_matching",
        "records",
        "esproposal_decorators",
        "types",
        "ambiguous_types",
        "enable_types_in_comments",
        "use_strict",
        "assert_operator",
        "module_ref_prefix",
        "ambient",
        "allow_return_outside_function",
    ]
}

/// uf's options with one member changed away from what uf chose.
fn flip(option: &str) -> ParseOptions {
    // No `..`: adding a member upstream breaks this line, which is the point.
    let ParseOptions {
        components,
        enums,
        pattern_matching,
        records,
        esproposal_decorators,
        types,
        ambiguous_types,
        enable_types_in_comments,
        use_strict,
        assert_operator,
        module_ref_prefix,
        ambient,
        allow_return_outside_function,
    } = uf_flow::PARSE_OPTIONS;

    let mut options = ParseOptions {
        components,
        enums,
        pattern_matching,
        records,
        esproposal_decorators,
        types,
        ambiguous_types,
        enable_types_in_comments,
        use_strict,
        assert_operator,
        module_ref_prefix,
        ambient,
        allow_return_outside_function,
    };

    match option {
        "components" => options.components = !options.components,
        "enums" => options.enums = !options.enums,
        "pattern_matching" => options.pattern_matching = !options.pattern_matching,
        "records" => options.records = !options.records,
        "esproposal_decorators" => options.esproposal_decorators = !options.esproposal_decorators,
        "types" => options.types = !options.types,
        "ambiguous_types" => options.ambiguous_types = !options.ambiguous_types,
        "enable_types_in_comments" => {
            options.enable_types_in_comments = !options.enable_types_in_comments;
        }
        "use_strict" => options.use_strict = !options.use_strict,
        "assert_operator" => options.assert_operator = !options.assert_operator,
        // Not a flag: uf leaves it unset, so "changed" is any prefix at all.
        "module_ref_prefix" => options.module_ref_prefix = Some("m#".into()),
        "ambient" => options.ambient = !options.ambient,
        "allow_return_outside_function" => {
            options.allow_return_outside_function = !options.allow_return_outside_function;
        }
        other => panic!("`{other}` is not a member of ParseOptions"),
    }

    options
}
