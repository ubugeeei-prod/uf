use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type ViewportList = SmallVec<[Viewport; 8]>;
pub type BrowserSteps = SmallVec<[BrowserStep; 16]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPlan {
    pub engine: BrowserEngine,
    pub viewports: ViewportList,
    pub steps: BrowserSteps,
}

impl Default for BrowserPlan {
    fn default() -> Self {
        Self {
            engine: BrowserEngine::PlaywrightCompatible,
            viewports: smallvec::smallvec![Viewport::desktop(), Viewport::mobile()],
            steps: SmallVec::new(),
        }
    }
}

impl BrowserPlan {
    pub fn step(mut self, step: BrowserStep) -> Self {
        self.steps.push(step);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrowserEngine {
    PlaywrightCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub name: CompactString,
    pub width: u16,
    pub height: u16,
    pub device_scale_factor: u8,
}

impl Viewport {
    pub fn desktop() -> Self {
        Self {
            name: CompactString::const_new("desktop"),
            width: 1440,
            height: 900,
            device_scale_factor: 1,
        }
    }

    pub fn mobile() -> Self {
        Self {
            name: CompactString::const_new("mobile"),
            width: 390,
            height: 844,
            device_scale_factor: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserStep {
    Visit(CompactString),
    Click(CompactString),
    Fill {
        selector: CompactString,
        value: CompactString,
    },
    Screenshot {
        name: CompactString,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSnapshot {
    pub story_id: CompactString,
    pub viewport: CompactString,
    pub baseline: CompactString,
}

impl VisualSnapshot {
    pub fn new(story_id: impl Into<CompactString>, viewport: &Viewport) -> Self {
        let story_id = story_id.into();
        Self {
            baseline: CompactString::from(format!("{}.{}.png", story_id, viewport.name)),
            story_id,
            viewport: viewport.name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_playwright_compatible_viewports() {
        let plan = BrowserPlan::default();

        assert_eq!(plan.engine, BrowserEngine::PlaywrightCompatible);
        assert_eq!(plan.viewports.len(), 2);
        assert_eq!(plan.viewports[0].name, "desktop");
        assert_eq!(plan.viewports[1].name, "mobile");
    }

    #[test]
    fn creates_visual_snapshot_name() {
        let viewport = Viewport::desktop();
        let snapshot = VisualSnapshot::new("button-primary", &viewport);

        assert_eq!(snapshot.baseline, "button-primary.desktop.png");
    }
}
