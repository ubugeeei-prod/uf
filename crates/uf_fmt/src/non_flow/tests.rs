use super::*;

fn config() -> FmtConfig {
    FmtConfig::default()
}

#[test]
fn none_runs_nothing() {
    assert_eq!(invocation(NonFlowFormatter::None, false, &config()), None);
    assert_eq!(invocation(NonFlowFormatter::None, true, &config()), None);
}

#[test]
fn biome_writes_when_asked_to_format_and_only_reports_under_check() {
    let write = invocation(NonFlowFormatter::Biome, false, &config()).expect("biome runs");
    let check = invocation(NonFlowFormatter::Biome, true, &config()).expect("biome runs");

    assert_eq!(write.program, "biome");
    assert!(write.arguments.iter().any(|argument| argument == "--write"));
    assert!(
        !check.arguments.iter().any(|argument| argument == "--write"),
        "a check must not rewrite the files it is checking"
    );
}

#[test]
fn prettier_is_a_real_second_provider() {
    // Red line 3's exit criterion: a second variant a project can actually
    // select, not a shape.
    let write = invocation(NonFlowFormatter::Prettier, false, &config()).expect("prettier runs");
    let check = invocation(NonFlowFormatter::Prettier, true, &config()).expect("prettier runs");

    assert_eq!(write.program, "prettier");
    assert!(write.arguments.iter().any(|argument| argument == "--write"));
    assert!(check.arguments.iter().any(|argument| argument == "--check"));
}

#[test]
fn every_provider_has_a_name_to_print() {
    assert_eq!(NonFlowFormatter::Biome.as_str(), "biome");
    assert_eq!(NonFlowFormatter::Prettier.as_str(), "prettier");
    assert_eq!(NonFlowFormatter::None.as_str(), "none");
}

/// A project with no JSON must not need a formatter installed at all.
#[test]
fn formatting_nothing_runs_nothing() {
    let root = Utf8Path::new(".");

    assert_eq!(
        run(NonFlowFormatter::Biome, root, &[], false, &config()),
        Ok(true)
    );
    assert_eq!(
        run(NonFlowFormatter::Prettier, root, &[], true, &config()),
        Ok(true)
    );
}

#[test]
fn none_is_success_even_with_files_to_format() {
    let files = vec!["a.json".to_string()];

    assert_eq!(
        run(
            NonFlowFormatter::None,
            Utf8Path::new("."),
            &files,
            false,
            &config()
        ),
        Ok(true)
    );
}

/// "No such file or directory" sends a reader looking for the source file
/// rather than the formatter, so the error says which binary is missing and
/// what to do instead.
#[test]
fn a_missing_formatter_says_which_one_and_how_to_proceed() {
    let error = NonFlowError::NotInstalled {
        formatter: "biome".into(),
        count: 12,
    };
    let rendered = error.to_string();

    assert!(rendered.contains("biome"), "{rendered}");
    assert!(rendered.contains("12"), "{rendered}");
    assert!(rendered.contains("fmt.nonFlow.formatter"), "{rendered}");
}

#[test]
fn a_failure_repeats_what_the_formatter_said() {
    let error = NonFlowError::Failed {
        formatter: "prettier".into(),
        detail: "SyntaxError: Unexpected token".into(),
    };

    assert!(error.to_string().contains("SyntaxError"));
}

#[test]
fn stderr_is_preferred_over_stdout_and_falls_back_to_it() {
    assert_eq!(detail_of(b"  the error  ", b"noise"), "the error");
    assert_eq!(
        detail_of(b"   \n ", b"what stdout said"),
        "what stdout said"
    );
}

#[test]
fn a_very_long_complaint_is_trimmed_rather_than_printed_whole() {
    let long = "x".repeat(MAX_DETAIL_BYTES * 3);

    let detail = detail_of(long.as_bytes(), b"");

    assert!(detail.len() <= MAX_DETAIL_BYTES + 4, "{}", detail.len());
    assert!(detail.ends_with('…'));
}

#[test]
fn trimming_never_splits_a_character() {
    // A multi-byte character straddling the cut would panic on a byte slice.
    let long = "日".repeat(MAX_DETAIL_BYTES);

    let detail = detail_of(long.as_bytes(), b"");

    assert!(detail.ends_with('…'));
}

/// A formatter is almost always a dependency rather than a global install.
#[test]
fn a_local_formatter_is_preferred_over_one_on_the_path() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    let bin = root.join("node_modules/.bin");
    std::fs::create_dir_all(&bin).expect("create node_modules/.bin");
    std::fs::write(bin.join("biome"), "#!/bin/sh\n").expect("write the binary");

    assert_eq!(program_path(&root, "biome"), bin.join("biome").as_str());
}

/// Falls back to the bare name, so the "not installed" error comes from the
/// spawn rather than from a path check that cannot know about `PATH`.
#[test]
fn a_formatter_with_no_local_copy_falls_back_to_the_path() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");

    assert_eq!(program_path(&root, "prettier"), "prettier");
}

/// The seam's real job: uf's settings, in each provider's vocabulary.
///
/// Biome indents with tabs by default and uf does not, so a provider that was
/// merely invoked would reformat every JSON file against the rule uf's own
/// printer follows — which is what happened the first time this ran over uf's
/// own repository.
#[test]
fn uf_settings_are_translated_into_the_providers_own_flags() {
    let mut config = FmtConfig::default();
    config.indent_width = 4;
    config.line_width = 80;

    let biome = invocation(NonFlowFormatter::Biome, false, &config).expect("biome runs");
    assert!(biome.arguments.iter().any(|a| a == "--indent-style=space"));
    assert!(biome.arguments.iter().any(|a| a == "--indent-width=4"));
    assert!(biome.arguments.iter().any(|a| a == "--line-width=80"));

    let prettier = invocation(NonFlowFormatter::Prettier, false, &config).expect("prettier runs");
    assert!(prettier.arguments.iter().any(|a| a == "--tab-width=4"));
    assert!(prettier.arguments.iter().any(|a| a == "--print-width=80"));
}

#[test]
fn the_quote_style_reaches_the_provider_that_has_one() {
    let mut single = FmtConfig::default();
    single.quotes = QuoteStyle::Single;
    let double = FmtConfig::default();

    let with = invocation(NonFlowFormatter::Prettier, false, &single).expect("prettier runs");
    let without = invocation(NonFlowFormatter::Prettier, false, &double).expect("prettier runs");

    assert!(with.arguments.iter().any(|a| a == "--single-quote"));
    assert!(!without.arguments.iter().any(|a| a == "--single-quote"));
}
