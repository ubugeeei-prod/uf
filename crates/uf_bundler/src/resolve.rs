//! Turning an import specifier into a module inside the project, or nothing.
//!
//! # Guards
//!
//! Specifiers come from source files, and source files come from
//! `node_modules`. Everything below is therefore a refusal rather than a
//! repair, and the lexical half of it is [`uf_rsc::resolve_specifier`] — the
//! same guard the RSC graph uses, so there is one implementation of "does this
//! climb out of the project?" and not two that can disagree.
//!
//! On top of that:
//!
//! * a specifier holding a control byte, a backslash, a `~`, a leading `/`, or
//!   a URL scheme is refused, on every platform — `\` is a separator
//!   everywhere, which is the shape of both the pnpm tarball traversal and the
//!   Vite-era dev-server bypasses;
//! * a resolved file that is a symbolic link is refused, so a link inside
//!   `node_modules` cannot pull a file from outside the project into a chunk;
//! * `package.json#exports` targets must be relative and non-climbing, so a
//!   package cannot export a path outside itself;
//! * every walk is bounded: specifier length, `node_modules` depth, manifest
//!   size, and `exports` nesting.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use thiserror::Error;
use uf_infra::FxHashMap;
use uf_rsc::SpecifierResolution;

pub mod exports;
pub mod manifest;

pub use exports::{CONDITIONS, resolve_exports};
pub use manifest::{POISONED_KEYS, PackageManifest, SideEffectsField};

use crate::limits::{BundlerLimits, LimitError};

/// The package every bare `@uniflowed/*` specifier lives inside.
///
/// `@uniflowed/react` is the subpath `./react` of `@uniflowed/core`, which is
/// how the toolchain ships one package with per-module subpath exports instead
/// of forty packages or one barrel.
pub const UNIFLOWED_PACKAGE: &str = "@uniflowed/core";

/// The scope those specifiers use.
pub const UNIFLOWED_SCOPE: &str = "@uniflowed/";

/// The directory bare specifiers are looked up in.
pub const NODE_MODULES: &str = "node_modules";

/// What a specifier resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A module inside the project, as a project-relative path.
    Module(Utf8PathBuf),
    /// Nothing the bundler will read; the runtime resolves it.
    External(CompactString),
}

/// Why a specifier could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    /// The specifier is empty.
    #[error("{importer} imports an empty specifier")]
    Empty {
        /// The module holding the import.
        importer: Utf8PathBuf,
    },
    /// The specifier holds a control byte.
    #[error("specifier {specifier:?} holds a control byte at offset {offset}")]
    ControlByte {
        /// The rejected specifier.
        specifier: CompactString,
        /// Byte offset of the first control byte.
        offset: usize,
    },
    /// The specifier holds a backslash, which uf treats as a separator.
    #[error("specifier {specifier:?} holds a path separator `\\` at offset {offset}")]
    BackslashSeparator {
        /// The rejected specifier.
        specifier: CompactString,
        /// Byte offset of the first backslash.
        offset: usize,
    },
    /// The specifier is relative to the user's home directory.
    #[error("specifier {specifier:?} is home-relative")]
    HomeRelative {
        /// The rejected specifier.
        specifier: CompactString,
    },
    /// The specifier is an absolute path.
    #[error("specifier {specifier:?} is an absolute path")]
    Absolute {
        /// The rejected specifier.
        specifier: CompactString,
    },
    /// The specifier is a URL, or a Windows drive-qualified path.
    #[error("specifier {specifier:?} names a {scheme:?} URL or drive, not a module")]
    UrlScheme {
        /// The rejected specifier.
        specifier: CompactString,
        /// The scheme found before the colon.
        scheme: CompactString,
    },
    /// The specifier climbs above the project root.
    #[error("{importer} imports {specifier:?}, which climbs out of the project root")]
    EscapesProjectRoot {
        /// The module holding the import.
        importer: Utf8PathBuf,
        /// The rejected specifier.
        specifier: CompactString,
    },
    /// No file matched a relative or package-internal specifier.
    #[error("{importer} imports {specifier:?}, which does not exist")]
    NotFound {
        /// The module holding the import.
        importer: Utf8PathBuf,
        /// The specifier that matched nothing.
        specifier: CompactString,
    },
    /// The resolved file is a symbolic link.
    #[error("{module} is a symbolic link, which the bundler will not follow")]
    Symlink {
        /// The link that was refused.
        module: Utf8PathBuf,
    },
    /// A ceiling was reached while resolving.
    #[error(transparent)]
    Limit(#[from] LimitError),
}

/// Resolves specifiers against one project, caching what it reads.
#[derive(Debug)]
pub struct Resolver {
    root: Utf8PathBuf,
    limits: BundlerLimits,
    manifests: FxHashMap<Utf8PathBuf, PackageManifest>,
}

impl Resolver {
    /// A resolver rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<Utf8PathBuf>, limits: BundlerLimits) -> Self {
        Self {
            root: root.into(),
            limits,
            manifests: FxHashMap::default(),
        }
    }

    /// The project root every resolved path is relative to.
    #[must_use]
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// The ceilings this resolver enforces.
    #[must_use]
    pub const fn limits(&self) -> &BundlerLimits {
        &self.limits
    }

    /// Resolve `specifier` as written in `importer`.
    pub fn resolve(
        &mut self,
        importer: &Utf8Path,
        specifier: &str,
    ) -> Result<Resolution, ResolveError> {
        if specifier.is_empty() {
            return Err(ResolveError::Empty {
                importer: importer.to_path_buf(),
            });
        }
        match classify(specifier, &self.limits)? {
            Specifier::Runtime => Ok(Resolution::External(CompactString::new(specifier))),
            Specifier::Relative => self.resolve_relative(importer, specifier),
            Specifier::Package { package, subpath } => {
                self.resolve_package(importer, specifier, &package, &subpath)
            }
        }
    }

    /// The manifest of the package a module belongs to.
    ///
    /// Walks up from the module's directory to the project root, so a module in
    /// `node_modules/x/src/y.js` sees `node_modules/x/package.json` and a module
    /// in `app/` sees the project's own.
    pub fn manifest_for(&mut self, module: &Utf8Path) -> Result<&PackageManifest, LimitError> {
        let mut directory = module.parent().unwrap_or(Utf8Path::new("")).to_path_buf();
        let mut steps = 0usize;

        loop {
            if self.root.join(&directory).join("package.json").is_file()
                || directory.as_str().is_empty()
                || steps >= self.limits.max_node_modules_depth
            {
                break;
            }
            let Some(parent) = directory.parent() else {
                break;
            };
            directory = parent.to_path_buf();
            steps += 1;
        }

        if !self.manifests.contains_key(&directory) {
            let manifest = PackageManifest::read(&self.root, &directory, &self.limits)?;
            self.manifests.insert(directory.clone(), manifest);
        }
        Ok(&self.manifests[&directory])
    }

    fn resolve_relative(
        &self,
        importer: &Utf8Path,
        specifier: &str,
    ) -> Result<Resolution, ResolveError> {
        match uf_rsc::resolve_specifier(importer, specifier) {
            SpecifierResolution::Relative(candidate) => match self.find_file(&candidate)? {
                Some(path) => Ok(Resolution::Module(path)),
                None => Err(ResolveError::NotFound {
                    importer: importer.to_path_buf(),
                    specifier: CompactString::new(specifier),
                }),
            },
            SpecifierResolution::Escapes => Err(ResolveError::EscapesProjectRoot {
                importer: importer.to_path_buf(),
                specifier: CompactString::new(specifier),
            }),
            // `classify` already decided this is relative.
            SpecifierResolution::Bare => Err(ResolveError::NotFound {
                importer: importer.to_path_buf(),
                specifier: CompactString::new(specifier),
            }),
        }
    }

    fn resolve_package(
        &mut self,
        importer: &Utf8Path,
        specifier: &str,
        package: &str,
        subpath: &str,
    ) -> Result<Resolution, ResolveError> {
        let Some(directory) = self.package_directory(importer, package) else {
            return Ok(Resolution::External(CompactString::new(specifier)));
        };

        let manifest = PackageManifest::read(&self.root, &directory, &self.limits)?;
        let target = match &manifest.exports {
            Some(exports) => resolve_exports(exports, subpath, package, &self.limits)?,
            None if subpath == "." => Some(
                manifest
                    .main
                    .as_deref()
                    .map_or_else(|| String::from("index.js"), str::to_string),
            ),
            None => Some(subpath.trim_start_matches("./").to_string()),
        };
        self.manifests.insert(directory.clone(), manifest);

        let Some(target) = target else {
            return Err(ResolveError::NotFound {
                importer: importer.to_path_buf(),
                specifier: CompactString::new(specifier),
            });
        };

        match self.find_file(&directory.join(target))? {
            Some(path) => Ok(Resolution::Module(path)),
            None => Err(ResolveError::NotFound {
                importer: importer.to_path_buf(),
                specifier: CompactString::new(specifier),
            }),
        }
    }

    /// Walk up from the importer looking for `node_modules/<package>`.
    ///
    /// The walk stops at the project root, so a package installed above the
    /// project — in a parent directory, or in the user's home — is never
    /// reached and the build stays reproducible from the repository alone.
    fn package_directory(&self, importer: &Utf8Path, package: &str) -> Option<Utf8PathBuf> {
        let mut directory = importer.parent().unwrap_or(Utf8Path::new("")).to_path_buf();

        for _ in 0..self.limits.max_node_modules_depth {
            let candidate = directory.join(NODE_MODULES).join(package);
            if self.root.join(&candidate).is_dir() {
                return Some(candidate);
            }
            if directory.as_str().is_empty() {
                return None;
            }
            directory = directory
                .parent()
                .unwrap_or(Utf8Path::new(""))
                .to_path_buf();
        }

        None
    }

    /// Try the path itself, then `.js`, then `/index.js`.
    fn find_file(&self, candidate: &Utf8Path) -> Result<Option<Utf8PathBuf>, ResolveError> {
        let normalized = uf_rsc::normalize_module_path(candidate);
        if !uf_rsc::is_inside_project(&normalized) {
            return Ok(None);
        }

        let with_extension = Utf8PathBuf::from(format!("{normalized}.js"));
        let with_index = normalized.join("index.js");
        for candidate in [&normalized, &with_extension, &with_index] {
            let absolute = self.root.join(candidate);
            let Ok(metadata) = std::fs::symlink_metadata(&absolute) else {
                continue;
            };
            if metadata.is_symlink() {
                return Err(ResolveError::Symlink {
                    module: candidate.clone(),
                });
            }
            if metadata.is_file() {
                return Ok(Some(candidate.clone()));
            }
        }

        Ok(None)
    }
}

/// What kind of thing a specifier names, once its bytes are known to be safe.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Specifier {
    /// `./x.js` or `../x.js`.
    Relative,
    /// A package and a subpath: `("react", ".")`, `("@uniflowed/core", "./react")`.
    Package {
        package: CompactString,
        subpath: CompactString,
    },
    /// A `node:` builtin, which is never read from disk.
    Runtime,
}

/// Check a specifier's bytes and say what it names.
fn classify(specifier: &str, limits: &BundlerLimits) -> Result<Specifier, ResolveError> {
    if specifier.is_empty() {
        return Err(ResolveError::Empty {
            importer: Utf8PathBuf::new(),
        });
    }
    if specifier.len() > limits.max_specifier_bytes {
        return Err(LimitError::SpecifierTooLong {
            bytes: specifier.len(),
            limit: limits.max_specifier_bytes,
        }
        .into());
    }

    let bytes = specifier.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte < 0x20 || byte == 0x7f {
            return Err(ResolveError::ControlByte {
                specifier: CompactString::new(specifier),
                offset,
            });
        }
        if byte == b'\\' {
            return Err(ResolveError::BackslashSeparator {
                specifier: CompactString::new(specifier),
                offset,
            });
        }
    }
    if bytes[0] == b'~' {
        return Err(ResolveError::HomeRelative {
            specifier: CompactString::new(specifier),
        });
    }
    if bytes[0] == b'/' {
        return Err(ResolveError::Absolute {
            specifier: CompactString::new(specifier),
        });
    }
    if let Some(scheme) = leading_scheme(specifier) {
        // `node:` names a runtime builtin; every other scheme is a request to
        // read from somewhere that is not this project.
        if scheme == "node" {
            return Ok(Specifier::Runtime);
        }
        return Err(ResolveError::UrlScheme {
            specifier: CompactString::new(specifier),
            scheme: CompactString::new(scheme),
        });
    }

    if specifier.starts_with("./") || specifier.starts_with("../") || specifier == "." {
        return Ok(Specifier::Relative);
    }

    let (package, subpath) = split_package(specifier);
    Ok(map_uniflowed(package, subpath))
}

/// Split `@scope/name/sub/path` or `name/sub/path` into package and subpath.
fn split_package(specifier: &str) -> (&str, &str) {
    let boundary = if specifier.starts_with('@') {
        specifier
            .match_indices('/')
            .nth(1)
            .map(|(index, _)| index)
            .unwrap_or(specifier.len())
    } else {
        specifier.find('/').unwrap_or(specifier.len())
    };
    (&specifier[..boundary], &specifier[boundary..])
}

/// Rewrite `@uniflowed/react` into `@uniflowed/core` subpath `./react`.
fn map_uniflowed(package: &str, subpath: &str) -> Specifier {
    let subpath = if subpath.is_empty() {
        CompactString::const_new(".")
    } else {
        CompactString::new(format!(".{subpath}"))
    };

    if package == UNIFLOWED_PACKAGE {
        return Specifier::Package {
            package: CompactString::const_new(UNIFLOWED_PACKAGE),
            subpath,
        };
    }
    let Some(name) = package.strip_prefix(UNIFLOWED_SCOPE) else {
        return Specifier::Package {
            package: CompactString::new(package),
            subpath,
        };
    };

    let subpath = match subpath.as_str() {
        "." => CompactString::new(format!("./{name}")),
        rest => CompactString::new(format!("./{name}{}", &rest[1..])),
    };
    Specifier::Package {
        package: CompactString::const_new(UNIFLOWED_PACKAGE),
        subpath,
    }
}

/// The `scheme` of `scheme:rest`, when the prefix is a valid URL scheme.
///
/// A single Windows drive letter matches too, which is deliberate: `C:/x` is an
/// absolute path wearing a scheme's clothes, and both are refused.
fn leading_scheme(specifier: &str) -> Option<&str> {
    let colon = specifier.find(':')?;
    let scheme = &specifier[..colon];
    let mut bytes = scheme.bytes();
    let first = bytes.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    bytes
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
        .then_some(scheme)
}
