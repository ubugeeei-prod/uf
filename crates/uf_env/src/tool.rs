//! What a tool is: a name, a version, and the platform it was built for.
//!
//! A store entry is identified by those three and nothing else. Two
//! repositories that pin `node@24.14.0` on the same machine name the same
//! directory, which is what makes the store shared rather than per-project;
//! two machines with different architectures never do, which is what stops a
//! Linux build being linked into a macOS checkout.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A JavaScript runtime or package manager uf can install.
///
/// Closed, because each variant carries where its archive comes from and how
/// the binary inside it is named, and an open set would mean a registry of
/// downloaders configured from somewhere. A new tool is a variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    /// Node.js.
    Node,
    /// Bun, which is a runtime and a package manager in one binary.
    Bun,
    /// Deno.
    Deno,
    /// npm, published as an npm package.
    Npm,
    /// pnpm, published as an npm package.
    Pnpm,
    /// Yarn, published as an npm package.
    Yarn,
}

impl Tool {
    /// Every tool, in the order they are listed to a reader.
    ///
    /// Runtimes first, then package managers: it is the order a project
    /// acquires them in, and the order the question "what am I running this
    /// on" is asked in.
    pub const ALL: [Self; 6] = [
        Self::Node,
        Self::Bun,
        Self::Deno,
        Self::Npm,
        Self::Pnpm,
        Self::Yarn,
    ];

    /// The name a person writes in `uf.config.js`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Bun => "bun",
            Self::Deno => "deno",
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
        }
    }

    /// Read a name written in `uf.config.js`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.name() == name)
    }

    /// Whether this tool runs JavaScript, as opposed to installing it.
    ///
    /// `bun` is both, and is a runtime here: a project that pins it has
    /// something to run its code on, which is the question this answers.
    #[must_use]
    pub const fn is_runtime(self) -> bool {
        matches!(self, Self::Node | Self::Bun | Self::Deno)
    }

    /// The executables an installed entry provides, in `bin/`.
    ///
    /// Node ships `npx` beside `node`, and npm's package ships `npx` too —
    /// which is why a project that pins both gets the npm one: it is linked
    /// second, and the later link wins. That is deliberate and it is the same
    /// answer every other version manager gives.
    #[must_use]
    pub const fn executables(self) -> &'static [&'static str] {
        match self {
            Self::Node => &["node", "npx", "corepack"],
            Self::Bun => &["bun", "bunx"],
            Self::Deno => &["deno"],
            Self::Npm => &["npm", "npx"],
            Self::Pnpm => &["pnpm", "pnpx"],
            Self::Yarn => &["yarn"],
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(self.name())
    }
}

/// The operating system and architecture an entry was built for.
///
/// Its own type rather than a string, so a store path cannot be assembled
/// from a `format!` that forgot one half of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Platform {
    /// `darwin`, `linux`.
    pub os: Os,
    /// `arm64`, `x64`.
    pub arch: Arch,
}

/// The operating systems uf installs tools for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Os {
    /// macOS.
    Darwin,
    /// Linux, glibc.
    Linux,
}

/// The architectures uf installs tools for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Arch {
    /// 64-bit ARM.
    Arm64,
    /// 64-bit x86.
    X64,
}

impl Platform {
    /// The platform this binary is running on.
    ///
    /// [`None`] where uf runs but does not know how to fetch tools —
    /// Windows, 32-bit, anything else. A project on such a machine gets a
    /// clear refusal rather than a download that cannot execute.
    #[must_use]
    pub fn current() -> Option<Self> {
        let os = match std::env::consts::OS {
            "macos" => Os::Darwin,
            "linux" => Os::Linux,
            _ => return None,
        };
        let arch = match std::env::consts::ARCH {
            "aarch64" => Arch::Arm64,
            "x86_64" => Arch::X64,
            _ => return None,
        };
        Some(Self { os, arch })
    }

    /// `darwin-arm64`, the way it appears in a store path.
    #[must_use]
    pub fn slug(self) -> String {
        format!("{}-{}", self.os.as_str(), self.arch.as_str())
    }
}

impl Os {
    /// The name uf uses in a store path.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Darwin => "darwin",
            Self::Linux => "linux",
        }
    }
}

impl Arch {
    /// The name uf uses in a store path.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::X64 => "x64",
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(&self.slug())
    }
}

/// One tool at one version for one platform: everything that names a store
/// entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Pin {
    /// Which tool.
    pub tool: Tool,
    /// An exact version. Ranges are not pins: a lockfile that can resolve
    /// differently tomorrow is not an environment.
    pub version: String,
    /// The platform the entry was built for.
    pub platform: Platform,
}

impl Pin {
    /// The directory name this pin has in the store.
    ///
    /// `node-24.14.0-darwin-arm64`. Readable on purpose: somebody looking at
    /// a full disk should be able to tell what is taking the space without a
    /// tool to decode it.
    #[must_use]
    pub fn slug(&self) -> String {
        format!("{}-{}-{}", self.tool.name(), self.version, self.platform)
    }
}

impl fmt::Display for Pin {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}@{}", self.tool.name(), self.version)
    }
}
