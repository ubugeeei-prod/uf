//! The bundle report: what a build emitted, and what each route costs.

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use rayon::prelude::*;
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

    let mut assets = files
        .into_par_iter()
        .map(|(path, relative)| {
            let contents = fs::read(&path).map_err(|source| ReportError::Read {
                path: path.clone(),
                source,
            })?;
            let size = measure(&contents).map_err(|source| ReportError::Measure {
                path: path.clone(),
                source,
            })?;

            Ok(AssetEntry {
                kind: AssetKind::from_path(&path),
                path: relative,
                size,
            })
        })
        .collect::<Result<Vec<_>, ReportError>>()?;

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
mod tests {
    use super::*;
    use crate::size::{BudgetMetric, ByteSize};

    fn write(root: &Utf8Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("has parent")).expect("creates dirs");
        fs::write(path, contents).expect("writes");
    }

    fn temp_root() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8 temp dir");
        (dir, root)
    }

    #[test]
    fn classifies_assets_by_extension() {
        assert_eq!(
            AssetKind::from_path(Utf8Path::new("a/app.js")),
            AssetKind::JavaScript
        );
        assert_eq!(
            AssetKind::from_path(Utf8Path::new("a/app.mjs")),
            AssetKind::JavaScript
        );
        assert_eq!(
            AssetKind::from_path(Utf8Path::new("a/app.css")),
            AssetKind::Stylesheet
        );
        assert_eq!(
            AssetKind::from_path(Utf8Path::new("a/index.html")),
            AssetKind::Html
        );
        assert_eq!(
            AssetKind::from_path(Utf8Path::new("a/data.json")),
            AssetKind::Json
        );
        assert_eq!(
            AssetKind::from_path(Utf8Path::new("a/font.woff2")),
            AssetKind::Other
        );
    }

    #[test]
    fn source_maps_win_over_the_extension_they_end_in() {
        assert_eq!(
            AssetKind::from_path(Utf8Path::new("a/app.js.map")),
            AssetKind::SourceMap
        );
        assert_eq!(
            AssetKind::from_path(Utf8Path::new("a/app.css.map")),
            AssetKind::SourceMap
        );
        assert!(!AssetKind::SourceMap.is_downloaded());
        assert!(AssetKind::JavaScript.is_downloaded());
    }

    #[test]
    fn collects_and_sorts_assets_from_the_output_directory() {
        let (_dir, root) = temp_root();
        write(&root, "assets/b.js", "const b = 2;\n");
        write(&root, "assets/a.js", "const a = 1;\n");
        write(&root, "index.html", "<!doctype html>\n");

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");

        let paths = assets.iter().map(|a| a.path.as_str()).collect::<Vec<_>>();
        assert_eq!(paths, ["assets/a.js", "assets/b.js", "index.html"]);
    }

    #[test]
    fn skips_build_metadata_by_default() {
        let (_dir, root) = temp_root();
        write(&root, "uf-build-manifest.json", "{}\n");
        write(&root, "uf-rsc-manifest.json", "{}\n");
        write(&root, "uf-bundle-report.json", "{}\n");
        write(&root, "app.js", "const a = 1;\n");

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].path, "app.js");
    }

    #[test]
    fn a_missing_output_directory_reports_no_assets() {
        let (_dir, root) = temp_root();

        let assets =
            collect_assets(&root.join("does-not-exist"), &ReportOptions::default()).expect("ok");

        assert!(assets.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_neither_measured_nor_followed() {
        let (_dir, root) = temp_root();
        write(&root, "app.js", "const a = 1;\n");
        std::os::unix::fs::symlink("/etc/passwd", root.join("leak.js")).expect("symlink");
        std::os::unix::fs::symlink(root.as_std_path(), root.join("loop")).expect("symlink");

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");

        let paths = assets.iter().map(|a| a.path.as_str()).collect::<Vec<_>>();
        assert_eq!(paths, ["app.js"]);
    }

    #[test]
    fn deep_trees_terminate_instead_of_recursing() {
        let (_dir, root) = temp_root();
        let mut nested = String::new();
        for _ in 0..(MAX_ASSET_DEPTH + 8) {
            nested.push_str("d/");
        }
        write(&root, &format!("{nested}deep.js"), "const a = 1;\n");
        write(&root, "shallow.js", "const b = 2;\n");

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");

        assert!(assets.iter().any(|a| a.path == "shallow.js"));
        assert!(assets.iter().all(|a| a.path != format!("{nested}deep.js")));
    }

    #[test]
    fn route_weight_splits_initial_from_lazy_javascript() {
        let (_dir, root) = temp_root();
        write(&root, "entry.js", &"const a = 1;\n".repeat(20));
        write(&root, "lazy.js", &"const b = 2;\n".repeat(40));
        write(&root, "app.css", &".a { color: red }\n".repeat(10));

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
        let report = build_report(
            assets,
            &[(
                CompactString::const_new("/"),
                vec![
                    CompactString::const_new("entry.js"),
                    CompactString::const_new("app.css"),
                ],
                vec![CompactString::const_new("lazy.js")],
            )],
        );

        let route = &report.routes[0];
        assert!(route.initial_js.raw.bytes() > 0);
        assert!(route.lazy_js.raw.bytes() > route.initial_js.raw.bytes());
        // Stylesheets count toward the route total but never toward initial JS.
        assert!(route.total.raw.bytes() > route.initial_js.raw.bytes() + route.lazy_js.raw.bytes());
        assert_eq!(route.assets.len(), 3);
    }

    #[test]
    fn source_maps_are_measured_but_excluded_from_totals() {
        let (_dir, root) = temp_root();
        write(&root, "app.js", "const a = 1;\n");
        write(&root, "app.js.map", &"x".repeat(4096));

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
        let report = build_report(assets, &[]);

        assert_eq!(report.assets.len(), 2);
        let app_js = report
            .assets
            .iter()
            .find(|a| a.path == "app.js")
            .expect("app.js");
        assert_eq!(report.total, app_js.size);
    }

    #[test]
    fn unknown_route_assets_are_ignored_rather_than_guessed() {
        let report = build_report(
            Vec::new(),
            &[(
                CompactString::const_new("/"),
                vec![CompactString::const_new("missing.js")],
                Vec::new(),
            )],
        );

        assert_eq!(report.routes[0].initial_js, AssetSize::zero());
        assert_eq!(report.routes[0].total, AssetSize::zero());
    }

    #[test]
    fn an_asset_shared_by_entry_and_lazy_is_counted_once_in_the_total() {
        let (_dir, root) = temp_root();
        write(&root, "shared.js", &"const a = 1;\n".repeat(20));

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
        let shared = assets[0].size;
        let report = build_report(
            assets,
            &[(
                CompactString::const_new("/"),
                vec![CompactString::const_new("shared.js")],
                vec![CompactString::const_new("shared.js")],
            )],
        );

        assert_eq!(report.routes[0].total, shared);
        assert_eq!(report.routes[0].assets.len(), 1);
    }

    #[test]
    fn routes_are_sorted_by_path() {
        let report = build_report(
            Vec::new(),
            &[
                (CompactString::const_new("/z"), Vec::new(), Vec::new()),
                (CompactString::const_new("/a"), Vec::new(), Vec::new()),
            ],
        );

        assert_eq!(report.routes[0].path, "/a");
        assert_eq!(report.routes[1].path, "/z");
    }

    #[test]
    fn largest_assets_are_ordered_by_the_requested_metric() {
        let (_dir, root) = temp_root();
        write(&root, "small.js", "const a = 1;\n");
        write(&root, "large.js", &"const b = 2;\n".repeat(200));

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
        let report = build_report(assets, &[]);

        let ordered = report.largest_assets(BudgetMetric::Gzip);
        assert_eq!(ordered[0].path, "large.js");
        assert_eq!(ordered[1].path, "small.js");
    }

    #[test]
    fn totals_group_by_asset_kind() {
        let (_dir, root) = temp_root();
        write(&root, "a.js", "const a = 1;\n");
        write(&root, "b.js", "const b = 2;\n");
        write(&root, "a.css", ".a{}\n");

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
        let report = build_report(assets, &[]);
        let totals = report.total_by_kind();

        assert_eq!(totals.len(), 2);
        assert!(totals.contains_key(&AssetKind::JavaScript));
        assert!(totals.contains_key(&AssetKind::Stylesheet));
    }

    #[test]
    fn the_written_report_is_deterministic_and_carries_its_compression_settings() {
        let (_dir, root) = temp_root();
        write(&root, "app.js", "const a = 1;\n");

        let assets = collect_assets(&root, &ReportOptions::default()).expect("collects");
        let report = build_report(assets, &[]);
        let path = write_report(&root, &report).expect("writes");
        let first = fs::read_to_string(&path).expect("reads");
        let second_path = write_report(&root, &report).expect("writes");
        let second = fs::read_to_string(&second_path).expect("reads");

        assert_eq!(first, second);
        assert!(first.contains("\"gzipLevel\": 9"), "{first}");
        assert!(first.contains("\"brotliQuality\": 11"), "{first}");
        assert!(first.contains("\"initialJs\"") || report.routes.is_empty());
        assert!(first.ends_with('\n'));
    }

    #[test]
    fn report_json_uses_camel_case_keys() {
        let report = build_report(
            Vec::new(),
            &[(CompactString::const_new("/"), Vec::new(), Vec::new())],
        );

        let json = serde_json::to_string(&report).expect("serializes");

        assert!(json.contains("\"initialJs\""), "{json}");
        assert!(json.contains("\"lazyJs\""), "{json}");
        assert!(!json.contains("initial_js"), "{json}");
    }

    #[test]
    fn an_empty_build_reports_zero() {
        let report = BundleReport::default();

        assert_eq!(report.total, AssetSize::zero());
        assert_eq!(
            report.total.get(BudgetMetric::Gzip),
            ByteSize::from_bytes(0)
        );
    }
}
