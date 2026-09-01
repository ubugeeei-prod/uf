use std::collections::BTreeMap;
use std::fmt;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

pub const CONFIG_FILES: &[&str] = &["uniflowed.config.flow"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UniflowedConfig {
    pub app: AppConfig,
    pub build: BuildConfig,
    pub dev: DevConfig,
    pub docs: DocsConfig,
    pub env: EnvConfig,
    pub fmt: FmtConfig,
    pub lint: LintConfig,
    pub publish: PublishConfig,
    pub tasks: BTreeMap<CompactString, TaskDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub component_default: ComponentBoundary,
    pub framework: FrameworkPreset,
    pub react: ReactConfig,
    pub rendering: RenderingConfig,
    pub router: RouterConfig,
    pub rsc: bool,
    pub server_actions: bool,
    pub orm: OrmConfig,
    pub builtins: BuiltinConfig,
    pub targets: Vec<RuntimeTarget>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            component_default: ComponentBoundary::Server,
            framework: FrameworkPreset::Uniflowed,
            react: ReactConfig::default(),
            rendering: RenderingConfig::default(),
            router: RouterConfig::default(),
            rsc: true,
            server_actions: true,
            orm: OrmConfig::default(),
            builtins: BuiltinConfig::default(),
            targets: vec![
                RuntimeTarget::Web,
                RuntimeTarget::ReactNative,
                RuntimeTarget::Server,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrameworkPreset {
    Uniflowed,
    React,
    ReactNative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RouterConfig {
    pub enabled: bool,
    pub entry: CompactString,
    pub manifest: CompactString,
    pub root: CompactString,
    pub convention: RouterConvention,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            entry: CompactString::const_new("app.flow"),
            manifest: CompactString::const_new("router.flow"),
            root: CompactString::const_new("app"),
            convention: RouterConvention::FileSystem,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouterConvention {
    FileSystem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OrmConfig {
    pub enabled: bool,
    pub module: CompactString,
}

impl Default for OrmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            module: CompactString::const_new("@uniflowed/orm"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BuiltinConfig {
    pub data: DataEngine,
    pub effect: EffectEngine,
    pub flow_cell: bool,
    pub framework_lints: bool,
    pub native_test_runner: bool,
    pub react_compiler: ReactCompilerConfig,
    pub react_testing_library: bool,
    pub relay: bool,
    pub style: StyleEngine,
}

impl Default for BuiltinConfig {
    fn default() -> Self {
        Self {
            data: DataEngine::UniflowedQuery,
            effect: EffectEngine::UniflowedEffect,
            flow_cell: true,
            framework_lints: true,
            native_test_runner: true,
            react_compiler: ReactCompilerConfig::default(),
            react_testing_library: true,
            relay: true,
            style: StyleEngine::StyleX,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StyleEngine {
    StyleX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataEngine {
    UniflowedQuery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectEngine {
    UniflowedEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReactCompilerConfig {
    pub enabled: bool,
    pub mode: ReactCompilerMode,
}

impl Default for ReactCompilerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: ReactCompilerMode::Syntax,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactCompilerMode {
    Syntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeTarget {
    Web,
    ReactNative,
    Server,
    Hermes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentBoundary {
    Server,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReactConfig {
    pub version: CompactString,
    pub async_react: bool,
    pub suspense: bool,
    pub use_hook: bool,
}

impl Default for ReactConfig {
    fn default() -> Self {
        Self {
            version: CompactString::const_new("19"),
            async_react: true,
            suspense: true,
            use_hook: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RenderingConfig {
    pub modes: Vec<RenderingMode>,
    pub cache: CacheConfig,
}

impl Default for RenderingConfig {
    fn default() -> Self {
        Self {
            modes: vec![
                RenderingMode::Ppr,
                RenderingMode::Ssr,
                RenderingMode::Ssg,
                RenderingMode::Isr,
            ],
            cache: CacheConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingMode {
    Ppr,
    Ssr,
    Ssg,
    Isr,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CacheConfig {
    pub actions: bool,
    pub data: bool,
    pub fetch: bool,
    pub route: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BuildConfig {
    pub entries: Vec<CompactString>,
    pub hooks: BTreeMap<CompactString, TaskDefinition>,
    pub out_dir: CompactString,
    pub static_build: bool,
    pub sourcemap: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            entries: vec![CompactString::const_new("app.flow")],
            hooks: BTreeMap::new(),
            out_dir: CompactString::const_new("dist"),
            static_build: false,
            sourcemap: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DocsConfig {
    pub enabled: bool,
    pub app: CompactString,
    pub source: CompactString,
    pub out_dir: CompactString,
    pub static_build: bool,
    pub deploy: DeployTarget,
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            app: CompactString::const_new("docs-site/app.flow"),
            source: CompactString::const_new("docs"),
            out_dir: CompactString::const_new("dist/docs"),
            static_build: true,
            deploy: DeployTarget::Void,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeployTarget {
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DevConfig {
    pub host: CompactString,
    pub port: u16,
    pub strict_port: bool,
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            host: CompactString::const_new("127.0.0.1"),
            port: 5173,
            strict_port: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EnvConfig {
    pub active: CompactString,
    pub files: Vec<CompactString>,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            active: CompactString::const_new("development"),
            files: vec![
                CompactString::const_new(".env"),
                CompactString::const_new(".env.local"),
                CompactString::const_new(".env.development"),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FmtConfig {
    pub indent_width: u8,
    pub line_width: u16,
    pub quotes: QuoteStyle,
    pub semicolons: bool,
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            line_width: 100,
            quotes: QuoteStyle::Single,
            semicolons: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuoteStyle {
    Single,
    Double,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LintConfig {
    pub files: Vec<CompactString>,
    pub ignore: Vec<CompactString>,
    pub rules: BTreeMap<CompactString, RuleLevel>,
}

impl Default for LintConfig {
    fn default() -> Self {
        let mut rules = BTreeMap::new();
        rules.insert(CompactString::const_new("flow/syntax"), RuleLevel::Error);
        rules.insert(
            CompactString::const_new("flow/type-aware/no-explicit-any"),
            RuleLevel::Error,
        );
        rules.insert(
            CompactString::const_new("uniflowed/no-tabs"),
            RuleLevel::Error,
        );
        rules.insert(
            CompactString::const_new("uniflowed/no-trailing-whitespace"),
            RuleLevel::Error,
        );
        rules.insert(
            CompactString::const_new("react/component-syntax"),
            RuleLevel::Warn,
        );
        rules.insert(
            CompactString::const_new("react/hook-syntax"),
            RuleLevel::Warn,
        );
        rules.insert(
            CompactString::const_new("react-native/platform-split"),
            RuleLevel::Warn,
        );
        rules.insert(
            CompactString::const_new("react/no-render-side-effects"),
            RuleLevel::Error,
        );
        rules.insert(
            CompactString::const_new("server/no-client-secret"),
            RuleLevel::Error,
        );
        rules.insert(
            CompactString::const_new("server/use-server-actions"),
            RuleLevel::Error,
        );
        rules.insert(
            CompactString::const_new("router/reserved-files"),
            RuleLevel::Error,
        );

        Self {
            files: vec![
                CompactString::const_new("app"),
                CompactString::const_new("packages"),
                CompactString::const_new("server"),
                CompactString::const_new("tests"),
            ],
            ignore: vec![
                CompactString::const_new("node_modules"),
                CompactString::const_new("dist"),
                CompactString::const_new("target"),
            ],
            rules,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleLevel {
    Off,
    Warn,
    Error,
}

impl RuleLevel {
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    pub fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

impl<'de> Deserialize<'de> for RuleLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = RuleLevel;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("\"off\", \"warn\", \"error\", false, true, 0, 1, or 2")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(if value {
                    RuleLevel::Error
                } else {
                    RuleLevel::Off
                })
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    0 => Ok(RuleLevel::Off),
                    1 => Ok(RuleLevel::Warn),
                    2 => Ok(RuleLevel::Error),
                    _ => Err(E::custom(format!("unsupported rule level {value}"))),
                }
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "off" => Ok(RuleLevel::Off),
                    "warn" | "warning" => Ok(RuleLevel::Warn),
                    "error" => Ok(RuleLevel::Error),
                    _ => Err(E::custom(format!("unsupported rule level {value:?}"))),
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PublishConfig {
    pub registry: CompactString,
    pub dry_run: bool,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            registry: CompactString::const_new("https://registry.npmjs.org"),
            dry_run: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TaskDefinition {
    Command(CompactString),
    Detailed(TaskCommand),
}

impl TaskDefinition {
    pub fn command(&self) -> &str {
        match self {
            Self::Command(command) => command.as_str(),
            Self::Detailed(task) => task.command.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TaskCommand {
    pub command: CompactString,
    pub cwd: Option<CompactString>,
    pub depends_on: Vec<CompactString>,
    pub env: BTreeMap<CompactString, CompactString>,
}

impl Default for TaskCommand {
    fn default() -> Self {
        Self {
            command: CompactString::new(""),
            cwd: None,
            depends_on: Vec::new(),
            env: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedConfig {
    pub root: Utf8PathBuf,
    pub config_path: Option<Utf8PathBuf>,
    pub config: UniflowedConfig,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {message}")]
    Parse { path: Utf8PathBuf, message: String },
    #[error(
        "unsupported config expression in {path}; use `export default defineConfig({{ ... }})`"
    )]
    UnsupportedExpression { path: Utf8PathBuf },
}

pub fn load_config(start: impl AsRef<Utf8Path>) -> Result<ResolvedConfig, ConfigError> {
    let start = start.as_ref();
    let root = discover_root(start);
    let config_path = discover_config(&root);

    let config = match &config_path {
        Some(path) => load_config_file(path)?,
        None => UniflowedConfig::default(),
    };

    Ok(ResolvedConfig {
        root,
        config_path,
        config,
    })
}

pub fn discover_root(start: &Utf8Path) -> Utf8PathBuf {
    let mut current = if start.is_file() {
        start.parent().unwrap_or(start).to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        if CONFIG_FILES.iter().any(|name| current.join(name).exists())
            || current.join("package.json").exists()
            || current.join(".git").exists()
        {
            return current;
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return start.to_path_buf(),
        }
    }
}

pub fn discover_config(root: &Utf8Path) -> Option<Utf8PathBuf> {
    CONFIG_FILES
        .iter()
        .map(|file| root.join(file))
        .find(|path| path.exists())
}

pub fn load_config_file(path: &Utf8Path) -> Result<UniflowedConfig, ConfigError> {
    let source = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    match path.extension() {
        Some("flow") => {
            let json5 = extract_config_object(&source).ok_or_else(|| {
                ConfigError::UnsupportedExpression {
                    path: path.to_path_buf(),
                }
            })?;
            json5::from_str(&json5).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                message: source.to_string(),
            })
        }
        _ => Err(ConfigError::UnsupportedExpression {
            path: path.to_path_buf(),
        }),
    }
}

pub fn extract_config_object(source: &str) -> Option<String> {
    let without_imports = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("import "))
        .collect::<Vec<_>>()
        .join("\n");
    let expression = without_imports.trim().trim_end_matches(';').trim();
    let expression = expression
        .strip_prefix("export default")
        .map(str::trim)
        .unwrap_or(expression);

    if expression.starts_with("defineConfig") {
        let open = expression.find('(')?;
        let call = extract_balanced(&expression[open..], '(', ')')?;
        let inner = &call[1..call.len() - 1];
        let inner = inner.trim();
        if inner.starts_with('{') {
            return extract_balanced(inner, '{', '}');
        }
        return None;
    }

    if expression.starts_with('{') {
        return extract_balanced(expression, '{', '}');
    }

    None
}

fn extract_balanced(source: &str, open: char, close: char) -> Option<String> {
    let mut chars = source.char_indices();
    let (_, first) = chars.next()?;
    if first != open {
        return None;
    }

    let mut depth = 1usize;
    let mut string_quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut previous = '\0';

    for (index, ch) in chars {
        if line_comment {
            if ch == '\n' {
                line_comment = false;
            }
            previous = ch;
            continue;
        }

        if block_comment {
            if previous == '*' && ch == '/' {
                block_comment = false;
            }
            previous = ch;
            continue;
        }

        if let Some(quote) = string_quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                string_quote = None;
            }
            previous = ch;
            continue;
        }

        match ch {
            '"' | '\'' | '`' => string_quote = Some(ch),
            '/' if previous == '/' => line_comment = true,
            '*' if previous == '/' => block_comment = true,
            current if current == open => depth += 1,
            current if current == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(source[..=index].to_string());
                }
            }
            _ => {}
        }
        previous = ch;
    }

    None
}

pub fn define_config(config: UniflowedConfig) -> UniflowedConfig {
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_config_defaults_to_flow_react_app_stack() {
        let config = UniflowedConfig::default();

        assert!(config.app.router.enabled);
        assert_eq!(config.app.router.entry, "app.flow");
        assert_eq!(config.app.router.root, "app");
        assert_eq!(config.app.component_default, ComponentBoundary::Server);
        assert_eq!(config.app.react.version, "19");
        assert!(config.app.react.async_react);
        assert!(config.app.react.suspense);
        assert!(config.app.react.use_hook);
        assert!(config.app.rsc);
        assert!(config.app.server_actions);
        assert!(!config.app.rendering.cache.fetch);
        assert!(!config.app.rendering.cache.route);
        assert!(config.app.rendering.modes.contains(&RenderingMode::Ppr));
        assert!(config.app.rendering.modes.contains(&RenderingMode::Isr));
        assert!(config.app.builtins.flow_cell);
        assert!(config.app.builtins.react_testing_library);
        assert!(config.app.builtins.relay);
        assert_eq!(config.app.builtins.style, StyleEngine::StyleX);
        assert_eq!(config.app.builtins.data, DataEngine::UniflowedQuery);
        assert_eq!(config.app.builtins.effect, EffectEngine::UniflowedEffect);
        assert_eq!(
            config.app.builtins.react_compiler.mode,
            ReactCompilerMode::Syntax
        );
        assert!(config.app.targets.contains(&RuntimeTarget::ReactNative));
        assert!(config.docs.enabled);
        assert!(config.docs.static_build);
        assert_eq!(config.docs.deploy, DeployTarget::Void);
        assert_eq!(config.lint.rules["flow/syntax"], RuleLevel::Error);
    }

    #[test]
    fn extracts_vite_style_define_config_object() {
        let source = r#"
            import { defineConfig } from "@uniflowed/config";

            export default defineConfig({
              dev: { port: 4111 },
              lint: {
                rules: {
                  "uniflowed/no-tabs": "off",
                  "react/component-syntax": "error",
                },
              },
              tasks: {
                storybook: "vite --host 0.0.0.0",
              },
            });
        "#;

        let object = extract_config_object(source).expect("object");
        let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

        assert_eq!(parsed.dev.port, 4111);
        assert_eq!(parsed.lint.rules["uniflowed/no-tabs"], RuleLevel::Off);
        assert_eq!(
            parsed.lint.rules["react/component-syntax"],
            RuleLevel::Error
        );
        assert_eq!(parsed.tasks["storybook"].command(), "vite --host 0.0.0.0");
    }

    #[test]
    fn extracts_plain_export_default_object_with_satisfies_tail() {
        let source = r#"
            export default {
              fmt: { lineWidth: 88 },
            } satisfies UniflowedConfig;
        "#;

        let object = extract_config_object(source).expect("object");
        let parsed: UniflowedConfig = json5::from_str(&object).expect("config");

        assert_eq!(parsed.fmt.line_width, 88);
    }

    #[test]
    fn parses_flow_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("uniflowed.config.flow")).unwrap();
        fs::write(
            &path,
            r#"
                export default defineConfig({
                  dev: { port: 3000 },
                  app: { builtins: { flowCell: false } },
                });
            "#,
        )
        .unwrap();

        let config = load_config_file(&path).unwrap();

        assert_eq!(config.dev.port, 3000);
        assert!(!config.app.builtins.flow_cell);
        assert!(config.app.builtins.native_test_runner);
    }

    #[test]
    fn discovers_config_from_child_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::write(
            root.join("uniflowed.config.flow"),
            "export default defineConfig({});",
        )
        .unwrap();
        fs::create_dir_all(root.join("src/app")).unwrap();

        let resolved = load_config(root.join("src/app")).unwrap();

        assert_eq!(resolved.root, root);
        assert_eq!(
            resolved.config_path.unwrap().file_name(),
            Some("uniflowed.config.flow")
        );
    }
}
