use super::*;

fn config() -> FmtConfig {
    FmtConfig::default()
}

/// A `FmtConfig` that names `formatter` the way a project's `uf.config.js`
/// does — which is the difference between a requirement and uf's suggestion.
fn asked_for(formatter: NonFlowFormatter) -> FmtConfig {
    let mut config = FmtConfig::default();
    config.non_flow.formatter = formatter;
    config.non_flow.chosen_by_project = true;
    config
}

/// A `FmtConfig` uf filled in, because the project said nothing.
fn defaulted_to(formatter: NonFlowFormatter) -> FmtConfig {
    let mut config = FmtConfig::default();
    config.non_flow.formatter = formatter;
    config
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
        run(root, &[], false, &asked_for(NonFlowFormatter::Biome)),
        Ok(nothing_was_rewritten())
    );
    assert_eq!(
        run(root, &[], true, &asked_for(NonFlowFormatter::Prettier)),
        Ok(nothing_was_rewritten())
    );
}

#[test]
fn none_is_success_even_with_files_to_format() {
    let files = vec!["a.json".to_string()];

    assert_eq!(
        run(
            Utf8Path::new("."),
            &files,
            false,
            &asked_for(NonFlowFormatter::None)
        ),
        Ok(nothing_was_rewritten())
    );
}

/// A skip is a third answer, not a quiet "formatted".
///
/// Whether the spawn fails cannot be arranged from a unit test without
/// rewriting the process's own `PATH`, which every other test in this binary
/// would then be running under; the missing binary is driven end to end in
/// `crates/uf_cli/tests/cli.rs`. What is pinned here is the shape of the
/// decision: the two configurations that reach `run` differ in exactly the one
/// bit that decides it, and files nobody looked at are not files that are
/// formatted.
#[test]
fn a_skip_is_neither_formatted_nor_unformatted() {
    let skipped = NonFlowOutcome::Skipped {
        formatter: "biome".into(),
        paths: vec!["data.json".to_string()],
    };

    assert!(!skipped.is_formatted());
    assert!(nothing_was_rewritten().is_formatted());
    assert!(!NonFlowOutcome::Unformatted.is_formatted());
    let uf_chose = defaulted_to(NonFlowFormatter::Biome);
    let project_chose = asked_for(NonFlowFormatter::Biome);
    assert_eq!(
        uf_chose.non_flow.formatter,
        project_chose.non_flow.formatter
    );
    assert!(!uf_chose.non_flow.chosen_by_project);
    assert!(project_chose.non_flow.chosen_by_project);
}

/// "1 non-Flow files were left alone" was the first sentence a new project
/// ever saw from uf. See ubugeeei-prod/uf#441.
#[test]
fn the_count_and_its_verb_agree() {
    assert!(
        skipped_message("biome", 1).contains("1 non-Flow file was skipped"),
        "{}",
        skipped_message("biome", 1)
    );
    assert!(
        skipped_message("biome", 4).contains("4 non-Flow files were skipped"),
        "{}",
        skipped_message("biome", 4)
    );
    let one = NonFlowError::NotInstalled {
        formatter: "prettier".into(),
        count: 1,
    };
    assert!(one.to_string().contains("1 non-Flow file was"), "{one}");
}

/// Both ways out, in the sentence that says something was skipped: install it,
/// or say this project does not want one.
#[test]
fn the_skip_names_the_formatter_and_both_fixes() {
    let message = skipped_message("biome", 2);

    assert!(message.contains("biome"), "{message}");
    assert!(message.contains("fmt.nonFlow.formatter"), "{message}");
    assert!(message.contains("\"none\""), "{message}");
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

/// A formatter that rewrote a file is not the same answer as one that did not.
///
/// The exit status cannot tell them apart — a formatter in write mode exits 0
/// either way — and `uf prepare --fix` has to know, because a file rewritten
/// during a commit hook is in the working tree and not in the index.
///
/// Driven against a stub rather than Biome, for two reasons. It is uf's own
/// bookkeeping that is under test, and that must not depend on which formatter
/// a machine happens to have installed or on what that formatter thinks of a
/// given file. And a stub can be given exactly one opinion, which is what makes
/// "it changed this one and not that one" a thing a test can state.
#[cfg(unix)]
#[test]
fn a_write_run_reports_the_files_the_formatter_changed() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    stub_formatter(&root);
    std::fs::write(root.join("messy.json"), UNFORMATTED).expect("an unformatted file");
    std::fs::write(root.join("tidy.json"), FORMATTED).expect("a formatted file");
    let paths = vec!["messy.json".to_string(), "tidy.json".to_string()];

    let outcome = run(&root, &paths, false, &asked_for(NonFlowFormatter::Biome));

    assert_eq!(
        outcome,
        Ok(NonFlowOutcome::Formatted {
            rewritten: vec!["messy.json".to_string()],
        }),
        "a file the formatter left alone must not be reported as rewritten"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("messy.json")).expect("the file is there"),
        FORMATTED
    );
}

/// A second run over what the first one wrote reports nothing.
///
/// Which is what makes the failure `uf prepare --fix` raises actionable: stage
/// the diff, run it again, and the same command passes.
#[cfg(unix)]
#[test]
fn a_write_run_over_already_formatted_files_rewrites_nothing() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    stub_formatter(&root);
    std::fs::write(root.join("tidy.json"), FORMATTED).expect("a formatted file");
    let paths = vec!["tidy.json".to_string()];

    assert_eq!(
        run(&root, &paths, false, &asked_for(NonFlowFormatter::Biome)),
        Ok(nothing_was_rewritten())
    );
}

/// A check does not write, so it has nothing to report having written.
#[cfg(unix)]
#[test]
fn a_check_run_reports_a_verdict_rather_than_a_rewrite() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    stub_formatter(&root);
    std::fs::write(root.join("messy.json"), UNFORMATTED).expect("an unformatted file");
    std::fs::write(root.join("tidy.json"), FORMATTED).expect("a formatted file");

    assert_eq!(
        run(
            &root,
            &["tidy.json".to_string()],
            true,
            &asked_for(NonFlowFormatter::Biome)
        ),
        Ok(nothing_was_rewritten())
    );
    assert_eq!(
        run(
            &root,
            &["messy.json".to_string()],
            true,
            &asked_for(NonFlowFormatter::Biome)
        ),
        Ok(NonFlowOutcome::Unformatted)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("messy.json")).expect("the file is there"),
        UNFORMATTED,
        "a check rewrote the file it was checking"
    );
}

/// The one file the stub below is content with.
#[cfg(unix)]
const FORMATTED: &str = "{\n  \"a\": 1\n}\n";

/// And a file it is not.
#[cfg(unix)]
const UNFORMATTED: &str = "{\"a\":1}\n";

/// Install a stand-in for the project's formatter at `root`.
///
/// `node_modules/.bin` is where [`program_path`] looks first, so this needs
/// nothing of the machine's own `PATH`. It has a real formatter's two modes:
/// it rewrites what it dislikes when it is given `--write`, which is how uf
/// spells "format" to Biome, and reports it by exiting 1 when it is not.
#[cfg(unix)]
fn stub_formatter(root: &Utf8Path) {
    use std::os::unix::fs::PermissionsExt;

    let bin = root.join("node_modules/.bin");
    std::fs::create_dir_all(&bin).expect("a place for the stub");
    let stub = bin.join("biome");
    std::fs::write(
        &stub,
        format!(
            "#!/usr/bin/env sh\n\
             set -eu\n\
             write=no\n\
             case \" $* \" in *' --write '*) write=yes ;; esac\n\
             status=0\n\
             formatted={FORMATTED:?}\n\
             for arg in \"$@\"; do\n\
             \x20 [ -f \"$arg\" ] || continue\n\
             \x20 [ \"$(cat \"$arg\")\" != \"$(printf '%b' \"$formatted\")\" ] || continue\n\
             \x20 if [ \"$write\" = yes ]; then printf '%b' \"$formatted\" > \"$arg\"; else status=1; fi\n\
             done\n\
             exit \"$status\"\n"
        ),
    )
    .expect("the stub is written");
    let mut permissions = std::fs::metadata(&stub)
        .expect("the stub is there")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&stub, permissions).expect("the stub is executable");
}
