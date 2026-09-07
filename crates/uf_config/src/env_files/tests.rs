//! The rules in the module docs, one test each.
//!
//! Everything here is a directory of files and a map standing in for the
//! process environment: [`super::load_from`] takes the second rather than
//! reading `std::env`, so these say what they mean and can run at once.

use super::*;

/// A project directory holding `files`, as `(name, contents)` pairs.
fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    dir
}

fn root(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap()
}

/// The environment for `mode`, with nothing in the process environment.
fn load_at(dir: &tempfile::TempDir, mode: &str) -> Result<ProjectEnv, EnvFileError> {
    load_from(
        &root(dir),
        &UniflowedConfig::default(),
        mode,
        &BTreeMap::new(),
    )
}

/// One value, or a panic naming what was loaded instead.
fn value(env: &ProjectEnv, name: &str) -> String {
    env.values()
        .get(name)
        .unwrap_or_else(|| panic!("{name} was not loaded; got {:?}", env.values()))
        .clone()
}

/// The message of a failure, or a panic saying it succeeded.
fn failure(result: Result<ProjectEnv, EnvFileError>) -> String {
    match result {
        Ok(env) => panic!("expected a failure; loaded {:?}", env.values()),
        Err(error) => error.to_string(),
    }
}

#[test]
fn the_cascade_is_read_in_order_and_the_later_file_wins() {
    let dir = project(&[
        (".env", "SHARED=base\nONLY_BASE=1\n"),
        (".env.local", "SHARED=local\n"),
        (".env.production", "SHARED=production\nMODE_ONLY=yes\n"),
        (".env.production.local", "SHARED=production-local\n"),
        (".env.development", "SHARED=development\n"),
    ]);

    let production = load_at(&dir, "production").unwrap();
    assert_eq!(value(&production, "SHARED"), "production-local");
    assert_eq!(value(&production, "ONLY_BASE"), "1");
    assert_eq!(value(&production, "MODE_ONLY"), "yes");

    // The same directory, one mode over: the mode files are the only
    // difference, and `.env.development` was invisible above.
    let development = load_at(&dir, "development").unwrap();
    assert_eq!(value(&development, "SHARED"), "development");
    assert!(!development.values().contains_key("MODE_ONLY"));

    assert_eq!(
        production
            .files()
            .iter()
            .map(|file| file.file_name().unwrap().to_owned())
            .collect::<Vec<_>>(),
        vec![
            ".env",
            ".env.local",
            ".env.production",
            ".env.production.local"
        ]
    );
}

/// The rule the whole feature rests on, and the one a deployment relies on.
#[test]
fn the_process_environment_beats_every_file() {
    let dir = project(&[
        (".env", "TOKEN=from-the-file\n"),
        (".env.production", "TOKEN=from-the-mode-file\n"),
    ]);
    let process = BTreeMap::from([(String::from("TOKEN"), String::from("from-the-shell"))]);

    let env = load_from(
        &root(&dir),
        &UniflowedConfig::default(),
        "production",
        &process,
    )
    .unwrap();

    // Not "the shell's value wins" but "uf sets nothing": the value is already
    // in the environment of every process uf starts, and re-exporting it would
    // be uf deciding it agrees.
    assert!(
        !env.values().contains_key("TOKEN"),
        "uf must not set a variable the environment already carries: {:?}",
        env.values()
    );
}

/// A value uf itself injected is a file's value, and loses to a file.
#[test]
fn a_value_a_parent_uf_injected_does_not_count_as_the_environment() {
    let dir = project(&[(".env.production", "API=production\n")]);
    // What `uf run` leaves for the `uf build` inside a task: the development
    // value, and the note that uf put it there.
    let process = BTreeMap::from([
        (String::from("API"), String::from("development")),
        (String::from(INJECTED), String::from("API")),
    ]);

    let env = load_from(
        &root(&dir),
        &UniflowedConfig::default(),
        "production",
        &process,
    )
    .unwrap();

    assert_eq!(value(&env, "API"), "production");
    // And the note travels on, so the process this one starts knows as much.
    let exported: BTreeMap<String, String> = env.exported().into_iter().collect();
    assert_eq!(exported.get(INJECTED).map(String::as_str), Some("API"));
}

#[test]
fn env_files_replaces_the_cascade_when_it_is_set() {
    let dir = project(&[
        (".env", "WHO=cascade\n"),
        (".env.production", "WHO=cascade-mode\n"),
        ("config/shared.env", "WHO=named\nSHARED=1\n"),
        ("config/secret.env", "WHO=named-second\n"),
    ]);
    let mut config = UniflowedConfig::default();
    config.env.files = vec![
        compact_str::CompactString::const_new("config/shared.env"),
        compact_str::CompactString::const_new("config/secret.env"),
    ];

    let env = load_from(&root(&dir), &config, "production", &BTreeMap::new()).unwrap();

    assert_eq!(value(&env, "WHO"), "named-second");
    assert_eq!(value(&env, "SHARED"), "1");
    assert_eq!(env.files().len(), 2, "{:?}", env.files());
}

/// A file the project names and does not have is not an error: `.env.local` is
/// git-ignored by convention, so it is absent on exactly the machines that
/// matter most.
#[test]
fn a_file_that_does_not_exist_is_skipped() {
    let dir = project(&[(".env", "PRESENT=1\n")]);

    let env = load_at(&dir, "production").unwrap();

    assert_eq!(value(&env, "PRESENT"), "1");
    assert_eq!(env.files().len(), 1);
}

/// What a watcher is given: the cascade, existing or not.
///
/// [`ProjectEnv::files`] answers "which files were read" and is what goes on a
/// banner. A watcher needs the other answer: a `.env.local` created while
/// `uf dev` runs changes the environment exactly as much as an edit to one that
/// was already there, and a watcher primed with only what existed at startup
/// would never see it. See ubugeeei-prod/uf#428.
#[test]
fn the_files_a_watcher_is_given_are_the_whole_cascade() {
    let dir = project(&[(".env", "PRESENT=1\n")]);
    let root = root(&dir);

    let watched = candidate_files(&root, &UniflowedConfig::default(), "development").unwrap();

    assert_eq!(
        watched,
        vec![
            root.join(".env"),
            root.join(".env.local"),
            root.join(".env.development"),
            root.join(".env.development.local"),
        ]
    );
    // And the mode is in the answer, so a watcher started for one mode is not
    // watching another's files.
    let production = candidate_files(&root, &UniflowedConfig::default(), "production").unwrap();
    assert!(production.contains(&root.join(".env.production")));
    assert!(!production.contains(&root.join(".env.development")));
}

/// `env.files` replaces the cascade for the watcher too, and is still refused
/// when it names something outside the project.
#[test]
fn the_files_a_watcher_is_given_follow_env_files() {
    let dir = project(&[("config/shared.env", "A=1\n")]);
    let root = root(&dir);
    let mut config = UniflowedConfig::default();
    config.env.files = vec![compact_str::CompactString::const_new("config/shared.env")];

    assert_eq!(
        candidate_files(&root, &config, "development").unwrap(),
        vec![root.join("config/shared.env")]
    );

    config.env.files = vec![compact_str::CompactString::const_new("../outside.env")];
    assert!(candidate_files(&root, &config, "development").is_err());
}

#[test]
fn comments_blank_lines_and_export_are_understood() {
    let dir = project(&[(
        ".env",
        "# a comment\n\
         \n\
           # an indented comment\n\
         export EXPORTED=yes\n\
         TRAILING=value # and a comment\n\
         HASH=pass#word\n\
         EMPTY=\n\
         SPACED   =   spaced\n",
    )]);

    let env = load_at(&dir, "development").unwrap();

    assert_eq!(value(&env, "EXPORTED"), "yes");
    assert_eq!(value(&env, "TRAILING"), "value");
    // A `#` with no space in front of it is part of the value: a password is
    // allowed to have one, and uf says so rather than silently truncating.
    assert_eq!(value(&env, "HASH"), "pass#word");
    assert_eq!(value(&env, "EMPTY"), "");
    assert_eq!(value(&env, "SPACED"), "spaced");
}

#[test]
fn quotes_carry_spaces_newlines_and_escapes() {
    let dir = project(&[(
        ".env",
        "DOUBLE=\"  two words  \"\n\
         SINGLE='  as written  '\n\
         ESCAPED=\"line\\nbreak\\ttab \\\"quoted\\\" \\\\ \"\n\
         KEPT=\"C:\\\\path\\dir\"\n\
         LITERAL='no $EXPANSION here'\n\
         MULTI=\"first\nsecond\"\n\
         BLOCK='one\ntwo'\n\
         AFTER=last\n",
    )]);

    let env = load_at(&dir, "development").unwrap();

    assert_eq!(value(&env, "DOUBLE"), "  two words  ");
    assert_eq!(value(&env, "SINGLE"), "  as written  ");
    assert_eq!(value(&env, "ESCAPED"), "line\nbreak\ttab \"quoted\" \\ ");
    // A backslash before anything that is not an escape stays a backslash, so
    // a Windows path survives being written once.
    assert_eq!(value(&env, "KEPT"), "C:\\path\\dir");
    assert_eq!(value(&env, "LITERAL"), "no $EXPANSION here");
    assert_eq!(value(&env, "MULTI"), "first\nsecond");
    assert_eq!(value(&env, "BLOCK"), "one\ntwo");
    // And the line after a value that spanned lines is still a line.
    assert_eq!(value(&env, "AFTER"), "last");
}

#[test]
fn a_value_expands_a_variable_that_is_already_defined() {
    let dir = project(&[
        (
            ".env",
            "HOST=example.com\n\
             URL=https://$HOST/api\n\
             BRACED=\"https://${HOST}/v2\"\n\
             LITERAL=costs \\$5\n\
             LONE=100$\n",
        ),
        (".env.production", "FROM_EARLIER=${URL}/production\n"),
    ]);
    let process = BTreeMap::from([(String::from("SHELL_ONLY"), String::from("from-the-shell"))]);
    let dir_root = root(&dir);
    let mut config = UniflowedConfig::default();
    config.env.files = Vec::new();

    let env = load_from(&dir_root, &config, "production", &process).unwrap();

    assert_eq!(value(&env, "URL"), "https://example.com/api");
    assert_eq!(value(&env, "BRACED"), "https://example.com/v2");
    assert_eq!(value(&env, "LITERAL"), "costs $5");
    assert_eq!(value(&env, "LONE"), "100$");
    // An earlier file counts as "already defined", which is what makes the
    // cascade composable rather than four unrelated files.
    assert_eq!(
        value(&env, "FROM_EARLIER"),
        "https://example.com/api/production"
    );
}

#[test]
fn a_value_expands_something_only_the_environment_has() {
    let dir = project(&[(".env", "GREETING=hello $NAME\n")]);
    let process = BTreeMap::from([(String::from("NAME"), String::from("world"))]);

    let env = load_from(
        &root(&dir),
        &UniflowedConfig::default(),
        "development",
        &process,
    )
    .unwrap();

    assert_eq!(value(&env, "GREETING"), "hello world");
}

/// The environment wins here too, so an expansion reads what the value would
/// have been rather than what the file happens to say two lines up.
#[test]
fn an_expansion_reads_the_environments_value_and_not_the_files() {
    let dir = project(&[(".env", "HOST=from-the-file\nURL=https://$HOST\n")]);
    let process = BTreeMap::from([(String::from("HOST"), String::from("from-the-shell"))]);

    let env = load_from(
        &root(&dir),
        &UniflowedConfig::default(),
        "development",
        &process,
    )
    .unwrap();

    assert_eq!(value(&env, "URL"), "https://from-the-shell");
}

#[test]
fn an_undefined_expansion_is_refused_by_name() {
    let dir = project(&[(".env", "GOOD=1\nURL=https://$TYPOED_HOST/api\n")]);

    let message = failure(load_at(&dir, "development"));

    assert!(message.contains(".env:2"), "{message}");
    assert!(message.contains("$TYPOED_HOST"), "{message}");
    assert!(message.contains("is not defined"), "{message}");
}

#[test]
fn a_line_without_an_equals_sign_is_refused() {
    let dir = project(&[(".env", "FINE=1\nJUST_A_NAME\n")]);

    let message = failure(load_at(&dir, "development"));

    assert!(message.contains(".env:2"), "{message}");
    assert!(message.contains("`JUST_A_NAME` has no `=`"), "{message}");
}

#[test]
fn a_name_that_is_not_a_name_is_refused() {
    let dir = project(&[(".env", "1TOKEN=nope\n")]);

    let message = failure(load_at(&dir, "development"));

    assert!(message.contains(".env:1"), "{message}");
    assert!(message.contains("1TOKEN"), "{message}");
    assert!(message.contains("is not a variable name"), "{message}");
}

/// A message about a line names what is wrong with it and never its value.
///
/// Half of what is in a `.env` file is a credential, and an error message is a
/// thing people paste into an issue. See `docs/security.md`.
#[test]
fn a_message_about_a_line_never_carries_its_value() {
    let dir = project(&[(".env", "1TOKEN=hunter2\n")]);

    let message = failure(load_at(&dir, "development"));

    assert!(message.contains("1TOKEN"), "{message}");
    assert!(
        !message.contains("hunter2"),
        "an error message must not print a value: {message}"
    );
}

#[test]
fn a_quote_that_is_never_closed_is_refused_at_the_line_it_opened() {
    let dir = project(&[(".env", "A=1\nBROKEN=\"never closed\nC=3\n")]);

    let message = failure(load_at(&dir, "development"));

    // The line it opened on, not the end of the file: that is the line to fix.
    assert!(message.contains(".env:2"), "{message}");
    assert!(message.contains("never closed"), "{message}");
}

#[test]
fn text_after_a_quoted_value_is_refused() {
    let dir = project(&[(".env", "A=\"one\" two\n")]);

    let message = failure(load_at(&dir, "development"));

    assert!(message.contains(".env:1"), "{message}");
    assert!(message.contains("text after the quoted value"), "{message}");
}

#[test]
fn an_unclosed_brace_is_refused() {
    let dir = project(&[(".env", "HOST=h\nURL=${HOST/api\n")]);

    let message = failure(load_at(&dir, "development"));

    assert!(message.contains(".env:2"), "{message}");
    assert!(message.contains("never closed"), "{message}");
}

#[test]
fn a_file_that_is_too_large_is_refused_rather_than_read() {
    let dir = tempfile::tempdir().unwrap();
    let big = "X".repeat(usize::try_from(MAX_FILE_BYTES).unwrap() + 1);
    std::fs::write(dir.path().join(".env"), big).unwrap();

    let message = failure(load_at(&dir, "development"));

    assert!(message.contains("is larger than"), "{message}");
}

#[test]
fn the_client_prefix_is_vites_and_a_project_may_name_another() {
    let dir = project(&[(".env", "VITE_PUBLIC=1\nSECRET=2\n")]);

    let default = load_at(&dir, "development").unwrap();
    assert!(default.is_client_visible("VITE_PUBLIC"));
    assert!(!default.is_client_visible("SECRET"));
    assert_eq!(default.client_prefixes(), [String::from("VITE_")]);

    let config = UniflowedConfig {
        vite: Some(serde_json::json!({ "envPrefix": ["PUBLIC_", "VITE_"] })),
        ..UniflowedConfig::default()
    };
    let named = load_from(&root(&dir), &config, "development", &BTreeMap::new()).unwrap();
    assert!(named.is_client_visible("PUBLIC_THING"));
    assert!(named.is_client_visible("VITE_PUBLIC"));
    assert!(!named.is_client_visible("SECRET"));

    // An empty prefix would put every variable in the bundle. Vite refuses it
    // and so does this, by falling back to the prefix that means something.
    let emptied = UniflowedConfig {
        vite: Some(serde_json::json!({ "envPrefix": "" })),
        ..UniflowedConfig::default()
    };
    let fallback = load_from(&root(&dir), &emptied, "development", &BTreeMap::new()).unwrap();
    assert_eq!(fallback.client_prefixes(), [String::from("VITE_")]);
    assert!(!fallback.is_client_visible("SECRET"));
}

#[test]
fn the_mode_comes_from_the_flag_then_the_profile_then_the_config() {
    let dir = project(&[]);
    let path = root(&dir);
    let mut config = UniflowedConfig::default();

    // Nothing set anywhere: the command's own default.
    assert_eq!(
        resolve_mode(&path, &config, None, "production").unwrap(),
        "production"
    );

    config.env.active = compact_str::CompactString::const_new("staging");
    assert_eq!(
        resolve_mode(&path, &config, None, "production").unwrap(),
        "staging"
    );

    std::fs::create_dir_all(path.join(".uf")).unwrap();
    std::fs::write(path.join(PROFILE_FILE), "review\n").unwrap();
    assert_eq!(
        resolve_mode(&path, &config, None, "production").unwrap(),
        "review"
    );

    // And what was typed beats all of it.
    assert_eq!(
        resolve_mode(&path, &config, Some("production"), "production").unwrap(),
        "production"
    );
}

#[test]
fn a_mode_that_could_not_be_a_file_name_is_refused() {
    for name in ["", "local", "../etc", "pro duction", ".hidden", "a/b"] {
        let error = check_mode(name).expect_err("{name} must not be a mode");
        assert!(
            error.to_string().contains("is not a mode"),
            "{name}: {error}"
        );
    }
    for name in ["development", "production", "test", "staging-2", "e2e.ci"] {
        check_mode(name).unwrap_or_else(|error| panic!("{name} must be a mode: {error}"));
    }
}

#[test]
fn what_is_exported_is_the_values_and_the_note_that_uf_set_them() {
    let dir = project(&[(".env", "ONE=1\nTWO=2\n")]);

    let env = load_at(&dir, "development").unwrap();
    let exported: BTreeMap<String, String> = env.exported().into_iter().collect();

    assert_eq!(exported.get("ONE").map(String::as_str), Some("1"));
    assert_eq!(exported.get("TWO").map(String::as_str), Some("2"));
    assert_eq!(exported.get(INJECTED).map(String::as_str), Some("ONE,TWO"));
}

/// A file uf reads is a file inside the project.
///
/// `uf.config.js` in a repository somebody just cloned is untrusted input, and
/// `~/.aws/credentials` parses as `NAME=value` like anything else: reading one
/// would put somebody's keys into the environment of every process uf starts,
/// and any name in it behind the client prefix into the bundle.
#[test]
fn env_files_cannot_name_a_path_outside_the_project() {
    let dir = project(&[(".env", "INSIDE=1\n")]);
    for entry in [
        "../outside.env",
        "/etc/passwd",
        "~/.aws/credentials",
        "config\\windows.env",
        "C:/secrets.env",
        "",
    ] {
        let mut config = UniflowedConfig::default();
        config.env.files = vec![compact_str::CompactString::new(entry)];
        let message = failure(load_from(
            &root(&dir),
            &config,
            "development",
            &BTreeMap::new(),
        ));
        assert!(
            message.contains("is not an environment file this project may read"),
            "{entry}: {message}"
        );
    }
}

/// And a symlink out of the project is not a file inside it.
///
/// The lexical check above cannot see this one: `.env` is a name in the project
/// root, and what it points at is somewhere else entirely.
#[cfg(unix)]
#[test]
fn a_file_that_points_out_of_the_project_is_refused() {
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("credentials");
    std::fs::write(&secret, "AWS_SECRET_ACCESS_KEY=not-yours\n").unwrap();

    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&secret, dir.path().join(".env")).unwrap();

    let message = failure(load_at(&dir, "development"));

    assert!(
        message.contains("is not an environment file this project may read"),
        "{message}"
    );
}

/// uf's own marker is not something a file may set.
///
/// A file that could write it could tell the next uf command that a value the
/// shell really set — a CI secret — came from a file and may be overridden.
#[test]
fn a_file_cannot_write_the_marker_uf_uses_for_its_own_children() {
    let dir = project(&[(".env", "UF_ENV_INJECTED=DATABASE_URL\n")]);

    let message = failure(load_at(&dir, "development"));

    assert!(message.contains(INJECTED), "{message}");
    assert!(message.contains("is uf's own"), "{message}");
}

/// A project with no files at all sets nothing, including the note.
#[test]
fn a_project_with_no_env_files_exports_nothing() {
    let dir = project(&[]);

    let env = load_at(&dir, "development").unwrap();

    assert!(env.values().is_empty());
    assert!(env.exported().is_empty());
    assert!(env.files().is_empty());
}

/// A name the caller is about to set itself is not one uf read from a file.
///
/// [`ProjectEnv::apply_over`] is what `uf run` uses for a task's `env` block,
/// and the half that is easy to lose is the marker rather than the value: a
/// child that holds the caller's value under uf's own label hands it to the
/// *next* uf as a file value, and that one lets its own files overrule it. So
/// an overridden name leaves both the values and [`INJECTED`], and a name that
/// really did come from a file stays in both.
#[test]
fn a_name_the_caller_overrides_leaves_the_marker() {
    let dir = project(&[(".env", "ONE=1\nTWO=2\n")]);

    let env = load_at(&dir, "development").unwrap();
    let overridden: BTreeSet<&str> = ["ONE"].into_iter().collect();
    let exported: BTreeMap<String, String> =
        env.exported_beneath(&overridden).into_iter().collect();

    assert_eq!(exported.get("ONE"), None);
    assert_eq!(exported.get("TWO").map(String::as_str), Some("2"));
    assert_eq!(exported.get(INJECTED).map(String::as_str), Some("TWO"));
}

/// And a name a *parent* uf injected drops out of the marker just the same.
///
/// The marker carries what this process was told about as well as what it
/// read, so an override has to be taken off both lists or the grandchild is
/// told the same wrong thing one step later.
#[test]
fn overriding_a_name_a_parent_injected_also_leaves_the_marker() {
    let dir = project(&[]);
    let process: BTreeMap<String, String> = [
        (String::from(INJECTED), String::from("API,DATABASE_URL")),
        (String::from("API"), String::from("from the parent")),
        (String::from("DATABASE_URL"), String::from("postgres:///x")),
    ]
    .into_iter()
    .collect();

    let env = load_from(
        &root(&dir),
        &UniflowedConfig::default(),
        "development",
        &process,
    )
    .unwrap();
    let overridden: BTreeSet<&str> = ["API"].into_iter().collect();
    let exported: BTreeMap<String, String> =
        env.exported_beneath(&overridden).into_iter().collect();

    assert_eq!(
        exported.get(INJECTED).map(String::as_str),
        Some("DATABASE_URL")
    );
}

/// Overriding everything leaves no marker at all, rather than an empty one.
#[test]
fn overriding_every_name_writes_no_marker() {
    let dir = project(&[(".env", "ONE=1\n")]);

    let env = load_at(&dir, "development").unwrap();
    let overridden: BTreeSet<&str> = ["ONE"].into_iter().collect();

    assert!(env.exported_beneath(&overridden).is_empty());
}

/// And the marker a *parent* set is taken off the command, not merely not
/// written.
///
/// A child inherits this process's environment and `Command::env` only adds to
/// it, so "write no marker" and "hand on no marker" are different sentences.
/// When every name has been overridden there is nothing left to write, and
/// without the removal the child would inherit a marker naming the one variable
/// the caller had just taken ownership of — which is the whole defect
/// [`ProjectEnv::apply_over`] exists to prevent, one process further along.
#[test]
fn apply_over_takes_an_inherited_marker_off_the_command() {
    let dir = project(&[]);
    let process: BTreeMap<String, String> = [
        (String::from(INJECTED), String::from("API")),
        (String::from("API"), String::from("from the parent")),
    ]
    .into_iter()
    .collect();

    let env = load_from(
        &root(&dir),
        &UniflowedConfig::default(),
        "development",
        &process,
    )
    .unwrap();
    let mut command = std::process::Command::new("true");
    env.apply_over(&mut command, &[("API", "from the task")]);

    let written: BTreeMap<String, Option<String>> = command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect();

    // `None` is a removal, which is the assertion: absent from this map would
    // mean the child keeps whatever it inherited.
    assert_eq!(written.get(INJECTED), Some(&None));
    assert_eq!(
        written.get("API"),
        Some(&Some(String::from("from the task")))
    );
}

/// A profile an older uf wrote is still read.
///
/// `uf env use` wrote `.uniflowed/profile` and writes `.uf/profile` now. A
/// project that has not run `uf env install` since has only the old one, and
/// falling back to the default there would put it on a different set of
/// `.env` files without saying so.
#[test]
fn a_profile_at_the_old_path_is_read_until_the_new_one_exists() {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8Path::from_path(dir.path()).unwrap();

    std::fs::create_dir_all(path.join(".uniflowed")).unwrap();
    std::fs::write(path.join(LEGACY_PROFILE_FILE), "review\n").unwrap();
    assert_eq!(active_profile(path).unwrap().as_deref(), Some("review"));

    // The new one wins once it is there, so a `uf env use` after the move is
    // not overruled by what it replaced.
    std::fs::create_dir_all(path.join(".uf")).unwrap();
    std::fs::write(path.join(PROFILE_FILE), "staging\n").unwrap();
    assert_eq!(active_profile(path).unwrap().as_deref(), Some("staging"));
}
