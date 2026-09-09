use super::*;

#[test]
fn default_contract_is_wintertc_flow_over_capability_js_hosts() {
    let contract = RuntimeContract::default();

    assert_eq!(contract.standard, RuntimeStandard::WinterTc);
    assert_eq!(contract.language, RuntimeLanguage::Flow);
    assert_eq!(
        contract.javascript_engine,
        JavaScriptEngine::CapabilityJsHost
    );
    assert_eq!(contract.event_loop, EventLoopModel::HostProvided);
    assert_eq!(contract.io, NativeIoModel::HostCapabilityBindings);
}

#[test]
fn default_contract_targets_capability_js_hosts() {
    let contract = RuntimeContract::default();

    assert!(contract.supports_host(RuntimeHost::Node));
    assert!(contract.supports_host(RuntimeHost::Deno));
    assert!(contract.supports_host(RuntimeHost::Bun));
    assert!(!contract.supports_host(RuntimeHost::Uf));
}

#[test]
fn default_contract_exposes_server_and_io_capabilities() {
    let contract = RuntimeContract::default();

    assert!(contract.has_capability(RuntimeCapability::Fetch));
    assert!(contract.has_capability(RuntimeCapability::Streams));
    assert!(contract.has_capability(RuntimeCapability::Tcp));
    assert!(contract.has_capability(RuntimeCapability::Tls));
    assert!(contract.has_capability(RuntimeCapability::Dns));
    assert!(contract.has_capability(RuntimeCapability::Cron));
    assert!(contract.has_capability(RuntimeCapability::S3));
    assert!(contract.has_capability(RuntimeCapability::SigV4));
    assert!(contract.has_capability(RuntimeCapability::Functions));
    assert!(contract.has_capability(RuntimeCapability::WebAssembly));
    assert!(contract.has_capability(RuntimeCapability::ServerActions));
    assert!(contract.has_capability(RuntimeCapability::ReactServerComponents));
    assert!(contract.has_capability(RuntimeCapability::TerminalUi));
}

#[test]
fn postponed_hermes_contract_remains_explicit() {
    let contract = RuntimeContract::wintertc_hermes_native();

    assert_eq!(contract.javascript_engine, JavaScriptEngine::Hermes);
    assert_eq!(contract.event_loop, EventLoopModel::RustNativeLibuvParity);
    assert_eq!(contract.io, NativeIoModel::ZeroCopyStreaming);
    assert!(contract.supports_host(RuntimeHost::Uf));
    assert!(contract.has_capability(RuntimeCapability::NativePackages));
}

/// Every host has a row, so `HostSupport::for_host` cannot panic.
///
/// The table is a `const` slice rather than a match, which is what makes it
/// readable as a table and what makes a forgotten variant possible; this is the
/// exhaustiveness a match would have given for free.
#[test]
fn every_host_has_a_support_row() {
    for host in RuntimeHost::ALL {
        assert_eq!(HostSupport::for_host(*host).host, *host);
    }
    assert_eq!(HOSTS.len(), RuntimeHost::ALL.len());
}

/// And every host the default contract targets has one too.
#[test]
fn every_targeted_host_has_a_support_row() {
    for host in &RuntimeContract::default().hosts {
        assert_eq!(HostSupport::for_host(*host).host, *host);
    }
    for host in &RuntimeContract::wintertc_hermes_native().hosts {
        assert_eq!(HostSupport::for_host(*host).host, *host);
    }
}

/// "Implemented" is a claim, and a claim needs the test that checks it.
///
/// This is the rule the table exists to enforce. Bun was described as working
/// in the README, in `docs/architecture.md` and in this crate's own host list
/// while `packages/host/bun-preload.js` could not load an ordinary dependency,
/// and the reason nobody noticed is that no test had ever started Bun. A row
/// that says `Implemented` with no `verified_by` is that state of affairs
/// written down again, so it fails here instead.
#[test]
fn an_implemented_host_names_the_test_that_starts_it() {
    for support in HOSTS {
        if support.level == SupportLevel::Implemented {
            assert!(
                support.verified_by.is_some(),
                "{:?} claims to be implemented and no test starts it",
                support.host
            );
            assert!(
                support.loads_flow(),
                "{:?} claims to be implemented and has no Flow loader",
                support.host
            );
        }
    }
}

/// A host that is not implemented says what it is waiting for.
#[test]
fn a_host_that_is_not_implemented_says_what_is_missing() {
    for support in HOSTS {
        if support.level != SupportLevel::Implemented {
            assert!(
                support.missing.is_some(),
                "{:?} is not implemented and does not say why",
                support.host
            );
        }
    }
}

/// The two hosts a uf project runs on today, and no others.
///
/// Written as an assertion rather than left implicit because the whole point of
/// the table is that adding a name to it is a decision somebody makes on
/// purpose. A third row turning `Implemented` has to change this line, and the
/// person changing it has to have the test in `verified_by` in front of them.
#[test]
fn node_and_bun_are_the_implemented_hosts() {
    let implemented: Vec<RuntimeHost> = HOSTS
        .iter()
        .filter(|support| support.level == SupportLevel::Implemented)
        .map(|support| support.host)
        .collect();
    assert_eq!(implemented, vec![RuntimeHost::Node, RuntimeHost::Bun]);
}

/// Deno enforces the whole permission set; Node enforces half of it.
#[test]
fn the_permission_table_matches_what_each_host_has() {
    assert_eq!(
        HostSupport::for_host(RuntimeHost::Deno).enforces,
        Permission::ALL
    );
    assert_eq!(
        HostSupport::for_host(RuntimeHost::Node).enforces,
        &[Permission::Read, Permission::Write]
    );
    assert!(HostSupport::for_host(RuntimeHost::Bun).enforces.is_empty());
}

mod permissions {
    use crate::permissions::{PermissionError, ToolchainAccess, explain, host_arguments};
    use crate::{Permission, Permissions, RuntimeHost};

    fn declared() -> Permissions {
        Permissions {
            read: vec![String::from("/home/me/fixtures")],
            write: vec![String::from("/tmp/out")],
            ..Permissions::default()
        }
    }

    fn toolchain() -> ToolchainAccess {
        ToolchainAccess {
            read: vec![String::from("/project")],
            write: vec![String::from("/project/.uf")],
            run: vec![String::from("/usr/local/bin/uf")],
            // The variables uf sets on the worker itself. Named on the host
            // that denies by default, and inert on the two that do not.
            env: vec![String::from("UF_BINARY")],
            loader_thread: true,
        }
    }

    #[test]
    fn an_undeclared_set_grants_nothing_and_is_still_a_sandbox() {
        // The empty set is not "no permissions block" — that case never reaches
        // here, because the caller holds an `Option`. This is
        // `permissions: {}`, which means deny everything the toolchain does not
        // need, and it has to produce a real `--permission` rather than an
        // empty argument list that would silently run unsandboxed.
        let arguments =
            host_arguments(RuntimeHost::Node, &Permissions::default(), &toolchain()).unwrap();
        assert_eq!(arguments.first().map(String::as_str), Some("--permission"));
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "--allow-fs-read=/project")
        );
        assert!(
            !arguments
                .iter()
                .any(|argument| argument.contains("/home/me"))
        );
    }

    #[test]
    fn node_grants_the_declared_paths_on_top_of_the_ones_uf_needs() {
        let arguments = host_arguments(RuntimeHost::Node, &declared(), &toolchain()).unwrap();
        assert_eq!(
            arguments,
            vec![
                String::from("--permission"),
                String::from("--allow-fs-read=/project"),
                String::from("--allow-fs-read=/home/me/fixtures"),
                String::from("--allow-fs-write=/project/.uf"),
                String::from("--allow-fs-write=/tmp/out"),
                // Because Node runs the module hooks on a loader thread, and
                // because the loader starts `uf transform` — and Node can say
                // neither which thread nor which program.
                String::from("--allow-worker"),
                String::from("--allow-child-process"),
            ]
        );
    }

    /// The refusal that is the whole design.
    ///
    /// Node has no network dimension at all. Passing the four flags it does
    /// have and dropping `net` would produce a run that reads as sandboxed and
    /// lets a test post the environment to anywhere, which is the failure
    /// `docs/security.md` names as worse than having no model.
    #[test]
    fn node_refuses_a_set_it_could_only_partly_apply() {
        let permissions = Permissions {
            net: vec![String::from("registry.npmjs.org")],
            env: vec![String::from("CI")],
            ..declared()
        };
        let error = host_arguments(RuntimeHost::Node, &permissions, &toolchain()).unwrap_err();
        assert_eq!(
            error,
            PermissionError::Unenforceable {
                host: RuntimeHost::Node,
                permissions: vec![Permission::Net, Permission::Env],
            }
        );
        let message = error.to_string();
        assert!(message.contains("`net`"), "{message}");
        assert!(message.contains("Deno"), "{message}");
    }

    /// `run` too, and for a reason worth keeping separate.
    ///
    /// `--allow-child-process` exists; it is just not a list. Translating a
    /// two-program list into "any program" is a widening, so it is refused —
    /// while uf's *own* need for a child is granted and disclosed, because that
    /// one is not a claim about the project's list.
    #[test]
    fn node_refuses_a_run_list_it_could_only_widen() {
        let permissions = Permissions {
            run: vec![String::from("git")],
            ..Permissions::default()
        };
        let error = host_arguments(RuntimeHost::Node, &permissions, &toolchain()).unwrap_err();
        assert_eq!(
            error,
            PermissionError::Unenforceable {
                host: RuntimeHost::Node,
                permissions: vec![Permission::Run],
            }
        );
    }

    #[test]
    fn deno_translates_all_five_as_one_flag_each() {
        let permissions = Permissions {
            net: vec![String::from("registry.npmjs.org")],
            env: vec![String::from("CI")],
            run: vec![String::from("git")],
            ..declared()
        };
        let arguments = host_arguments(RuntimeHost::Deno, &permissions, &toolchain()).unwrap();
        assert_eq!(
            arguments,
            vec![
                String::from("--allow-read=/project,/home/me/fixtures"),
                String::from("--allow-write=/project/.uf,/tmp/out"),
                String::from("--allow-net=registry.npmjs.org"),
                // uf's own variable first, then the project's, the way every
                // other category is ordered: what the toolchain needs to start
                // the run, then what the project asked for.
                String::from("--allow-env=UF_BINARY,CI"),
                String::from("--allow-run=/usr/local/bin/uf,git"),
            ]
        );
    }

    /// A permission with nothing in it gets no flag, which is Deno's "denied".
    ///
    /// Writing `--allow-net=` would be the opposite of what it looks like: an
    /// empty value is how Deno spells *every* host.
    ///
    /// `net` is the one category with nothing on either side here: uf never
    /// opens a socket on a project's behalf, so an undeclared `net` is a
    /// category with no flag at all. The environment is not that case any more
    /// — uf sets variables on the worker itself and has to name them — which is
    /// why this asserts about the two separately rather than about "the
    /// categories uf does not need".
    #[test]
    fn deno_writes_no_flag_for_a_permission_with_no_entries() {
        let arguments =
            host_arguments(RuntimeHost::Deno, &Permissions::default(), &toolchain()).unwrap();
        assert!(
            !arguments
                .iter()
                .any(|argument| argument.starts_with("--allow-net"))
        );
        assert_eq!(
            arguments
                .iter()
                .find(|argument| argument.starts_with("--allow-env"))
                .map(String::as_str),
            Some("--allow-env=UF_BINARY"),
            "uf's own variables are named; nothing else is"
        );
    }

    /// A comma in an entry would arrive as a second grant nobody wrote.
    ///
    /// Deno takes one flag per permission and splits its value on commas — a
    /// repeated `--allow-read` is an argument error, not a second grant — so
    /// `/tmp/a,/etc` is two paths by the time Deno reads it. There is no
    /// quoting to reach for, which leaves refusing.
    #[test]
    fn deno_refuses_an_entry_a_comma_would_split() {
        let permissions = Permissions {
            read: vec![String::from("/tmp/a,/etc")],
            ..Permissions::default()
        };
        let error = host_arguments(RuntimeHost::Deno, &permissions, &ToolchainAccess::default())
            .unwrap_err();
        assert_eq!(
            error,
            PermissionError::Unexpressible {
                host: RuntimeHost::Deno,
                permission: Permission::Read,
                entry: String::from("/tmp/a,/etc"),
            }
        );
        assert!(error.to_string().contains("comma"), "{error}");
    }

    #[test]
    fn bun_refuses_the_whole_set_because_it_has_nothing_to_enforce_it_with() {
        let error = host_arguments(RuntimeHost::Bun, &declared(), &toolchain()).unwrap_err();
        assert_eq!(
            error,
            PermissionError::Unenforceable {
                host: RuntimeHost::Bun,
                permissions: vec![Permission::Read, Permission::Write],
            }
        );
        assert!(error.to_string().contains("no permission model"), "{error}");
    }

    /// `uf explain` has to name the grants uf added, not only the declared ones.
    #[test]
    fn explaining_a_run_counts_the_grants_uf_added_itself() {
        let lines = explain(RuntimeHost::Node, &declared(), &toolchain());
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("read: 1 declared, 1 added by uf")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("net: 0 declared, 0 added by uf, not enforced")),
            "{lines:?}"
        );
    }
}

mod documentation {
    use std::path::{Path, PathBuf};

    use crate::{HOSTS, HostSupport, RuntimeHost, SupportLevel};

    /// This repository's root, from the crate manifest.
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("the crate has a parent")
    }

    /// A row that names a test names one that exists.
    ///
    /// `verified_by` is the only thing separating this table from the prose it
    /// replaced, and a path that has been renamed out from under it is that
    /// prose again — a sentence asserting somebody checked, with nothing behind
    /// it. The entries are comma-separated because Node's claims are checked in
    /// more than one place.
    #[test]
    fn a_row_that_names_a_test_names_one_that_exists() {
        for support in HOSTS {
            let Some(verified_by) = support.verified_by else {
                continue;
            };
            for path in verified_by.split(',') {
                let path = repo_root().join(path.trim());
                assert!(
                    path.is_file(),
                    "{:?} says it is checked by {} and that file is not there",
                    support.host,
                    path.display()
                );
            }
        }
    }

    /// `docs/hosts.md` grades the same hosts, in the same order.
    ///
    /// The page opens by saying the table is the source of truth and it is not,
    /// which is only true while something keeps them equal. Order included: the
    /// page is read top to bottom and a reader who finds Deno above Bun will
    /// take the ranking for the answer.
    #[test]
    fn the_documented_matrix_matches_the_table() {
        let page = std::fs::read_to_string(repo_root().join("docs/hosts.md"))
            .expect("docs/hosts.md is part of this repository");
        let matrix = page
            .split("## The matrix")
            .nth(1)
            .expect("docs/hosts.md has a matrix")
            .split("\n## ")
            .next()
            .expect("the matrix ends somewhere");
        let rows: Vec<&str> = matrix
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('|'))
            // The header row and the `| --- |` rule under it.
            .skip(2)
            .collect();

        assert_eq!(
            rows.len(),
            HOSTS.len(),
            "docs/hosts.md has {} rows for {} hosts",
            rows.len(),
            HOSTS.len()
        );
        for (row, support) in rows.iter().zip(HOSTS) {
            assert!(
                row.contains(support.level.as_str()),
                "{:?} is {} in the table and its row in docs/hosts.md does not say so:\n{row}",
                support.host,
                support.level.as_str()
            );
        }
    }

    /// And the page names the issue each unfinished host is waiting on.
    #[test]
    fn the_page_names_every_tracking_issue() {
        let page = std::fs::read_to_string(repo_root().join("docs/hosts.md"))
            .expect("docs/hosts.md is part of this repository");
        for support in HOSTS {
            if let Some(issue) = support.tracking_issue {
                assert!(
                    page.contains(&format!("#{issue}")),
                    "{:?} is tracked by #{issue} and docs/hosts.md does not mention it",
                    support.host
                );
            }
        }
    }

    /// The two hosts a project runs on are the two the page calls implemented.
    #[test]
    fn only_node_and_bun_are_documented_as_implemented() {
        let page = std::fs::read_to_string(repo_root().join("docs/hosts.md"))
            .expect("docs/hosts.md is part of this repository");
        assert_eq!(
            page.matches("**implemented**").count(),
            HOSTS
                .iter()
                .filter(|support| support.level == SupportLevel::Implemented)
                .count(),
            "adding a host to the table means grading it on the page too"
        );
        for host in [RuntimeHost::Node, RuntimeHost::Bun] {
            assert_eq!(HostSupport::for_host(host).level, SupportLevel::Implemented);
        }
    }
}
