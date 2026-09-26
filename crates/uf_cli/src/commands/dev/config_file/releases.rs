//! The release lists version completion reads, and how they stay fresh
//! without a request ever waiting for one.
//!
//! # Never on a request's time
//!
//! `runtime: "node@"` is answered from the list `uf_env::index` caches under
//! `$XDG_CACHE_HOME/uf/index`, and nothing else. Reading that file is the most a
//! completion request does. Fetching a list is a `curl` to a publisher that may
//! take a second or may never answer, so it happens on a thread of its own, and
//! its result is picked up by whichever request comes after it lands:
//!
//! * **No list cached:** a refresh starts and the request is answered with an
//!   empty, *incomplete* list. Incomplete is the protocol's word for "ask again
//!   as the user types", so the versions appear a keystroke after they arrive
//!   rather than after the editor is next told to complete.
//! * **A list older than [`REFRESH_AFTER`]:** it is offered as it is — an old
//!   list still names releases that exist — and a refresh starts behind it.
//! * **A refresh that fails:** nothing is offered for that tool, and it is not
//!   tried again for [`RETRY_AFTER`]. A machine with no route to a publisher
//!   would otherwise start a `curl` per keystroke.
//!
//! At most one refresh per tool runs at a time. The thread is not joined: a
//! server told to exit while one is in flight exits, and the cache it would have
//! written is written whole and renamed into place by `uf_env`, so there is no
//! half of one to find later.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use uf_env::Tool;
use uf_env::index::{Bases, Index, cache_dir, cached_in, refresh_in};
use uf_infra::{FxHashMap, FxHashSet};

/// How old a cached list may be before a lookup refreshes it behind itself.
///
/// A day: the publishers release weekly at their busiest, and a list that is a
/// day behind is missing at most the release somebody is least likely to pin.
pub(crate) const REFRESH_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// How long a tool whose refresh failed is left alone before it is tried again.
pub(crate) const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

/// What is known about one tool's releases, right now.
#[derive(Debug)]
pub(crate) enum Lookup<'a> {
    /// A list, from the cache or from a refresh that has landed.
    Ready(&'a Index),
    /// None yet, and a refresh is running that may bring one.
    Pending,
    /// None, and none coming: nowhere to cache one, or a refresh that failed.
    Unavailable,
}

/// Where version completion gets its lists.
///
/// A trait so the completion logic can be tested against a list the test
/// wrote, with no cache directory and no thread.
pub(crate) trait Releases {
    /// `tool`'s releases, without waiting for anything.
    fn lookup(&mut self, tool: Tool) -> Lookup<'_>;
}

/// The lists one editor session has read, and the refreshes it has started.
pub(crate) struct ReleaseLists {
    /// The cache directory, and where a refresh fetches from. [`None`] when
    /// there is no cache directory to mean — no `$HOME`, no `$XDG_CACHE_HOME`.
    source: Option<(Utf8PathBuf, Bases)>,
    /// Lists in memory, so a keystroke does not parse Node's index again.
    loaded: FxHashMap<Tool, Index>,
    /// Tools whose cache file this session has read, whatever it held.
    read: FxHashSet<Tool>,
    refreshing: FxHashSet<Tool>,
    /// When each tool's last refresh failed.
    failed: FxHashMap<Tool, Instant>,
    sender: Sender<(Tool, Option<Index>)>,
    receiver: Receiver<(Tool, Option<Index>)>,
}

impl ReleaseLists {
    /// The cache `uf_env` uses, refreshed from the publishers or from
    /// `$UF_TOOL_INDEX_BASE`.
    pub(crate) fn from_env() -> Self {
        Self::new(cache_dir().ok(), Bases::from_env())
    }

    /// Lists cached in `cache`, refreshed from `bases`.
    pub(crate) fn new(cache: Option<Utf8PathBuf>, bases: Bases) -> Self {
        let (sender, receiver) = channel();
        Self {
            source: cache.map(|dir| (dir, bases)),
            loaded: FxHashMap::default(),
            read: FxHashSet::default(),
            refreshing: FxHashSet::default(),
            failed: FxHashMap::default(),
            sender,
            receiver,
        }
    }

    /// Take in every refresh that has finished since the last lookup.
    fn settle(&mut self) {
        while let Ok((tool, fetched)) = self.receiver.try_recv() {
            self.refreshing.remove(&tool);
            match fetched {
                Some(index) => {
                    self.failed.remove(&tool);
                    self.loaded.insert(tool, index);
                }
                None => {
                    self.failed.insert(tool, Instant::now());
                }
            }
        }
    }

    /// Start refreshing `tool` on a thread, unless one is running or the last
    /// one failed recently. Whether a refresh is running afterwards.
    fn refresh(&mut self, tool: Tool) -> bool {
        let Some((dir, bases)) = &self.source else {
            return false;
        };
        if self.refreshing.contains(&tool) {
            return true;
        }
        if self
            .failed
            .get(&tool)
            .is_some_and(|failed| failed.elapsed() < RETRY_AFTER)
        {
            return false;
        }

        let (dir, bases, sender) = (dir.clone(), bases.clone(), self.sender.clone());
        let started = std::thread::Builder::new()
            .name(uf_infra::cstr!("uf-lsp-index-{tool}").into_string())
            .spawn(move || {
                let fetched = refresh_in(&dir, tool, &bases).ok();
                // The session may have ended; a list nobody will read is fine.
                let _ = sender.send((tool, fetched));
            });
        match started {
            Ok(_) => {
                self.refreshing.insert(tool);
                true
            }
            Err(_) => {
                self.failed.insert(tool, Instant::now());
                false
            }
        }
    }
}

impl Releases for ReleaseLists {
    fn lookup(&mut self, tool: Tool) -> Lookup<'_> {
        self.settle();

        // The cache file, once per session. A refresh that landed first wrote
        // that same file, so what is already loaded is kept.
        if self.read.insert(tool)
            && let Some((dir, _)) = &self.source
            && let Some(index) = cached_in(dir, tool)
        {
            self.loaded.entry(tool).or_insert(index);
        }

        // A clock behind the file is a machine to distrust, not a list to
        // fetch again: `age` is `None` then, and the list counts as fresh.
        let stale = self
            .loaded
            .get(&tool)
            .map(|index| index.age().is_some_and(|age| age > REFRESH_AFTER));
        match stale {
            Some(stale) => {
                if stale {
                    self.refresh(tool);
                }
                self.loaded
                    .get(&tool)
                    .map_or(Lookup::Unavailable, Lookup::Ready)
            }
            None if self.refresh(tool) => Lookup::Pending,
            None => Lookup::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use uf_env::index::{FORMAT, Release};

    use super::*;

    /// nodejs.org's `dist/index.json`, in its own shape.
    const NODE_INDEX: &str = r#"[
      {"version":"v26.10.0","date":"2026-10-01","files":[],"lts":false},
      {"version":"v24.14.0","date":"2026-08-20","files":[],"lts":"Krypton"}
    ]"#;

    fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        (dir, path)
    }

    /// A publisher serving `NODE_INDEX` over `file://`, or serving nothing.
    fn publisher(serves: bool) -> (tempfile::TempDir, Bases) {
        let (guard, root) = temp();
        if serves {
            std::fs::write(root.join("index.json"), NODE_INDEX).unwrap();
        }
        (guard, Bases::at(uf_infra::cstr!("file://{root}").as_str()))
    }

    fn versions(lookup: &Lookup<'_>) -> Option<Vec<String>> {
        match lookup {
            Lookup::Ready(index) => Some(
                index
                    .releases
                    .iter()
                    .map(|release| release.version.clone())
                    .collect(),
            ),
            Lookup::Pending | Lookup::Unavailable => None,
        }
    }

    /// Look `tool` up until `done` says so, the way an editor asks again as
    /// somebody types. Panics after ten seconds.
    fn until(lists: &mut ReleaseLists, tool: Tool, done: impl Fn(&Lookup<'_>) -> bool) {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            if done(&lists.lookup(tool)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("`{tool}` never got there: {:?}", lists.lookup(tool));
    }

    #[test]
    fn a_missing_list_is_fetched_behind_the_request_and_offered_when_it_lands() {
        let (_publisher, bases) = publisher(true);
        let (_cache, dir) = temp();
        let mut lists = ReleaseLists::new(Some(dir.clone()), bases);

        // Answered at once, with nothing — and a refresh started.
        assert!(matches!(lists.lookup(Tool::Node), Lookup::Pending));

        until(&mut lists, Tool::Node, |lookup| {
            versions(lookup).is_some_and(|versions| versions == ["26.10.0", "24.14.0"])
        });
        // And cached, for the next session.
        assert!(cached_in(&dir, Tool::Node).is_some());
    }

    #[test]
    fn an_old_list_is_offered_at_once_and_refreshed_behind_itself() {
        let (_publisher, bases) = publisher(true);
        let (_cache, dir) = temp();
        let old = Index {
            format: FORMAT,
            tool: Tool::Node,
            fetched_at: 0,
            sources: Vec::new(),
            releases: vec![Release::new("20.0.0")],
        };
        std::fs::write(dir.join("node.json"), serde_json::to_string(&old).unwrap()).unwrap();
        let mut lists = ReleaseLists::new(Some(dir), bases);

        assert_eq!(
            versions(&lists.lookup(Tool::Node)),
            Some(vec![String::from("20.0.0")])
        );
        until(&mut lists, Tool::Node, |lookup| {
            versions(lookup).is_some_and(|versions| versions.contains(&String::from("26.10.0")))
        });
    }

    #[test]
    fn a_fresh_list_starts_no_refresh() {
        let (_publisher, bases) = publisher(true);
        let (_cache, dir) = temp();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let fresh = Index {
            format: FORMAT,
            tool: Tool::Bun,
            fetched_at: now,
            sources: Vec::new(),
            releases: vec![Release::new("1.4.2")],
        };
        std::fs::write(dir.join("bun.json"), serde_json::to_string(&fresh).unwrap()).unwrap();
        let mut lists = ReleaseLists::new(Some(dir), bases);

        assert!(matches!(lists.lookup(Tool::Bun), Lookup::Ready(_)));
        assert!(lists.refreshing.is_empty());
    }

    #[test]
    fn a_list_that_cannot_be_fetched_is_not_asked_for_on_every_keystroke() {
        let (_publisher, bases) = publisher(false);
        let (_cache, dir) = temp();
        let mut lists = ReleaseLists::new(Some(dir), bases);

        assert!(matches!(lists.lookup(Tool::Node), Lookup::Pending));
        until(&mut lists, Tool::Node, |lookup| {
            matches!(lookup, Lookup::Unavailable)
        });
        // Still unavailable, and not pending again: no second `curl`.
        assert!(matches!(lists.lookup(Tool::Node), Lookup::Unavailable));
        assert!(lists.refreshing.is_empty());
    }

    #[test]
    fn with_nowhere_to_cache_there_is_nothing_to_wait_for() {
        let mut lists = ReleaseLists::new(None, Bases::default());

        assert!(matches!(lists.lookup(Tool::Node), Lookup::Unavailable));
        assert!(lists.refreshing.is_empty());
    }
}
