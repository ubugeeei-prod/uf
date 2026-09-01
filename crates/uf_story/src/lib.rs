use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uf_browser::{BrowserPlan, VisualSnapshot};
use uf_mock::MockRegistry;

pub type StoryList = SmallVec<[Story; 16]>;
pub type VariantList = SmallVec<[StoryVariant; 8]>;
pub type SnapshotList = SmallVec<[VisualSnapshot; 16]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Storybook {
    pub stories: StoryList,
    pub mocks: MockRegistry,
    pub browser: BrowserPlan,
}

impl Default for Storybook {
    fn default() -> Self {
        Self {
            stories: SmallVec::new(),
            mocks: MockRegistry::new(),
            browser: BrowserPlan::default(),
        }
    }
}

impl Storybook {
    pub fn story(mut self, story: Story) -> Self {
        self.stories.push(story);
        self
    }

    pub fn visual_snapshots(&self) -> SnapshotList {
        let mut snapshots = SnapshotList::new();
        for story in &self.stories {
            for variant in &story.variants {
                for viewport in &self.browser.viewports {
                    snapshots.push(VisualSnapshot::new(variant.id.as_str(), viewport));
                }
            }
        }
        snapshots
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Story {
    pub id: CompactString,
    pub title: CompactString,
    pub module: CompactString,
    pub variants: VariantList,
}

impl Story {
    pub fn new(id: impl Into<CompactString>, module: impl Into<CompactString>) -> Self {
        let id = id.into();
        Self {
            title: id.clone(),
            id,
            module: module.into(),
            variants: SmallVec::new(),
        }
    }

    pub fn variant(mut self, variant: StoryVariant) -> Self {
        self.variants.push(variant);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryVariant {
    pub id: CompactString,
    pub name: CompactString,
    pub server_component_safe: bool,
}

impl StoryVariant {
    pub fn new(id: impl Into<CompactString>, name: impl Into<CompactString>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            server_component_safe: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_visual_snapshot_matrix_from_stories_and_viewports() {
        let stories = Storybook::default().story(
            Story::new("button", "crates/uf_lib/lib/ui/button")
                .variant(StoryVariant::new("button-primary", "Primary")),
        );

        let snapshots = stories.visual_snapshots();

        assert_eq!(snapshots.len(), 2);
        assert!(
            snapshots
                .iter()
                .any(|snapshot| snapshot.baseline == "button-primary.desktop.png")
        );
        assert!(
            snapshots
                .iter()
                .any(|snapshot| snapshot.baseline == "button-primary.mobile.png")
        );
    }
}
