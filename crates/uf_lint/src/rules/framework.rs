//! Descriptors for the rules uf defines itself, on top of Flow's lint set.
//!
//! These are the rules that come from uf being a toolchain rather than a type
//! checker: the router's file names, the server/client boundary, package layout,
//! the fetch client, the security set, and the accessibility and HTML-nesting
//! rules that come free with reading the parser's tree.

use uf_config::RuleLevel;

use crate::rules::RuleRequirement::SourceText;
use crate::rules::{RuleCategory, RuleDescriptor};

/// uf's own rules, on top of the Flow built-in set.
pub(crate) static OWN_RULES: &[RuleDescriptor] = &[
    RuleDescriptor {
        id: "a11y/alt-text",
        category: RuleCategory::A11y,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "`img`, `area` and `input type=\"image\"` need an `alt`",
    },
    RuleDescriptor {
        id: "a11y/aria-props",
        category: RuleCategory::A11y,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "an `aria-*` attribute must be one ARIA defines",
    },
    RuleDescriptor {
        id: "a11y/heading-order",
        category: RuleCategory::A11y,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "heading levels go down one at a time",
    },
    RuleDescriptor {
        id: "a11y/label-has-associated-control",
        category: RuleCategory::A11y,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "a `label` must name a control, by `htmlFor` or by holding it",
    },
    RuleDescriptor {
        id: "a11y/no-static-element-interactions",
        category: RuleCategory::A11y,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "an `onClick` needs an element a keyboard can reach",
    },
    RuleDescriptor {
        id: "markup/no-invalid-nesting",
        category: RuleCategory::Markup,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "nest elements the HTML parser will leave where you wrote them",
    },
    RuleDescriptor {
        id: "vite/hot-needs-optional-chaining",
        category: RuleCategory::Vite,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "reach `import.meta.hot` through `?.`; `if` does not refine it",
    },
    RuleDescriptor {
        id: "flow/syntax",
        category: RuleCategory::Flow,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "the file must parse with the official Flow parser",
    },
    RuleDescriptor {
        id: "uniflowed/no-tabs",
        category: RuleCategory::Uniflowed,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "indent with spaces, never tabs",
    },
    RuleDescriptor {
        id: "uniflowed/no-trailing-whitespace",
        category: RuleCategory::Uniflowed,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "lines must not end in whitespace",
    },
    RuleDescriptor {
        id: "uniflowed/no-npm-script-invocation",
        category: RuleCategory::Uniflowed,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "shell out to uf tasks, not `npm run`/`yarn`/`pnpm`/`bunx`",
    },
    RuleDescriptor {
        id: "uniflowed/unknown-lint-suppression",
        category: RuleCategory::Uniflowed,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "`uf-lint-disable` comments must name a rule this linter knows",
    },
    // The official React Compiler's categories, each at the level
    // `eslint-plugin-react-hooks` gives it. `hooks` is `error` although the
    // plugin leaves its compiler rule off: the plugin ships a separate
    // `rules-of-hooks` rule at `error` for the same question, and uf has no such
    // second rule — the compiler's is the answer.
    RuleDescriptor {
        id: "react-compiler/globals",
        category: RuleCategory::ReactCompiler,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "the React Compiler's `Globals` diagnostics: a variable from outside a component or hook changed during render",
    },
    RuleDescriptor {
        id: "react-compiler/hooks",
        category: RuleCategory::ReactCompiler,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "the React Compiler's `Hooks` diagnostics: the Rules of Hooks",
    },
    RuleDescriptor {
        id: "react-compiler/purity",
        category: RuleCategory::ReactCompiler,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "the React Compiler's `Purity` diagnostics: a known-impure function called during render",
    },
    RuleDescriptor {
        id: "react/component-syntax",
        category: RuleCategory::React,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "declare React components with Flow `component` syntax",
    },
    RuleDescriptor {
        id: "react/hook-syntax",
        category: RuleCategory::React,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "declare React hooks with Flow `hook` syntax",
    },
    RuleDescriptor {
        id: "react/no-derived-state-effect",
        category: RuleCategory::React,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "compute state derived from props or state during render, not in an effect",
    },
    // `warn`, not `error`, for the same reason as `react/component-syntax`: this
    // is a convention the ecosystem (and uf's own `uf create app` scaffold) is
    // still migrating to, and a linter must not fail a freshly created project.
    // It becomes an error once the scaffold ships named exports.
    RuleDescriptor {
        id: "react/no-default-export-component",
        category: RuleCategory::React,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "modules that declare components must use named exports",
    },
    RuleDescriptor {
        id: "react/no-redundant-memo",
        category: RuleCategory::React,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "drop a `useMemo`/`useCallback` the React Compiler already did",
    },
    RuleDescriptor {
        id: "react-native/platform-split",
        category: RuleCategory::ReactNative,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "prefer platform-specific files over `Platform.OS` branches",
    },
    RuleDescriptor {
        id: "server/no-client-secret",
        category: RuleCategory::Server,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "client modules must not read server secrets",
    },
    RuleDescriptor {
        id: "server/no-server-only-import-in-client",
        category: RuleCategory::Server,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "client modules must not import server-only modules",
    },
    RuleDescriptor {
        id: "server/use-client-directive-position",
        category: RuleCategory::Server,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "`use client`/`use server` must be the module's first statement",
    },
    RuleDescriptor {
        id: "server/use-server-actions",
        category: RuleCategory::Server,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "server action modules must open with `\"use server\";`",
    },
    RuleDescriptor {
        id: "router/reserved-files",
        category: RuleCategory::Router,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "`$*` file names are reserved for layout, page, and middleware",
    },
    RuleDescriptor {
        id: "router/unsupported-segment",
        category: RuleCategory::Router,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "`(.)segment` directories must be interceptions uf reads, inside a `@slot`",
    },
    RuleDescriptor {
        id: "package/no-npm-scripts",
        category: RuleCategory::Package,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "declare tasks in `uf.config.js`, not `package.json` scripts",
    },
    RuleDescriptor {
        id: "fetch/no-global-override",
        category: RuleCategory::Fetch,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "do not monkey-patch global `fetch`",
    },
    RuleDescriptor {
        id: "security/no-dangerously-set-inner-html",
        category: RuleCategory::Security,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "render HTML only through a sanitizing `@uniflowed/markdown` helper",
    },
    RuleDescriptor {
        id: "security/no-eval",
        category: RuleCategory::Security,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "never turn strings into code via `eval`, `new Function`, or timer strings",
    },
];
