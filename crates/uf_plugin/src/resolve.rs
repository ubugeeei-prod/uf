//! Turning `plugins: [...]` into descriptors, and refusing the entries that are
//! really a request to run code from somewhere else on the machine.
//!
//! A plugin name is untrusted input. `uf install` and `uf build` run on freshly
//! cloned repositories, so a config that can name `../../../.ssh/id_ed25519` or
//! `/etc/anything` as a plugin is a remote-code-execution primitive, not a
//! configuration mistake. See `docs/security.md`.
//!
//! The grammar below is a closed one, checked in a single byte scan with no
//! regex, and it is the same on every platform — in particular `\` is a
//! separator everywhere, never only on Windows, which is the shape of both the
//! pnpm tarball traversal and the Vite-era dev-server bypasses.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use thiserror::Error;
use uf_config::{PipelineMode, PluginEntry, UniflowedConfig};

use crate::builtin::BuiltinSet;
use crate::container::{ContainerError, PluginContainer};
use crate::descriptor::{PluginDescriptor, PluginSource};
use crate::hook::HookSet;

/// Longest plugin name the resolver will look at.
///
/// Config is attacker-controlled text; every parse in uf has an explicit bound
/// above it. 256 bytes is longer than any real package specifier or project
/// path and short enough that a hostile config cannot make the resolver
/// allocate.
pub const MAX_PLUGIN_NAME_BYTES: usize = 256;

/// The prefix uf reserves for its own plugins.
///
/// A project plugin may not use it, so a config can neither shadow a built-in
/// nor make `uf inspect --json` claim uf ships something it does not.
pub const BUILTIN_PREFIX: &str = "uf:";

/// Why a declared plugin name is not usable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PluginPathError {
    /// The name is empty.
    #[error("plugin name is empty")]
    Empty,
    /// The name is longer than [`MAX_PLUGIN_NAME_BYTES`].
    #[error("plugin name is {bytes} bytes, over the {limit} byte ceiling")]
    TooLong {
        /// Length of the declared name.
        bytes: usize,
        /// The ceiling, always [`MAX_PLUGIN_NAME_BYTES`].
        limit: usize,
    },
    /// The name holds a control byte, which no resolver should ever see.
    #[error("plugin name {name:?} holds a control byte at offset {offset}")]
    ControlByte {
        /// The declared name.
        name: CompactString,
        /// Byte offset of the first control byte.
        offset: usize,
    },
    /// The name uses the prefix uf reserves for its own plugins.
    #[error("plugin name {name:?} uses the reserved {prefix:?} prefix")]
    ReservedPrefix {
        /// The declared name.
        name: CompactString,
        /// The reserved prefix, always [`BUILTIN_PREFIX`].
        prefix: &'static str,
    },
    /// The name is a URL, or a Windows drive-qualified path.
    #[error("plugin name {name:?} names a {scheme:?} URL or drive, not a package or project file")]
    UrlScheme {
        /// The declared name.
        name: CompactString,
        /// The scheme or drive letter found before the colon.
        scheme: CompactString,
    },
    /// The name is relative to the user's home directory.
    #[error("plugin name {name:?} is home-relative")]
    HomeRelative {
        /// The declared name.
        name: CompactString,
    },
    /// The name uses a backslash, which uf treats as a separator everywhere.
    #[error("plugin name {name:?} holds a path separator `\\` at offset {offset}")]
    BackslashSeparator {
        /// The declared name.
        name: CompactString,
        /// Byte offset of the first backslash.
        offset: usize,
    },
    /// The name is an absolute path.
    #[error("plugin name {name:?} is an absolute path")]
    Absolute {
        /// The declared name.
        name: CompactString,
    },
    /// The name has an empty path segment, as in `a//b` or a trailing slash.
    #[error("plugin name {name:?} has an empty path segment at position {position}")]
    EmptySegment {
        /// The declared name.
        name: CompactString,
        /// Which `/`-separated segment was empty, counting from zero.
        position: usize,
    },
    /// The name walks upwards out of the project.
    #[error("plugin name {name:?} has a `..` segment at position {position}")]
    ParentSegment {
        /// The declared name.
        name: CompactString,
        /// Which `/`-separated segment was `..`, counting from zero.
        position: usize,
    },
    /// The name has a `.` segment somewhere other than the front.
    #[error("plugin name {name:?} has a `.` segment at position {position}")]
    CurrentSegment {
        /// The declared name.
        name: CompactString,
        /// Which `/`-separated segment was `.`, counting from zero.
        position: usize,
    },
    /// The name resolved to a path outside the project root.
    ///
    /// Reached when a symlink inside the project points outwards; the lexical
    /// grammar cannot see that, so containment is re-checked after resolution.
    #[error("plugin {name:?} resolves to {resolved}, outside the project root {root}")]
    OutsideRoot {
        /// The declared name.
        name: CompactString,
        /// Where it actually pointed.
        resolved: Utf8PathBuf,
        /// The project root it had to stay inside.
        root: Utf8PathBuf,
    },
    /// The path exists but could not be resolved to a real path.
    ///
    /// Containment cannot be proven, so the entry is refused rather than
    /// admitted on the strength of the lexical check alone.
    #[error("plugin {name:?} at {path} could not be resolved to a real path")]
    Unresolvable {
        /// The declared name.
        name: CompactString,
        /// The path that failed to resolve.
        path: Utf8PathBuf,
    },
}

/// Why a project's `plugins: [...]` could not be turned into a pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    /// One entry is not a usable plugin name.
    #[error("plugins[{index}] is not a usable plugin name")]
    Entry {
        /// Index into `plugins: [...]`, so the error points at a line.
        index: usize,
        /// What was wrong with it.
        #[source]
        source: PluginPathError,
    },
    /// The resolved plugins do not form a usable pipeline.
    #[error(transparent)]
    Container(#[from] ContainerError),
}

/// Classify a declared plugin name, rejecting anything that reaches outside the
/// project.
///
/// A leading `./` is what makes a name a project file; everything else is a
/// package specifier, exactly as a module resolver would read it. That means
/// `plugins/x.js` is looked up as a package and never joined onto a path, so
/// there is only one way for a config to name a file on disk and only one place
/// to guard.
pub fn classify_plugin_name(name: &str, root: &Utf8Path) -> Result<PluginSource, PluginPathError> {
    if name.is_empty() {
        return Err(PluginPathError::Empty);
    }
    if name.len() > MAX_PLUGIN_NAME_BYTES {
        return Err(PluginPathError::TooLong {
            bytes: name.len(),
            limit: MAX_PLUGIN_NAME_BYTES,
        });
    }

    let bytes = name.as_bytes();
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte < 0x20 || byte == 0x7f {
            return Err(PluginPathError::ControlByte {
                name: CompactString::new(name),
                offset,
            });
        }
        if byte == b'\\' {
            return Err(PluginPathError::BackslashSeparator {
                name: CompactString::new(name),
                offset,
            });
        }
    }

    if name.starts_with(BUILTIN_PREFIX) {
        return Err(PluginPathError::ReservedPrefix {
            name: CompactString::new(name),
            prefix: BUILTIN_PREFIX,
        });
    }
    if let Some(scheme) = leading_scheme(name) {
        return Err(PluginPathError::UrlScheme {
            name: CompactString::new(name),
            scheme: CompactString::new(scheme),
        });
    }
    if bytes[0] == b'~' {
        return Err(PluginPathError::HomeRelative {
            name: CompactString::new(name),
        });
    }
    if bytes[0] == b'/' {
        return Err(PluginPathError::Absolute {
            name: CompactString::new(name),
        });
    }

    for (position, segment) in name.split('/').enumerate() {
        match segment {
            "" => {
                return Err(PluginPathError::EmptySegment {
                    name: CompactString::new(name),
                    position,
                });
            }
            ".." => {
                return Err(PluginPathError::ParentSegment {
                    name: CompactString::new(name),
                    position,
                });
            }
            "." if position > 0 => {
                return Err(PluginPathError::CurrentSegment {
                    name: CompactString::new(name),
                    position,
                });
            }
            _ => {}
        }
    }

    if name.starts_with("./") {
        Ok(PluginSource::ProjectFile {
            path: resolve_inside_root(name, root)?,
        })
    } else {
        Ok(PluginSource::Package {
            specifier: CompactString::new(name),
        })
    }
}

/// Join a checked relative name onto the root and prove the result stays inside.
///
/// The segment walk above already refuses `..`, so the containment test here is
/// belt and braces for the lexical case. It earns its keep on the case the
/// grammar cannot see: a symlink inside the project pointing out of it. When
/// the target exists, both sides are canonicalized and compared as paths, never
/// as string prefixes, so `/project-evil` is not treated as living under
/// `/project`.
fn resolve_inside_root(name: &str, root: &Utf8Path) -> Result<Utf8PathBuf, PluginPathError> {
    let mut resolved = root.to_path_buf();
    for segment in name.split('/') {
        match segment {
            "." => {}
            ".." => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }

    if !resolved.starts_with(root) {
        return Err(PluginPathError::OutsideRoot {
            name: CompactString::new(name),
            resolved,
            root: root.to_path_buf(),
        });
    }

    if !resolved.exists() {
        return Ok(resolved);
    }
    let Ok(canonical_root) = root.canonicalize_utf8() else {
        return Ok(resolved);
    };
    let canonical = resolved
        .canonicalize_utf8()
        .map_err(|_| PluginPathError::Unresolvable {
            name: CompactString::new(name),
            path: resolved.clone(),
        })?;
    if !canonical.starts_with(&canonical_root) {
        return Err(PluginPathError::OutsideRoot {
            name: CompactString::new(name),
            resolved: canonical,
            root: canonical_root,
        });
    }

    Ok(resolved)
}

/// Descriptors for the plugins a project declares, in declaration order.
///
/// The hook set starts empty: which hooks a project plugin implements is only
/// known once its module is read, and the resolver reads nothing. A loader
/// fills it in with [`PluginDescriptor::with_hooks`].
pub fn resolve_project_plugins(
    config: &UniflowedConfig,
    root: &Utf8Path,
) -> Result<Vec<PluginDescriptor>, ResolveError> {
    config
        .plugins
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            resolve_entry(entry, root).map_err(|source| ResolveError::Entry { index, source })
        })
        .collect()
}

/// Turn one declared entry into a descriptor.
pub fn resolve_entry(
    entry: &PluginEntry,
    root: &Utf8Path,
) -> Result<PluginDescriptor, PluginPathError> {
    let name = entry.name();
    let source = classify_plugin_name(name, root)?;
    Ok(PluginDescriptor::project(
        name,
        source,
        entry.order(),
        entry.apply(),
        HookSet::EMPTY,
    ))
}

/// The whole pipeline for a project: uf's built-ins plus whatever the config
/// declares, resolved into one container.
///
/// Built-ins come first so a project plugin with the same band still runs after
/// them, and so a duplicate name collides against the built-in rather than
/// quietly replacing it.
pub fn resolve_pipeline(
    config: &UniflowedConfig,
    root: &Utf8Path,
    mode: PipelineMode,
) -> Result<PluginContainer, ResolveError> {
    let mut descriptors = BuiltinSet::from_config(config).descriptors();
    descriptors.extend(resolve_project_plugins(config, root)?);
    Ok(PluginContainer::from_descriptors(mode, descriptors)?)
}

/// The `scheme` of `scheme:rest`, when the prefix is a valid URL scheme.
///
/// A single Windows drive letter matches too, which is deliberate: `C:/x` is an
/// absolute path wearing a scheme's clothes, and both are refused here.
fn leading_scheme(name: &str) -> Option<&str> {
    let colon = name.find(':')?;
    let scheme = &name[..colon];
    let mut bytes = scheme.bytes();
    let first = bytes.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    bytes
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
        .then_some(scheme)
}
