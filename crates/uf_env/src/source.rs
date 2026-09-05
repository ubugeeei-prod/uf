//! Where a tool's archive comes from, and what it should hash to.
//!
//! # Why the checksum is not optional
//!
//! An environment is a supply chain. A tarball fetched over TLS from the
//! right host is still a tarball whose contents nobody checked, and the
//! failure it protects against — a mirror serving something else — is silent
//! by construction. Every source here therefore carries a way to learn the
//! expected digest *from the publisher*, and [`crate::archive`] refuses to
//! unpack anything that does not match it.
//!
//! Node publishes `SHASUMS256.txt` beside each release; the npm registry
//! publishes an `integrity` field per version. Both are fetched separately
//! from the archive, which is the point.
//!
//! # Why the base is overridable
//!
//! `UF_TOOL_BASE` replaces the host, so the tests fetch from a directory of
//! fixtures over `file://` and never touch the network. It is the same shape
//! as `UF_RELEASE_BASE` in `infra/cloudflare/setup-assets/install.sh`, and
//! for the same reason: an acquisition path that can only be exercised
//! against the real internet is an acquisition path that is not exercised.

use crate::tool::{Arch, Os, Pin, Tool};

/// How to get one archive and check it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Where the archive is.
    pub archive: String,
    /// How its digest is learnt.
    pub checksum: Checksum,
    /// The archive's container format.
    pub format: Format,
    /// How many leading path components to drop when unpacking.
    ///
    /// Node's tarball is `node-v24.14.0-darwin-arm64/bin/node`; the store
    /// entry should be `bin/node`. An npm registry tarball wraps everything
    /// in `package/`. Both are one.
    pub strip: usize,
}

/// What kind of archive it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// gzipped tar, unpacked with `tar`.
    TarGz,
    /// zip, unpacked with `unzip`.
    Zip,
}

/// How the expected digest is learnt.
///
/// Two publishers, two algorithms, and they are not interchangeable: Node
/// lists SHA-256 as hex, npm publishes `sha512-<base64>`. Carrying the
/// algorithm with the source is what stops a check that compares a SHA-256
/// to a SHA-512 and calls every download corrupt — which is exactly what a
/// single `sha256` field did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checksum {
    /// A `SHASUMS256.txt`-style file listing `<digest>  <file>` lines, and
    /// the file name to find in it.
    Sha256File {
        /// Where the list is.
        url: String,
        /// The archive's name, as it appears in the list.
        file: String,
    },
    /// The npm registry's version manifest, which carries `dist.integrity`
    /// as `sha512-<base64>`.
    NpmIntegrity {
        /// Where the manifest is.
        url: String,
    },
}

/// A digest and the algorithm that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Digest {
    /// SHA-256, lower-case hex, as `SHASUMS256.txt` lists it.
    Sha256Hex(String),
    /// SHA-512, standard base64, as npm's `integrity` carries it — without
    /// the `sha512-` prefix.
    Sha512Base64(String),
}

impl std::fmt::Display for Digest {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sha256Hex(value) => write!(out, "sha256:{value}"),
            Self::Sha512Base64(value) => write!(out, "sha512-{value}"),
        }
    }
}

impl Source {
    /// Where `pin` comes from.
    ///
    /// [`None`] for a combination uf has no publisher for — there is no
    /// official Deno build for every platform pair, and inventing a URL that
    /// 404s is a worse error than saying so.
    #[must_use]
    pub fn for_pin(pin: &Pin) -> Option<Self> {
        match pin.tool {
            Tool::Node => Some(Self::node(pin)),
            Tool::Bun => Self::bun(pin),
            Tool::Deno => Self::deno(pin),
            Tool::Npm | Tool::Pnpm | Tool::Yarn => Some(Self::npm_package(pin)),
        }
    }

    /// `https://nodejs.org/dist/v24.14.0/node-v24.14.0-darwin-arm64.tar.gz`.
    fn node(pin: &Pin) -> Self {
        let base = base("https://nodejs.org/dist");
        let version = &pin.version;
        let name = format!(
            "node-v{version}-{}-{}",
            match pin.platform.os {
                Os::Darwin => "darwin",
                Os::Linux => "linux",
            },
            match pin.platform.arch {
                Arch::Arm64 => "arm64",
                Arch::X64 => "x64",
            }
        );
        let file = format!("{name}.tar.gz");
        Self {
            archive: format!("{base}/v{version}/{file}"),
            checksum: Checksum::Sha256File {
                url: format!("{base}/v{version}/SHASUMS256.txt"),
                file,
            },
            format: Format::TarGz,
            // `node-v24.14.0-darwin-arm64/`.
            strip: 1,
        }
    }

    /// Bun's GitHub release asset.
    fn bun(pin: &Pin) -> Option<Self> {
        let base = base("https://github.com/oven-sh/bun/releases/download");
        let version = &pin.version;
        let target = match (pin.platform.os, pin.platform.arch) {
            (Os::Darwin, Arch::Arm64) => "darwin-aarch64",
            (Os::Darwin, Arch::X64) => "darwin-x64",
            (Os::Linux, Arch::Arm64) => "linux-aarch64",
            (Os::Linux, Arch::X64) => "linux-x64",
        };
        let file = format!("bun-{target}.zip");
        Some(Self {
            archive: format!("{base}/bun-v{version}/{file}"),
            checksum: Checksum::Sha256File {
                url: format!("{base}/bun-v{version}/SHASUMS256.txt"),
                file,
            },
            format: Format::Zip,
            // `bun-darwin-aarch64/bun`.
            strip: 1,
        })
    }

    /// Deno's GitHub release asset, named by Rust target triple.
    fn deno(pin: &Pin) -> Option<Self> {
        let base = base("https://github.com/denoland/deno/releases/download");
        let version = &pin.version;
        let triple = match (pin.platform.os, pin.platform.arch) {
            (Os::Darwin, Arch::Arm64) => "aarch64-apple-darwin",
            (Os::Darwin, Arch::X64) => "x86_64-apple-darwin",
            (Os::Linux, Arch::Arm64) => "aarch64-unknown-linux-gnu",
            (Os::Linux, Arch::X64) => "x86_64-unknown-linux-gnu",
        };
        Some(Self {
            archive: format!("{base}/v{version}/deno-{triple}.zip"),
            // Deno publishes no per-asset digest file beside the release, so
            // there is nothing to check against and this is refused rather
            // than installed unverified. See `for_pin`'s doc.
            checksum: Checksum::Sha256File {
                url: format!("{base}/v{version}/SHASUMS256.txt"),
                file: format!("deno-{triple}.zip"),
            },
            format: Format::Zip,
            // The zip holds `deno` at its root.
            strip: 0,
        })
    }

    /// An npm registry tarball: npm, pnpm and Yarn are all published as npm
    /// packages, which is also how `corepack` gets them.
    fn npm_package(pin: &Pin) -> Self {
        let base = base("https://registry.npmjs.org");
        let name = pin.tool.name();
        let version = &pin.version;
        Self {
            archive: format!("{base}/{name}/-/{name}-{version}.tgz"),
            checksum: Checksum::NpmIntegrity {
                url: format!("{base}/{name}/{version}"),
            },
            format: Format::TarGz,
            // Every npm tarball wraps its contents in `package/`.
            strip: 1,
        }
    }
}

/// `$UF_TOOL_BASE` with the publisher's path appended, or the publisher.
///
/// The override replaces the *whole* base, so a fixture directory serves
/// `<base>/v24.14.0/node-v24.14.0-darwin-arm64.tar.gz` and nothing has to
/// mirror nodejs.org's hostname.
fn base(default: &str) -> String {
    match std::env::var("UF_TOOL_BASE") {
        Ok(value) if !value.trim().is_empty() => value.trim_end_matches('/').to_owned(),
        _ => default.to_owned(),
    }
}
