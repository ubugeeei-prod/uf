use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type SubscriberList = SmallVec<[CellSubscriber; 4]>;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowCell<T> {
    id: CompactString,
    value: T,
    subscribers: SubscriberList,
    revision: u64,
}

impl<T> FlowCell<T> {
    pub fn new(id: impl Into<CompactString>, value: T) -> Self {
        Self {
            id: id.into(),
            value,
            subscribers: SmallVec::new(),
            revision: 0,
        }
    }

    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    pub fn set(&mut self, value: T) -> CellUpdate<'_> {
        self.value = value;
        self.revision += 1;
        CellUpdate {
            id: self.id.as_str(),
            revision: self.revision,
            subscribers: &self.subscribers,
        }
    }

    pub fn subscribe(&mut self, subscriber: CellSubscriber) {
        if self
            .subscribers
            .iter()
            .any(|candidate| candidate.id == subscriber.id)
        {
            return;
        }
        self.subscribers.push(subscriber);
    }

    pub fn unsubscribe(&mut self, id: &str) -> bool {
        let Some(index) = self
            .subscribers
            .iter()
            .position(|subscriber| subscriber.id == id)
        else {
            return false;
        };
        self.subscribers.swap_remove(index);
        true
    }

    pub fn snapshot(&self) -> CellSnapshot<'_, T> {
        CellSnapshot {
            id: self.id.as_str(),
            value: &self.value,
            revision: self.revision,
            subscriber_count: self.subscribers.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellSubscriber {
    pub id: CompactString,
    pub scope: CellScope,
}

impl CellSubscriber {
    pub fn new(id: impl Into<CompactString>, scope: CellScope) -> Self {
        Self {
            id: id.into(),
            scope,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CellScope {
    Client,
    Server,
    ReactRender,
    NativeRuntime,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellUpdate<'cell> {
    pub id: &'cell str,
    pub revision: u64,
    pub subscribers: &'cell [CellSubscriber],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSnapshot<'cell, T> {
    pub id: &'cell str,
    pub value: &'cell T,
    pub revision: u64,
    pub subscriber_count: usize,
}

pub fn cell<T>(id: impl Into<CompactString>, value: T) -> FlowCell<T> {
    FlowCell::new(id, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_cell_with_stable_snapshot() {
        let cell = cell("tone", "calm");
        let snapshot = cell.snapshot();

        assert_eq!(snapshot.id, "tone");
        assert_eq!(*snapshot.value, "calm");
        assert_eq!(snapshot.revision, 0);
        assert_eq!(snapshot.subscriber_count, 0);
    }

    #[test]
    fn set_increments_revision_and_returns_subscribers() {
        let mut cell = cell("count", 1);
        cell.subscribe(CellSubscriber::new("view", CellScope::ReactRender));

        {
            let update = cell.set(2);
            assert_eq!(update.id, "count");
            assert_eq!(update.revision, 1);
            assert_eq!(update.subscribers.len(), 1);
        }
        assert_eq!(cell.revision(), 1);
        assert_eq!(*cell.get(), 2);
    }

    #[test]
    fn subscribe_is_idempotent_by_id() {
        let mut cell = cell("count", 1);

        cell.subscribe(CellSubscriber::new("view", CellScope::ReactRender));
        cell.subscribe(CellSubscriber::new("view", CellScope::Client));

        assert_eq!(cell.subscriber_count(), 1);
    }

    #[test]
    fn unsubscribe_removes_subscriber() {
        let mut cell = cell("count", 1);
        cell.subscribe(CellSubscriber::new("view", CellScope::ReactRender));

        assert!(cell.unsubscribe("view"));
        assert!(!cell.unsubscribe("view"));
        assert_eq!(cell.subscriber_count(), 0);
    }
}
