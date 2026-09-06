#![deny(missing_docs)]
//! Bundle size measurement and budget enforcement for uniflowed.
//!
//! Shipped JavaScript weight is a product requirement, not a report nobody
//! reads, so this crate does two things and keeps them separate:
//!
//! - [`report`] measures what a build actually emitted — raw, gzip, and brotli
//!   bytes per asset, and what each route costs a visitor.
//! - [`budget`] checks that picture against ceilings declared in `uf.config.js`
//!   and returns **every** violation, so a size regression is one CI failure
//!   rather than a sequence of them.
//! - [`embed`] packs the same set of assets into a JavaScript module, which is
//!   how `uf build --compile` gets `dist/` inside a standalone binary.
//!
//! Compressed figures come from really compressing the bytes at fixed settings
//! ([`size::GZIP_LEVEL`], [`size::BROTLI_QUALITY`]), never from an estimate, and
//! those settings are written into the report so two runs are only ever compared
//! on equal terms.

pub mod budget;
pub mod embed;
pub mod report;
pub mod size;

pub use budget::{
    BudgetOutcome, BudgetScope, BudgetViolation, BudgetViolations, BundleBudgets, SizeBudget,
    evaluate,
};
pub use embed::{EmbedError, Embedded, content_type, write_embedded_assets};
pub use report::{
    AssetEntry, AssetKind, BundleReport, MAX_ASSET_DEPTH, MAX_ASSETS, ReportError, ReportOptions,
    RouteEntry, build_report, collect_assets, write_report,
};
pub use size::{
    AssetSize, BROTLI_QUALITY, BudgetMetric, ByteSize, ByteSizeParseError, GZIP_LEVEL,
    MAX_ASSET_BYTES, MeasureError, measure, parse_byte_size,
};

#[cfg(test)]
mod tests;
