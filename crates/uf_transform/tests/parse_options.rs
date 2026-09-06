//! One set of parse options, and the two things that keep it one.
//!
//! uf hands source to Meta's Flow parser from three places — [`uf_flow::parse`]
//! for anything that rewrites a module, [`uf_flow::validate_source`] for the
//! linter's "does this parse", and [`uf_transform::estree::parse`] for the
//! transform. Each used to carry its own `ParseOptions` literal, equal to the
//! others member for member, each with a comment saying it mirrored one of
//! them. Nothing compared them.
//!
//! A drift there does not look like a bug in the parser. It looks like a file
//! that lints clean and will not format, or transforms and will not check —
//! and the reader has no way to tell which of the two commands is right. So
//! there is one constant now, and these tests are what stop a second one:
//!
//! * [`every_entry_point_parses_the_same_syntax`] runs the samples through all
//!   three and requires the same answer, which is the property a reader
//!   actually depends on;
//! * [`every_option_decides_a_sample`] requires each member of `ParseOptions`
//!   to change the answer for at least one sample, which is what makes the
//!   first test complete. Without it a member could be dropped from one copy
//!   and no sample would notice.
//!
//! This test lives in `uf_transform` because it is the crate that can see both
//! sides.

use flow_parser::ParseOptions;

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
