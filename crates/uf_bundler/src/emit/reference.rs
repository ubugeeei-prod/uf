//! Naming: what a chunk calls each module and each thing it imports.
//!
//! Every name a chunk emits is derived, never invented per run: a module's
//! cross-chunk symbol is a hash of its path, its in-chunk name is its position
//! in the chunk, and an external import's alias is its position in a list built
//! by walking modules in order. Two builds of the same input therefore produce
//! the same identifiers, which is half of why the output is byte-identical.

use compact_str::CompactString;
use uf_infra::FxHashMap;

use crate::graph::ModuleIndex;
use crate::hash::ContentHasher;

/// Prefix for the local name of a module inside its own chunk.
pub const MODULE_PREFIX: &str = "__uf_m";

/// Prefix for the function that evaluates one module.
///
/// A named function declaration rather than an immediately-invoked arrow: it
/// costs nothing at runtime, it puts the module's own name in a stack trace,
/// and it keeps the module body four expression levels shallower, which is what
/// a JavaScript-hosted parser's recursion budget notices.
pub const INIT_PREFIX: &str = "__uf_init";

/// Prefix for a module namespace imported from another chunk.
pub const CROSS_PREFIX: &str = "__uf_c";

/// Prefix for a binding imported from outside the bundle.
pub const EXTERNAL_PREFIX: &str = "__uf_x";

/// Prefix for the symbol a chunk exports a module namespace under.
pub const SYMBOL_PREFIX: &str = "uf_";

/// Helper that turns a namespace into the shape `export *` needs.
pub const STAR_HELPER: &str = "__uf_star";

/// What a chunk needs from a package it does not bundle.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExternalKind {
    /// `import "…"`, for the side effects alone.
    Bare,
    /// `import local from "…"`.
    Default,
    /// `import { name as local } from "…"`.
    Named(CompactString),
    /// `import * as local from "…"`.
    Namespace,
}

/// One hoisted `import` statement at the top of a chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalImport {
    /// The specifier, exactly as the author wrote it.
    pub specifier: CompactString,
    /// What is taken from it.
    pub kind: ExternalKind,
    /// The local alias, or empty for [`ExternalKind::Bare`].
    pub local: CompactString,
}

impl ExternalImport {
    /// The statement this import renders as.
    #[must_use]
    pub fn render(&self) -> String {
        let specifier = &self.specifier;
        match &self.kind {
            ExternalKind::Bare => format!("import {};\n", quote(specifier)),
            ExternalKind::Default => {
                format!("import {} from {};\n", self.local, quote(specifier))
            }
            ExternalKind::Namespace => {
                format!("import * as {} from {};\n", self.local, quote(specifier))
            }
            ExternalKind::Named(name) => format!(
                "import {{ {} as {} }} from {};\n",
                name,
                self.local,
                quote(specifier)
            ),
        }
    }
}

/// The names one chunk uses, collected before anything is rendered.
#[derive(Debug, Default)]
pub struct References {
    local: FxHashMap<ModuleIndex, CompactString>,
    cross: FxHashMap<ModuleIndex, CompactString>,
    cross_order: Vec<(usize, ModuleIndex)>,
    externals: Vec<ExternalImport>,
    external_index: FxHashMap<(CompactString, ExternalKind), usize>,
}

impl References {
    /// Name the modules this chunk owns.
    pub fn with_locals(modules: &[ModuleIndex]) -> Self {
        let local = modules
            .iter()
            .enumerate()
            .map(|(position, index)| {
                (
                    *index,
                    CompactString::new(format!("{MODULE_PREFIX}{position}")),
                )
            })
            .collect();
        Self {
            local,
            ..Self::default()
        }
    }

    /// Whether the chunk owns `module`.
    #[must_use]
    pub fn owns(&self, module: ModuleIndex) -> bool {
        self.local.contains_key(&module)
    }

    /// The expression that names `module`, importing it if it lives elsewhere.
    pub fn module_reference(&mut self, module: ModuleIndex, chunk: usize) -> CompactString {
        if let Some(name) = self.local.get(&module) {
            return name.clone();
        }
        if let Some(name) = self.cross.get(&module) {
            return name.clone();
        }
        let name = CompactString::new(format!("{CROSS_PREFIX}{}", self.cross.len()));
        self.cross.insert(module, name.clone());
        self.cross_order.push((chunk, module));
        name
    }

    /// The alias for one thing taken from outside the bundle.
    pub fn external(&mut self, specifier: &str, kind: ExternalKind) -> CompactString {
        let key = (CompactString::new(specifier), kind.clone());
        if let Some(position) = self.external_index.get(&key) {
            return self.externals[*position].local.clone();
        }

        let local = if kind == ExternalKind::Bare {
            CompactString::default()
        } else {
            CompactString::new(format!("{EXTERNAL_PREFIX}{}", self.externals.len()))
        };
        self.external_index.insert(key, self.externals.len());
        self.externals.push(ExternalImport {
            specifier: CompactString::new(specifier),
            kind,
            local: local.clone(),
        });
        local
    }

    /// Modules imported from other chunks, in the order they were first needed.
    #[must_use]
    pub fn cross_imports(&self) -> &[(usize, ModuleIndex)] {
        &self.cross_order
    }

    /// The alias a cross-chunk module was given.
    #[must_use]
    pub fn cross_alias(&self, module: ModuleIndex) -> Option<&CompactString> {
        self.cross.get(&module)
    }

    /// Hoisted external imports, in the order they were first needed.
    #[must_use]
    pub fn externals(&self) -> &[ExternalImport] {
        &self.externals
    }
}

/// The symbol a chunk exports a module's namespace under.
///
/// Derived from the module path so the name is stable across builds and unique
/// without a counter, and prefixed so it is always a valid identifier whatever
/// the path looked like.
#[must_use]
pub fn module_symbol(path: &camino::Utf8Path) -> CompactString {
    let digest = ContentHasher::new().with(path.as_str().as_bytes()).finish();
    CompactString::new(format!("{SYMBOL_PREFIX}{digest}"))
}

/// A JavaScript string literal for `value`.
///
/// Escapes everything a specifier could carry into a source file: quotes,
/// backslashes, line terminators, and the two Unicode separators that end a
/// line in JavaScript but not in most editors.
#[must_use]
pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            character if (character as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", character as u32);
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Whether `name` can be written as a bare identifier in emitted code.
#[must_use]
pub fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$')
}
