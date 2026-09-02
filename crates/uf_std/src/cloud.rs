//! Cloud-facing descriptors: cron, S3, SigV4 and deployed functions.
//!
//! Each type records what a request or a schedule is made of without performing
//! it, so the planning that `uf` does offline stays separate from the runtime
//! that eventually issues the call.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};

/// Cron schedule descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronSchedule {
    /// Minute field.
    pub minute: CompactString,
    /// Hour field.
    pub hour: CompactString,
    /// Day-of-month field.
    pub day_of_month: CompactString,
    /// Month field.
    pub month: CompactString,
    /// Day-of-week field.
    pub day_of_week: CompactString,
}

/// Parse a five-field cron schedule.
pub fn parse_cron(source: &str) -> Option<CronSchedule> {
    let mut parts = source.split_whitespace();
    let minute = parts.next()?;
    let hour = parts.next()?;
    let day_of_month = parts.next()?;
    let month = parts.next()?;
    let day_of_week = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    Some(CronSchedule {
        minute: minute.to_compact_string(),
        hour: hour.to_compact_string(),
        day_of_month: day_of_month.to_compact_string(),
        month: month.to_compact_string(),
        day_of_week: day_of_week.to_compact_string(),
    })
}

/// S3 object operation descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3ObjectRequest {
    /// Bucket name.
    pub bucket: CompactString,
    /// Object key.
    pub key: CompactString,
    /// Whether the operation should use SigV4 signing.
    pub sigv4: bool,
}

impl S3ObjectRequest {
    /// Create a signed S3 object request descriptor.
    pub fn new(bucket: &str, key: &str) -> Self {
        Self {
            bucket: bucket.to_compact_string(),
            key: key.to_compact_string(),
            sigv4: true,
        }
    }
}

/// SigV4 credential scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigV4Scope {
    /// AWS region.
    pub region: CompactString,
    /// Service name.
    pub service: CompactString,
}

impl SigV4Scope {
    /// Create a SigV4 credential scope.
    pub fn new(region: &str, service: &str) -> Self {
        Self {
            region: region.to_compact_string(),
            service: service.to_compact_string(),
        }
    }
}

/// Function runtime target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FunctionRuntime {
    /// Worker-compatible runtime.
    Worker,
    /// AWS Lambda-compatible runtime.
    Lambda,
}

/// Function descriptor used by deploy-anywhere adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDescriptor {
    /// Function name.
    pub name: CompactString,
    /// Runtime target.
    pub runtime: FunctionRuntime,
    /// Entry module.
    pub entry: CompactString,
}

impl FunctionDescriptor {
    /// Create a function descriptor.
    pub fn new(name: &str, runtime: FunctionRuntime, entry: &str) -> Self {
        Self {
            name: name.to_compact_string(),
            runtime,
            entry: entry.to_compact_string(),
        }
    }
}
