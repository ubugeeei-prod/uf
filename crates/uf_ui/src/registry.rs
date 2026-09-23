//! The components this uf carries, and what each one's source says it needs.

use compact_str::{CompactString, ToCompactString};

/// The version every component in this binary was written at: the uf that
/// carries it.
pub const REGISTRY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One source, as it was embedded when uf was built.
pub(crate) struct Embedded {
    pub(crate) name: &'static str,
    pub(crate) source: &'static str,
}

/// Embed `registry/ui/<name>.js` for every name given.
///
/// A macro because `include_str!` takes a literal path and nothing computed.
macro_rules! embed {
    ($($name:literal),* $(,)?) => {
        &[$(Embedded {
            name: $name,
            source: include_str!(concat!("../../../registry/ui/", $name, ".js")),
        }),*]
    };
}

/// Every component in `registry/ui/`, in alphabetical order.
///
/// Written out, because the build cannot list a directory into `include_str!`.
/// `tests.rs` holds this list to the directory in both directions, so a
/// component added there and not here — or removed there and still named here —
/// fails the suite rather than shipping a registry that disagrees with the
/// repository it came from.
pub(crate) const EMBEDDED: &[Embedded] = embed![
    "accordion",
    "alert",
    "alert-dialog",
    "avatar",
    "badge",
    "breadcrumb",
    "button",
    "calendar",
    "card",
    "carousel",
    "checkbox",
    "collapsible",
    "color-picker",
    "combobox",
    "context-menu",
    "date-field",
    "date-picker",
    "date-range-picker",
    "dialog",
    "drawer",
    "field",
    "grid-list",
    "hover-card",
    "i18n-provider",
    "input",
    "input-otp",
    "label",
    "list-box",
    "menu",
    "menubar",
    "navigation-menu",
    "number-field",
    "pagination",
    "popover",
    "progress",
    "radio-group",
    "range-calendar",
    "resizable",
    "scroll-area",
    "select",
    "separator",
    "sheet",
    "sidebar",
    "skeleton",
    "slider",
    "switch",
    "table",
    "tabs",
    "tag-group",
    "textarea",
    "time-field",
    "toast",
    "toggle",
    "toggle-group",
    "tooltip",
    "tree",
    "visually-hidden",
];

/// A component, as its source declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    /// Its name, which is its file's: `dialog` is `dialog.js`.
    pub name: &'static str,
    /// What its header says it is: the first line, after the title.
    pub description: CompactString,
    /// The npm packages it imports, by package name, sorted.
    pub dependencies: Vec<CompactString>,
    /// The other components it imports, by name, sorted.
    pub requires: Vec<CompactString>,
    /// The file, exactly as `uf ui add` writes it before the stamp.
    pub source: &'static str,
}

impl Component {
    /// The file this component is written to.
    pub fn file_name(&self) -> String {
        format!("{}.js", self.name)
    }

    fn read(name: &'static str, source: &'static str) -> Result<Self, RegistryError> {
        let description =
            description(source).ok_or(RegistryError::NoDescription { component: name })?;
        let mut dependencies: Vec<CompactString> = Vec::new();
        let mut requires: Vec<CompactString> = Vec::new();
        for specifier in imports(source) {
            let (list, entry) = match classify(specifier) {
                Import::Package(package) => (&mut dependencies, package),
                Import::Sibling(component) => (&mut requires, component),
                Import::Elsewhere => {
                    return Err(RegistryError::UnresolvedImport {
                        component: name,
                        specifier: specifier.to_compact_string(),
                    });
                }
            };
            if !list.iter().any(|seen| seen == entry) {
                list.push(entry.to_compact_string());
            }
        }
        dependencies.sort();
        requires.sort();
        Ok(Self {
            name,
            description: description.to_compact_string(),
            dependencies,
            requires,
            source,
        })
    }
}

/// Why a registry could not be read.
///
/// Every one of these is a defect in how uf was built rather than in a project,
/// and `tests.rs` fails on each before a release could carry it; they are
/// errors rather than panics so that a uf built from a broken checkout says what
/// is broken instead of aborting.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// The header does not open with `// Title: description`.
    #[error(
        "the registry's `{component}.js` does not open with a `// Title: what it is` line, which is where its description is read from"
    )]
    NoDescription { component: &'static str },
    /// An import that is neither a package nor a sibling component.
    #[error(
        "the registry's `{component}.js` imports `{specifier}`, which is neither a package nor another component in the registry"
    )]
    UnresolvedImport {
        component: &'static str,
        specifier: CompactString,
    },
    /// A sibling import naming a component the registry does not have.
    #[error(
        "the registry's `{component}.js` imports `./{required}.js`, which the registry does not have"
    )]
    MissingComponent {
        component: &'static str,
        required: CompactString,
    },
}

/// A name that is not a component in this registry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no component named `{name}`")]
pub struct UnknownComponent {
    /// The name, as it was asked for.
    pub name: String,
}

/// The registry this uf was built with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    components: Vec<Component>,
}

impl Registry {
    /// The registry embedded in this binary.
    pub fn embedded() -> Result<Self, RegistryError> {
        Self::from_sources(EMBEDDED.iter().map(|entry| (entry.name, entry.source)))
    }

    /// A registry of the given `(name, source)` pairs, for a test that needs
    /// components the real registry does not have.
    pub fn from_sources(
        sources: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> Result<Self, RegistryError> {
        let components = sources
            .into_iter()
            .map(|(name, source)| Component::read(name, source))
            .collect::<Result<Vec<_>, _>>()?;
        for component in &components {
            for required in &component.requires {
                if !components
                    .iter()
                    .any(|other| other.name == required.as_str())
                {
                    return Err(RegistryError::MissingComponent {
                        component: component.name,
                        required: required.clone(),
                    });
                }
            }
        }
        Ok(Self { components })
    }

    /// Every component, in alphabetical order.
    pub fn components(&self) -> &[Component] {
        &self.components
    }

    /// The component called `name`.
    pub fn get(&self, name: &str) -> Option<&Component> {
        self.components
            .iter()
            .find(|component| component.name == name)
    }

    /// `names` and every component they need, each once, a component's
    /// requirements before the component itself.
    ///
    /// # Errors
    ///
    /// The first name that is not a component, so the caller can say which one
    /// and offer the closest names the registry does have.
    pub fn closure(&self, names: &[&str]) -> Result<Vec<&Component>, UnknownComponent> {
        let mut ordered: Vec<&Component> = Vec::new();
        for name in names {
            let component = self.get(name).ok_or_else(|| UnknownComponent {
                name: (*name).to_owned(),
            })?;
            self.visit(component, &mut ordered, &mut Vec::new());
        }
        Ok(ordered)
    }

    /// Put `component`'s requirements, then `component`, onto `ordered`.
    ///
    /// `path` is what is being visited, so a cycle ends rather than recurses;
    /// `tests.rs` holds the registry to having none.
    fn visit<'a>(
        &'a self,
        component: &'a Component,
        ordered: &mut Vec<&'a Component>,
        path: &mut Vec<&'static str>,
    ) {
        if ordered.iter().any(|seen| seen.name == component.name) || path.contains(&component.name)
        {
            return;
        }
        path.push(component.name);
        for required in &component.requires {
            if let Some(dependency) = self.get(required) {
                self.visit(dependency, ordered, path);
            }
        }
        path.pop();
        ordered.push(component);
    }
}

/// Whether `name` is spelled the way a component's name is: lower case, digits
/// and single hyphens between words.
pub fn is_component_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// What an import specifier names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Import<'a> {
    /// An npm package, by its package name.
    Package(&'a str),
    /// Another component, by name.
    Sibling(&'a str),
    /// Anything else, which a registry component may not import.
    Elsewhere,
}

/// Read an import specifier as a package or a sibling component.
fn classify(specifier: &str) -> Import<'_> {
    if let Some(file) = specifier.strip_prefix("./") {
        return match file.strip_suffix(".js") {
            Some(stem) if is_component_name(stem) => Import::Sibling(stem),
            _ => Import::Elsewhere,
        };
    }
    if specifier.is_empty() || specifier.starts_with('.') || specifier.starts_with('/') {
        return Import::Elsewhere;
    }
    let mut segments = specifier.split('/');
    let first = segments.next().unwrap_or_default();
    let length = if first.starts_with('@') {
        match segments.next() {
            Some(second) if first.len() > 1 && !second.is_empty() => first.len() + 1 + second.len(),
            _ => return Import::Elsewhere,
        }
    } else {
        first.len()
    };
    Import::Package(&specifier[..length])
}

/// Every specifier `source` imports or re-exports from.
///
/// A line scanner rather than a parser, and it can afford to be: every file it
/// reads is in `registry/ui/`, where `uf fmt --check` holds each import to one
/// shape — `import … from "…";` on one line, or a list closed by
/// `} from "…";` — and the suite fails a file this reads differently from the
/// way it is written.
pub(crate) fn imports(source: &str) -> Vec<&str> {
    let mut found = Vec::new();
    for line in source.lines() {
        let line = line.trim_start();
        let statement = line.starts_with("import ")
            || line.starts_with("export ")
            || line.starts_with("} from ");
        if !statement {
            continue;
        }
        let rest = match line.find(" from \"") {
            Some(at) => &line[at + " from \"".len()..],
            None => match line.strip_prefix("import \"") {
                Some(rest) => rest,
                None => continue,
            },
        };
        if let Some(end) = rest.find('"') {
            found.push(&rest[..end]);
        }
    }
    found
}

/// The description a source's header opens with: the paragraph that starts
/// `// Title: description`.
///
/// Read from the first comment line after the directive and the `@flow` pragma,
/// and from the comment lines that continue it, up to the first line that is
/// only `//`. So a description wraps at the width the rest of the header does,
/// and a colon further down the header is never mistaken for a title.
pub(crate) fn description(source: &str) -> Option<String> {
    let mut lines = source.lines().map(str::trim_end);
    let opening = lines.by_ref().find(|line| {
        !(line.is_empty() || *line == "\"use client\";" || *line == "// @flow" || *line == "//")
    })?;
    let (title, first) = opening.strip_prefix("// ")?.split_once(": ")?;
    let mut description = first.trim().to_owned();
    for line in lines {
        match line.strip_prefix("// ") {
            Some(more) if !more.trim().is_empty() => {
                description.push(' ');
                description.push_str(more.trim());
            }
            _ => break,
        }
    }
    (!title.is_empty() && !description.is_empty()).then_some(description)
}
