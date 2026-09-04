//! The bundle report: what a build emitted, and what each route costs.

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::size::{AssetSize, MeasureError, measure};

/// Deepest directory nesting `uf` will walk while collecting assets.
///
/// A build directory can contain a symlink loop or a pathological tree, so the
/// walk is bounded and iterative rather than trusting the filesystem.
pub const MAX_ASSET_DEPTH: usize = 32;

/// Most assets `uf` will measure in one report.
pub const MAX_ASSETS: usize = 100_000;

/// What kind of thing an asset is, for grouping in the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetKind {
    /// A JavaScript chunk.
    JavaScript,
    /// A stylesheet.
    Stylesheet,
    /// A source map. Never downloaded by users, so it is measured but excluded
    /// from route weight.
    SourceMap,
    /// An HTML document.
    Html,
    /// A build manifest or other JSON emitted alongside the app.
    Json,
    /// Anything else: fonts, images, wasm.
    Other,
}

impl AssetKind {
    /// Classify by file extension.
    #[must_use]
    pub fn from_path(path: &Utf8Path) -> Self {
        // `.js.map` must win over `.map`-less `.js`, so check the full suffix
        // before falling back to the extension.
        let name = path.file_name().unwrap_or_default();
        if name.ends_with(".map") {
            return Self::SourceMap;
        }

        match path.extension() {
            Some("js" | "mjs" | "cjs") => Self::JavaScript,
            Some("css") => Self::Stylesheet,
            Some("html" | "htm") => Self::Html,
            Some("json") => Self::Json,
            _ => Self::Other,
        }
    }

    /// Whether this asset counts toward what a user downloads for a route.
    #[must_use]
    pub const fn is_downloaded(self) -> bool {
        !matches!(self, Self::SourceMap)
    }

    /// Stable identifier used in reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::Stylesheet => "stylesheet",
            Self::SourceMap => "source-map",
            Self::Html => "html",
            Self::Json => "json",
            Self::Other => "other",
        }
    }
}

/// One emitted asset and its measured sizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEntry {
    /// Path relative to the output directory, always using `/`.
    pub path: CompactString,
    /// What kind of asset it is.
    pub kind: AssetKind,
    /// Raw and compressed sizes.
    pub size: AssetSize,
}

/// What one route costs a visitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteEntry {
    /// Route path as the router reports it.
    pub path: CompactString,
    /// JavaScript needed before the route can render.
    pub initial_js: AssetSize,
    /// JavaScript the route loads later.
    pub lazy_js: AssetSize,
    /// Everything the route pulls in.
    pub total: AssetSize,
    /// Assets attributed to the route, sorted.
    pub assets: Vec<CompactString>,
}

/// A whole build's size picture.
///
/// Every collection is sorted, so two builds of the same input serialize
/// byte-identically and a diff of the report is a real signal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleReport {
    /// Emitted assets, sorted by path.
    pub assets: Vec<AssetEntry>,
    /// Per-route weight, sorted by route path.
    pub routes: Vec<RouteEntry>,
    /// Every downloaded asset added together.
    pub total: AssetSize,
}

impl BundleReport {
    /// Assets sorted largest first under `metric`, for a human-readable summary.
    #[must_use]
    pub fn largest_assets(&self, metric: crate::size::BudgetMetric) -> Vec<&AssetEntry> {
        let mut assets = self.assets.iter().collect::<Vec<_>>();
        assets.sort_by(|a, b| {
            b.size
                .get(metric)
                .cmp(&a.size.get(metric))
                .then_with(|| a.path.cmp(&b.path))
        });
        assets
    }

    /// Total size per asset kind, for a quick "where is the weight" answer.
    #[must_use]
    pub fn total_by_kind(&self) -> BTreeMap<AssetKind, AssetSize> {
        let mut totals = BTreeMap::new();
        for asset in &self.assets {
            let entry = totals.entry(asset.kind).or_insert_with(AssetSize::zero);
            *entry = entry.saturating_add(asset.size);
        }
        totals
    }
}

/// Failures while building a report.
#[derive(Debug, Error)]
pub enum ReportError {
    /// A directory or file could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A path in the output directory was not UTF-8.
    #[error("output path is not UTF-8: {path}")]
    NonUtf8Path {
        /// Lossy rendering of the offending path.
        path: String,
    },
    /// The build emitted more assets than `uf` will measure.
    #[error("output directory holds more than {MAX_ASSETS} assets")]
    TooManyAssets,
    /// An asset could not be measured.
    #[error("failed to measure {path}: {source}")]
    Measure {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: MeasureError,
    },
    /// The report could not be serialized.
    #[error("failed to serialize the bundle report: {0}")]
    Serialize(#[source] serde_json::Error),
    /// The report could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// Which files a report should skip.
#[derive(Debug, Clone)]
pub struct ReportOptions {
    /// File names, relative to the output directory, that are build metadata
    /// rather than shipped assets.
    pub excluded: Vec<CompactString>,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            excluded: vec![
                CompactString::const_new("uf-build-manifest.json"),
                CompactString::const_new("uf-bundle-report.json"),
                CompactString::const_new("uf-rsc-manifest.json"),
            ],
        }
    }
}

/// Measure every asset under `out_dir`.
///
/// Walks iteratively with an explicit stack and does not follow symlinks: a
/// build directory is written by dependencies, and a symlink out of the tree
/// would otherwise let a hostile package have `uf` read arbitrary files.
///
/// The walk is serial (it is I/O-bound on directory metadata) but measurement is
/// not: brotli at quality 11 costs milliseconds per asset, so the files fan out
/// across rayon. A build with hundreds of chunks would otherwise spend seconds
/// compressing them one at a time.
pub fn collect_assets(
    out_dir: &Utf8Path,
    options: &ReportOptions,
) -> Result<Vec<AssetEntry>, ReportError> {
    let mut files: Vec<(Utf8PathBuf, CompactString)> = Vec::new();
    let mut stack = vec![(out_dir.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_ASSET_DEPTH {
            continue;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(ReportError::Read { path: dir, source }),
        };

        for entry in entries {
            let entry = entry.map_err(|source| ReportError::Read {
                path: dir.clone(),
                source,
            })?;
            let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| {
                ReportError::NonUtf8Path {
                    path: path.display().to_string(),
                }
            })?;
            // `file_type` does not traverse symlinks, so a link is neither
            // descended into nor measured.
            let file_type = entry.file_type().map_err(|source| ReportError::Read {
                path: path.clone(),
                source,
            })?;

            if file_type.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let relative = path.strip_prefix(out_dir).unwrap_or(&path);
            let relative = relative.as_str().to_compact_string();
            if options.excluded.contains(&relative) {
                continue;
            }
            if files.len() >= MAX_ASSETS {
                return Err(ReportError::TooManyAssets);
            }

            files.push((path, relative));
        }
    }

    let mut assets = uf_infra::parallel::map(&files, |(path, relative)| {
        let contents = fs::read(path).map_err(|source| ReportError::Read {
            path: path.clone(),
            source,
        })?;
        let size = measure(&contents).map_err(|source| ReportError::Measure {
            path: path.clone(),
            source,
        })?;

        Ok(AssetEntry {
            kind: AssetKind::from_path(path),
            path: relative.clone(),
            size,
        })
    })?;

    assets.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(assets)
}

/// Build a report from measured assets and a route-to-asset attribution.
///
/// `entry_assets` names the assets a route needs before first render;
/// `route_assets` names everything it eventually loads. Both are looked up in
/// `assets`; an unknown name is ignored rather than guessed at, so a partially
/// wired build under-reports instead of inventing numbers.
#[must_use]
pub fn build_report(
    assets: Vec<AssetEntry>,
    routes: &[(CompactString, Vec<CompactString>, Vec<CompactString>)],
) -> BundleReport {
    let by_path = assets
        .iter()
        .map(|asset| (asset.path.as_str(), asset))
        .collect::<BTreeMap<_, _>>();

    let mut route_entries = routes
        .iter()
        .map(|(path, entry_assets, lazy_assets)| {
            let initial_js = sum_javascript(&by_path, entry_assets);
            let lazy_js = sum_javascript(&by_path, lazy_assets);
            let mut names = entry_assets
                .iter()
                .chain(lazy_assets)
                .cloned()
                .collect::<Vec<_>>();
            names.sort();
            names.dedup();
            let total = names
                .iter()
                .filter_map(|name| by_path.get(name.as_str()))
                .filter(|asset| asset.kind.is_downloaded())
                .fold(AssetSize::zero(), |total, asset| {
                    total.saturating_add(asset.size)
                });

            RouteEntry {
                path: path.clone(),
                initial_js,
                lazy_js,
                total,
                assets: names,
            }
        })
        .collect::<Vec<_>>();
    route_entries.sort_by(|a, b| a.path.cmp(&b.path));

    let total = assets
        .iter()
        .filter(|asset| asset.kind.is_downloaded())
        .fold(AssetSize::zero(), |total, asset| {
            total.saturating_add(asset.size)
        });

    BundleReport {
        assets,
        routes: route_entries,
        total,
    }
}

fn sum_javascript(by_path: &BTreeMap<&str, &AssetEntry>, names: &[CompactString]) -> AssetSize {
    names
        .iter()
        .filter_map(|name| by_path.get(name.as_str()))
        .filter(|asset| asset.kind == AssetKind::JavaScript)
        .fold(AssetSize::zero(), |total, asset| {
            total.saturating_add(asset.size)
        })
}

/// Write the report to `<out_dir>/uf-bundle-report.json`.
pub fn write_report(out_dir: &Utf8Path, report: &BundleReport) -> Result<Utf8PathBuf, ReportError> {
    let path = out_dir.join("uf-bundle-report.json");
    let mut contents = serde_json::to_string_pretty(&BundleReportFile {
        version: 1,
        gzip_level: crate::size::GZIP_LEVEL,
        brotli_quality: crate::size::BROTLI_QUALITY,
        report,
    })
    .map_err(ReportError::Serialize)?;
    contents.push('\n');

    fs::write(&path, contents).map_err(|source| ReportError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// On-disk shape of `uf-bundle-report.json`.
///
/// The compression settings travel with the numbers so a report from one
/// version is never silently compared against a differently-compressed one.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleReportFile<'a> {
    version: u8,
    gzip_level: u32,
    brotli_quality: u32,
    #[serde(flatten)]
    report: &'a BundleReport,
}

#[cfg(test)]
mod tests;
