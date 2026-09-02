//! The verdict an invalidation reaches, and the vocabulary for saying it.
//!
//! Split from the walk itself because these are the types the rest of the
//! toolchain sees: they serialize into the update payload, they name the
//! terminal line, and [`ReloadReason`] is what turns a silent page reload into
//! one a developer can act on.

use serde::{Deserialize, Serialize};

use crate::hmr::graph::DevModuleId;

/// What happened to the file the invalidation starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    /// The file did not exist the last time the tree was polled.
    Created,
    /// The file exists and its contents or size changed.
    Modified,
    /// The file has stopped existing.
    Deleted,
}

impl ChangeKind {
    /// Stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }
}

/// Which half of the app a stale module belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UpdateSide {
    /// The module is in the browser bundle: it is re-fetched over the update
    /// channel.
    Client,
    /// The module runs while rendering: the route re-renders and the browser
    /// bundle is left alone.
    Server,
}

/// What the browser and the server must do about a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateKind {
    /// Nothing at runtime observes the change.
    Inert,
    /// Client modules swap in place through React Fast Refresh.
    Hot,
    /// A server module changed: the route re-renders, the browser bundle is
    /// untouched.
    Route,
    /// Both halves are stale, because shared code changed.
    HotAndRoute,
    /// The client graph has nothing that can accept the update in place.
    FullReload,
}

impl UpdateKind {
    /// Stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inert => "inert",
            Self::Hot => "hot",
            Self::Route => "route",
            Self::HotAndRoute => "hot-and-route",
            Self::FullReload => "full-reload",
        }
    }

    /// Whether the browser has to throw its module registry away.
    pub fn is_full_reload(self) -> bool {
        matches!(self, Self::FullReload)
    }

    /// Whether anything at all has to happen.
    pub fn is_inert(self) -> bool {
        matches!(self, Self::Inert)
    }
}

/// Why an update could not be applied in place.
///
/// Every fallback to a full reload names one of these, and `uf dev` prints it.
/// A reload the developer cannot explain is a reload they will blame on the
/// tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReloadReason {
    /// The update reached the top of the client graph without finding a module
    /// whose exports are all components.
    NoAcceptingBoundary,
    /// A module the browser had already loaded stopped existing.
    ModuleRemoved,
    /// The importer chain is deeper than
    /// [`MAX_INVALIDATION_DEPTH`](super::MAX_INVALIDATION_DEPTH).
    DepthExceeded,
    /// The changed module has no request target the browser could fetch it
    /// from.
    Unservable,
    /// The update names more modules than one payload will carry.
    TooManyModules,
}

impl ReloadReason {
    /// One sentence a developer can act on.
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoAcceptingBoundary => "no client module accepts the update",
            Self::ModuleRemoved => "a loaded module was deleted",
            Self::DepthExceeded => "the import chain is deeper than the invalidation bound",
            Self::Unservable => "the changed module has no servable request target",
            Self::TooManyModules => "the update touches more modules than one payload carries",
        }
    }

    /// Stable kebab-case name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoAcceptingBoundary => "no-accepting-boundary",
            Self::ModuleRemoved => "module-removed",
            Self::DepthExceeded => "depth-exceeded",
            Self::Unservable => "unservable",
            Self::TooManyModules => "too-many-modules",
        }
    }
}

/// What one change made stale.
///
/// The two module lists are disjoint by construction only in the common case: a
/// module reached from both a Client Component and a route legitimately appears
/// in both, because both halves have to be told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Invalidation {
    pub(super) client: Vec<DevModuleId>,
    pub(super) server: Vec<DevModuleId>,
    pub(super) boundaries: Vec<DevModuleId>,
    pub(super) reload: Option<ReloadReason>,
}

impl Invalidation {
    /// Client modules the browser must re-fetch, in identifier order.
    pub fn client(&self) -> &[DevModuleId] {
        &self.client
    }

    /// Server modules whose rendered output is now stale, in identifier order.
    pub fn server(&self) -> &[DevModuleId] {
        &self.server
    }

    /// Client modules that accept the update in place.
    pub fn boundaries(&self) -> &[DevModuleId] {
        &self.boundaries
    }

    /// Why a full reload is required, when one is.
    pub fn reload_reason(&self) -> Option<ReloadReason> {
        self.reload
    }

    /// Whether nothing at all was invalidated.
    pub fn is_empty(&self) -> bool {
        self.client.is_empty() && self.server.is_empty() && self.reload.is_none()
    }

    /// How many modules were invalidated, counting a shared module once per
    /// half it belongs to.
    pub fn len(&self) -> usize {
        self.client.len() + self.server.len()
    }

    /// The verdict the browser acts on.
    pub fn kind(&self) -> UpdateKind {
        if self.reload.is_some() {
            return UpdateKind::FullReload;
        }
        match (self.client.is_empty(), self.server.is_empty()) {
            (true, true) => UpdateKind::Inert,
            (false, true) => UpdateKind::Hot,
            (true, false) => UpdateKind::Route,
            (false, false) => UpdateKind::HotAndRoute,
        }
    }
}
