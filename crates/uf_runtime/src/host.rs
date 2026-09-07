//! What each host actually does today, as data rather than as prose.
//!
//! [`RuntimeContract`](crate::RuntimeContract) says which hosts a contract is
//! *written for*. This says what each of them does when you run it, which is a
//! different question and for a while was answered in three places that
//! disagreed: an enum listing Node, Deno and Bun as equals, a README claiming
//! Bun worked while `packages/host/bun-preload.js` could not load a single
//! ordinary dependency (ubugeeei-prod/uf#418), and `uf test`'s own
//! `HostCommand::loads_flow`, which was the only honest record and was private
//! to the test runner.
//!
//! So the table is the record, one row per host, and the rows are graded the
//! way `ubugeeei-redundancy.md` grades everything: **Implemented**,
//! **Experimental**, **Planned**, kept distinct. A row that claims
//! [`SupportLevel::Implemented`] has to name the test that starts a real
//! process of that host — [`HostSupport::verified_by`] — and
//! [`crate::tests`] fails if one does not.
//!
//! An adapter interface is not compatibility. A name in an enum is not
//! compatibility. A test that starts the runtime and watches it finish is, and
//! this is where a reader finds out which hosts have one.

use crate::kind::RuntimeHost;
use crate::permissions::Permission;

/// How much of a uf *project* runs on a host.
///
/// About the project, not about every claim in the row: Deno enforces the
/// whole permission set and is still [`SupportLevel::Planned`], because
/// nothing a uf project is made of can be imported there yet. Grading the row
/// by its best feature is how "Deno is supported" got written down in the
/// first place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SupportLevel {
    /// Runs uf projects, and a test in this repository starts it and checks.
    Implemented,
    /// Runs something real, with a named limitation a user will meet.
    Experimental,
    /// Nothing runs there yet. The row says what it would take.
    Planned,
}

impl SupportLevel {
    /// The word this level is written as, in documentation and in `uf inspect`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::Experimental => "experimental",
            Self::Planned => "planned",
        }
    }
}

/// What one host does, and what it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostSupport {
    /// The host this row is about.
    pub host: RuntimeHost,
    /// How much of a uf project runs there.
    pub level: SupportLevel,
    /// The module that teaches this host to load Flow, when one exists.
    ///
    /// `None` is the whole of what "Deno is a name in an enum" meant: without
    /// one, a Flow file reaching the host is a syntax error from a runtime that
    /// was never told what the syntax is.
    pub flow_loader: Option<&'static str>,
    /// The permissions this host enforces, of the five uf can declare.
    ///
    /// Empty means the host has no permission model, which
    /// [`crate::permissions::host_arguments`] turns into a refusal rather than
    /// a silent grant.
    pub enforces: &'static [Permission],
    /// The test that starts a real process of this host and checks this row.
    ///
    /// `None` is a claim nobody has checked, and no row may be
    /// [`SupportLevel::Implemented`] with `None` here. A
    /// [`SupportLevel::Planned`] row may still have one: `deno_host.rs` starts
    /// Deno to establish that the gaps named in [`Self::missing`] are the real
    /// gaps and that the permissions it claims to enforce are enforced, which
    /// is the difference between a plan and a guess.
    pub verified_by: Option<&'static str>,
    /// What this host still needs, in one sentence, when it needs something.
    pub missing: Option<&'static str>,
    /// The issue that tracks the rest of the work.
    pub tracking_issue: Option<u32>,
}

/// Every host, in the order they are documented.
///
/// The order is "what a person is most likely to be running", not the enum's:
/// the two that work, the one that half works, then the deployment shapes and
/// the runtime that does not exist yet.
pub const HOSTS: &[HostSupport] = &[
    HostSupport {
        host: RuntimeHost::Node,
        level: SupportLevel::Implemented,
        flow_loader: Some("@uniflowed/host/register"),
        // Node's permission system is `--allow-fs-read` and `--allow-fs-write`
        // and nothing else: no network dimension, no environment dimension, and
        // `--allow-child-process` is every program or none.
        enforces: &[Permission::Read, Permission::Write],
        verified_by: Some("crates/uf_cli/tests/testing.rs, crates/uf_cli/tests/permissions.rs"),
        missing: None,
        tracking_issue: None,
    },
    HostSupport {
        host: RuntimeHost::Bun,
        level: SupportLevel::Implemented,
        flow_loader: Some("@uniflowed/host/bun-preload"),
        enforces: &[],
        verified_by: Some("crates/uf_cli/tests/bun_host.rs"),
        missing: Some(
            "a permission model of any kind; a declared permission set is refused here rather \
             than silently granted",
        ),
        tracking_issue: None,
    },
    HostSupport {
        host: RuntimeHost::Deno,
        level: SupportLevel::Planned,
        flow_loader: None,
        // The only host that enforces the whole set, which is the reason the
        // model has the shape it has.
        enforces: Permission::ALL,
        verified_by: Some("crates/uf_cli/tests/deno_host.rs"),
        missing: Some(
            "a Flow loader. Deno has no module hook to install one in, so this is an \
             ahead-of-time transform and an import map rather than a `register.js` — and on the \
             Deno line uf has measured, that map is what a bare specifier needs as well",
        ),
        tracking_issue: Some(246),
    },
    HostSupport {
        host: RuntimeHost::Edge,
        level: SupportLevel::Planned,
        flow_loader: None,
        // Not "no permission model": the platform *is* the sandbox, and a
        // worker cannot be handed `--allow-read` because it has no filesystem
        // to read. A permission set is refused there for a different reason
        // than on Bun, and the two should not be written as one.
        enforces: &[],
        verified_by: None,
        missing: Some(
            "everything: there is no host at all. A worker runtime has no loader hook and no \
             child process, so the transform has to happen before deployment — the same \
             ahead-of-time question the standalone binary asks, answered once for both",
        ),
        tracking_issue: Some(246),
    },
    HostSupport {
        host: RuntimeHost::Serverless,
        level: SupportLevel::Planned,
        flow_loader: None,
        enforces: &[],
        verified_by: None,
        missing: Some(
            "nothing of its own: a serverless function runs the `node` adapter's output, so what \
             is missing is a test that runs it there rather than a host binding",
        ),
        tracking_issue: Some(391),
    },
    HostSupport {
        host: RuntimeHost::Container,
        level: SupportLevel::Planned,
        flow_loader: None,
        enforces: &[],
        verified_by: None,
        missing: Some(
            "nothing of its own: a container runs Node, and `uf build --adapter container` \
             already writes what it runs",
        ),
        tracking_issue: Some(391),
    },
    HostSupport {
        host: RuntimeHost::Uf,
        level: SupportLevel::Planned,
        flow_loader: None,
        enforces: &[],
        verified_by: None,
        missing: Some(
            "the runtime. `RuntimeContract::wintertc_hermes_native` is the contract it would \
             satisfy, kept explicit precisely so that it is not mistaken for one that exists",
        ),
        tracking_issue: None,
    },
];

impl HostSupport {
    /// The row for one host.
    ///
    /// Every [`RuntimeHost`] has one — [`crate::tests`] is what keeps that
    /// true when a variant is added — so this cannot fail and does not return
    /// an `Option` a caller would have to invent an answer for.
    #[must_use]
    pub fn for_host(host: RuntimeHost) -> &'static Self {
        HOSTS
            .iter()
            .find(|support| support.host == host)
            .expect("every RuntimeHost has a support row; uf_runtime::tests asserts it")
    }

    /// Whether a Flow module can be imported on this host without a build step.
    #[must_use]
    pub const fn loads_flow(&self) -> bool {
        self.flow_loader.is_some()
    }

    /// Whether this host can enforce one permission.
    #[must_use]
    pub fn enforces(&self, permission: Permission) -> bool {
        self.enforces.contains(&permission)
    }
}
