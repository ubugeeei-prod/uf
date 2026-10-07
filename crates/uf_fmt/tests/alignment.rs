#![allow(clippy::disallowed_macros)]

use uf_config::FmtConfig;
use uf_fmt::format_source;

fn formatted(source: &str, align: bool) -> String {
    formatted_with_width(source, align, 100)
}

fn formatted_with_width(source: &str, align: bool, line_width: u16) -> String {
    let mut config = FmtConfig::default();
    config.align = align;
    config.line_width = line_width;
    let once = format_source(source, &config).expect("valid Flow").output;
    let twice = format_source(&once, &config)
        .expect("valid formatted Flow")
        .output;
    assert_eq!(once, twice, "alignment must be idempotent");
    once
}

#[test]
fn aligns_match_arrows_within_one_match() {
    let source = r#"function modeLabel(mode: PaletteMode): string {
  return match (mode) {
    "files" => "Files",
    "commands" => "Commands",
    "symbols" => "Symbols",
    "projectSymbols" => "Project",
    "line" => "Line",
  };
}
"#;
    let aligned = formatted(source, true);
    assert!(
        aligned.contains("\"files\"          => \"Files\""),
        "{aligned}"
    );
    assert!(
        aligned.contains("\"projectSymbols\" => \"Project\""),
        "{aligned}"
    );
    let plain = formatted(source, false);
    assert!(plain.contains("\"files\" => \"Files\""), "{plain}");
}

#[test]
fn aligns_component_hook_bindings_and_assignments() {
    let source = r#"component Search() {
  const [showSearch, setShowSearch] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);
  return null;
}
"#;
    let aligned = formatted(source, true);
    let hooks: Vec<_> = aligned
        .lines()
        .filter(|line| line.contains(" = use"))
        .collect();
    assert_eq!(hooks.len(), 3, "{aligned}");
    let equals: Vec<_> = hooks.iter().map(|line| line.find('=').unwrap()).collect();
    assert!(
        equals.iter().all(|column| *column == equals[0]),
        "{aligned}"
    );
    let setters: Vec<_> = hooks[..2]
        .iter()
        .map(|line| {
            line.find("setShowSearch")
                .or_else(|| line.find("setSearchQuery"))
                .unwrap()
        })
        .collect();
    assert_eq!(setters[0], setters[1], "{aligned}");
    let plain = formatted(source, false);
    assert!(plain.contains("const searchInputRef = useRef"), "{plain}");
}

#[test]
fn aligns_expanded_object_values_and_can_opt_out() {
    let source = "const palette = {\n  file: 'Files',\n  projectSymbols: 'Project',\n};\n";
    let aligned = formatted(source, true);
    assert!(aligned.contains("file          : \"Files\""), "{aligned}");
    assert!(aligned.contains("projectSymbols: \"Project\""), "{aligned}");
    let plain = formatted(source, false);
    assert!(plain.contains("file: \"Files\""), "{plain}");
}

#[test]
fn comments_and_multiline_entries_end_an_alignment_group() {
    let source = r#"const palette = {
  file: "Files",
  // This entry starts a new group.
  projectSymbols: "Project",
  line: "Line",
};
const inline = { file: 1, projectSymbols: 2 };
const complex = {
  file: "Files",
  projectSymbols: {
    label: "Project",
  },
  line: "Line",
};
"#;
    let output = formatted(source, true);
    assert!(output.contains("  file: \"Files\","), "{output}");
    assert!(output.contains("  line          : \"Line\","), "{output}");
    let inline = output
        .lines()
        .find(|line| line.contains("const inline"))
        .expect("inline object");
    assert!(inline.contains("file: 1"), "{inline}");
    assert!(
        output.contains("  file: \"Files\",\n  projectSymbols: {"),
        "{output}"
    );
    assert!(output.contains("  line: \"Line\",\n};"), "{output}");
}

#[test]
fn aligns_multiline_function_parameters() {
    let source = r#"function load(
  id: number,
  selectedProjectSymbol: string,
  options?: Readonly<{ includeHidden: boolean }>,
): void {}
"#;
    let aligned = formatted(source, true);
    let params: Vec<_> = aligned
        .lines()
        .filter(|line| {
            line.contains(": number") || line.contains(": string") || line.contains(": Readonly")
        })
        .collect();
    assert_eq!(params.len(), 3, "{aligned}");
    let columns: Vec<_> = params.iter().map(|line| line.find(':').unwrap()).collect();
    assert!(
        columns.iter().all(|column| *column == columns[0]),
        "{aligned}"
    );
    let plain = formatted(source, false);
    assert!(plain.contains("  id: number,"), "{plain}");
}

#[test]
fn aligns_type_aliases_and_object_type_fields() {
    let source = r#"type Short = string;
export type LongerName = number;
type Props = {
  id: number,
  selectedProjectSymbol: string,
};
"#;
    let aligned = formatted(source, true);
    let aliases: Vec<_> = aligned
        .lines()
        .filter(|line| line.starts_with("type Short") || line.starts_with("export type LongerName"))
        .collect();
    assert_eq!(aliases.len(), 2, "{aligned}");
    assert_eq!(aliases[0].find('='), aliases[1].find('='), "{aligned}");
    let fields: Vec<_> = aligned
        .lines()
        .filter(|line| {
            line.trim_start().starts_with("id ")
                || line.trim_start().starts_with("selectedProjectSymbol:")
        })
        .collect();
    assert_eq!(fields.len(), 2, "{aligned}");
    assert_eq!(fields[0].find(':'), fields[1].find(':'), "{aligned}");
    let plain = formatted(source, false);
    assert!(plain.contains("type Short = string;"), "{plain}");
    assert!(plain.contains("  id: number,"), "{plain}");
}

#[test]
fn aligns_component_and_function_type_parameters() {
    let source = r#"component Search(
  query: string,
  selectedProjectSymbol: number,
  includeHiddenResults: boolean,
) {
  return null;
}
type Resolver = (
  id: number,
  selectedProjectSymbol: string,
  includeHiddenResults: boolean,
) => Promise<void>;
"#;
    let aligned = formatted_with_width(source, true, 60);
    for prefix in ["query", "id"] {
        let lines: Vec<_> = aligned
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                line.starts_with(prefix)
                    || line.starts_with("selectedProjectSymbol")
                    || line.starts_with("includeHiddenResults")
            })
            .collect();
        let lines = if prefix == "query" {
            &lines[..3]
        } else {
            &lines[lines.len() - 3..]
        };
        let columns: Vec<_> = lines.iter().map(|line| line.find(':').unwrap()).collect();
        assert!(
            columns.iter().all(|column| *column == columns[0]),
            "{aligned}"
        );
    }
}

#[test]
fn aligns_single_line_default_parameters() {
    let source = r#"function connect(
  id: number = 0,
  selectedProjectSymbol: string = "",
  enabled: boolean = true,
): void {}
component Search(
  query: string = "",
  selectedProjectSymbol: number = 0,
) {
  return null;
}
"#;
    let aligned = formatted_with_width(source, true, 60);
    let function_params: Vec<_> = aligned
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("id")
                || line.starts_with("selectedProjectSymbol")
                || line.starts_with("enabled")
        })
        .take(3)
        .collect();
    assert_eq!(function_params.len(), 3, "{aligned}");
    assert!(
        function_params
            .iter()
            .all(|line| line.find(':') == function_params[0].find(':')),
        "{aligned}"
    );
    let component_params: Vec<_> = aligned
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("query") || line.starts_with("selectedProjectSymbol")
        })
        .rev()
        .take(2)
        .collect();
    assert_eq!(component_params.len(), 2, "{aligned}");
    assert_eq!(
        component_params[0].find(':'),
        component_params[1].find(':'),
        "{aligned}"
    );
}

#[test]
fn aligns_rest_parameter_colons() {
    for source in [
        r#"component Search(
  query: string,
  selectedProjectSymbol: number,
  ...rest: Rest
) {
  return null;
}
"#,
        r#"component Search(
  query?: string,
  selectedProjectSymbol: number,
  ...rest: Rest
) {
  return null;
}
"#,
        r#"function load(
  id: number,
  selectedProjectSymbol: string,
  ...rest: Array<string>
): void {}
"#,
        r#"type Resolver = (
  id: number,
  selectedProjectSymbol: string,
  ...rest: Array<string>
) => Promise<void>;
"#,
    ] {
        let aligned = formatted_with_width(source, true, 60);
        let lines: Vec<_> = aligned
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                line.starts_with("query")
                    || line.starts_with("id")
                    || line.starts_with("selectedProjectSymbol")
                    || line.starts_with("...")
            })
            .collect();
        assert_eq!(lines.len(), 3, "{aligned}");
        let columns: Vec<_> = lines.iter().map(|line| line.find(':').unwrap()).collect();
        assert!(
            columns.iter().all(|column| *column == columns[0]),
            "{columns:?}\n{aligned}"
        );
        let plain = formatted_with_width(source, false, 60);
        assert!(
            plain.contains("...rest: Rest") || plain.contains("...rest: Array<string>"),
            "{plain}"
        );
        assert!(!plain.contains("...rest "), "{plain}");
    }
}

#[test]
fn leaves_a_lone_or_unannotated_rest_tight() {
    for source in [
        r#"function load(
  ...rest: Array<string>
): void {}
"#,
        r#"function load(
  id: number,
  selectedProjectSymbol: string,
  ...rest
): void {}
"#,
        r#"component Search(
  query: string,
  ...items
) {
  return null;
}
"#,
    ] {
        let aligned = formatted_with_width(source, true, 60);
        assert!(
            !aligned.contains("...rest ") && !aligned.contains("...items "),
            "{aligned}"
        );
    }
}

fn arrow_columns(source: &str, markers: &[&str]) -> Vec<usize> {
    let columns: Vec<_> = source
        .lines()
        .filter(|line| markers.iter().any(|marker| line.contains(marker)))
        .map(|line| line.find("=>").expect(line))
        .collect();
    assert_eq!(columns.len(), markers.len(), "{source}");
    columns
}

#[test]
fn aligns_multiline_hooks_match_arrows_and_readonly_fields() {
    let source = r#"function Form() {
  const [draft, setDraft] = useState({ name: "", email: "", handle: "", password: "" });
  const [state, submit, pending] = useActionState<FormState<null>, FormData>(
    async (_previous: FormState<null>, form: FormData): Promise<FormState<null>> => {
      return null;
    },
  );
  const reset = useCallback(() => null);
  return match (mode) {
    "signup" =>
      <>
        Already have an account? <Link to="/login">Sign in</Link>
      </>,
    "login" =>
      <>
        No account yet? <Link to="/signup">Create an account</Link>
      </>,
  };
}
function onResult(result) {
  match (result) {
    {status: "success", value: const value, ...} => {
      setCurrent(value);
    }
    {status: "error", ...} => {}
  }
  return match (data) {
    {kind: "unauthenticated"} => <SignInPrompt title="Sign in to manage your account" />,
    {kind: "ready", value: const settings} => <SettingsClient initial={settings} />,
  };
}
type Clip = {|
  readonly id: string,
  readonly title: string,
  readonly description: string,
  readonly poster: ImageSourcePropType,
  readonly credit: string,
  readonly source: string,
|};
function Session(session) {
  return match (session) {
    {kind: "guest"} => null,
    {kind: "authenticated", user: const user} =>
      <View {...stylex.props(styles.rule, local.identity)}>
        <Avatar user={user} />
        <View {...stylex.props(styles.grow)}>
          <Text {...stylex.props(local.name)}>{user.name}</Text>
          <Text {...stylex.props(styles.hint)}>@{user.handle}</Text>
        </View>
      </View>,
  };
}
"#;
    let aligned = formatted(source, true);
    let hooks: Vec<_> = aligned
        .lines()
        .filter(|line| line.contains(" = use"))
        .collect();
    assert_eq!(hooks.len(), 3, "{aligned}");
    let equals: Vec<_> = hooks
        .iter()
        .map(|line| line.find(" = use").unwrap())
        .collect();
    assert!(
        equals.iter().all(|column| *column == equals[0]),
        "{aligned}"
    );
    for (name, markers) in [
        ("mode", ["\"signup\"", "\"login\""].as_slice()),
        (
            "result",
            ["{status: \"success\"", "{status: \"error\""].as_slice(),
        ),
        (
            "data",
            ["{kind: \"unauthenticated\"}", "{kind: \"ready\""].as_slice(),
        ),
        (
            "session",
            ["{kind: \"guest\"}", "{kind: \"authenticated\""].as_slice(),
        ),
    ] {
        let columns = arrow_columns(&aligned, markers);
        assert!(
            columns.iter().all(|column| *column == columns[0]),
            "{name} arrows differ\n{aligned}"
        );
    }
    let prompt = aligned
        .lines()
        .find(|line| line.contains("SignInPrompt"))
        .expect("sign-in arm");
    assert!(prompt.len() > 100, "{prompt}");
    let fields: Vec<_> = aligned
        .lines()
        .filter(|line| line.trim_start().starts_with("readonly "))
        .map(|line| line.find(':').unwrap())
        .collect();
    assert_eq!(fields.len(), 6, "{aligned}");
    assert!(
        fields.iter().all(|column| *column == fields[0]),
        "{aligned}"
    );
}

#[test]
fn trailing_comment_breaks_type_alias_alignment() {
    let source =
        "type Short = string; // separate group\ntype LongerName = number;\ntype Mid = boolean;\n";
    let aligned = formatted(source, true);
    assert!(
        aligned.contains("type Short = string; // separate group"),
        "{aligned}"
    );
    let last_two: Vec<_> = aligned
        .lines()
        .filter(|line| line.starts_with("type LongerName") || line.starts_with("type Mid"))
        .collect();
    assert_eq!(last_two.len(), 2, "{aligned}");
    assert_eq!(last_two[0].find('='), last_two[1].find('='), "{aligned}");
}
