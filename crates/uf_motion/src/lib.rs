#![deny(missing_docs)]
//! React Compiler-safe native motion contracts for `@uniflowed/motion`.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Inline motion track list.
pub type MotionTrackList = SmallVec<[MotionTrack; 8]>;

/// Native motion engine contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionContract {
    /// Native engine backing motion primitives.
    pub engine: MotionEngine,
    /// Motion tracks in this timeline.
    pub tracks: MotionTrackList,
    /// Whether generated hooks/components are safe for React Compiler syntax mode.
    pub compiler_safe: bool,
    /// Whether primitives can be rendered from Server Components.
    pub server_component_safe: bool,
    /// Whether reduced motion is respected by default.
    pub reduced_motion_default: bool,
}

impl Default for MotionContract {
    fn default() -> Self {
        Self {
            engine: MotionEngine::UfNative,
            tracks: SmallVec::new(),
            compiler_safe: true,
            server_component_safe: true,
            reduced_motion_default: true,
        }
    }
}

impl MotionContract {
    /// Add a motion track to the contract.
    pub fn track(mut self, track: MotionTrack) -> Self {
        self.tracks.push(track);
        self
    }
}

/// Motion engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionEngine {
    /// uf native engine.
    UfNative,
}

/// Animatable property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionProperty {
    /// CSS transform.
    Transform,
    /// Opacity.
    Opacity,
    /// Layout-independent color transition.
    Color,
}

/// Easing family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionEasing {
    /// Linear interpolation.
    Linear,
    /// Cubic ease out.
    Out,
    /// Spring solver.
    Spring,
}

/// One animation track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionTrack {
    /// Stable track id.
    pub id: CompactString,
    /// Property animated by the track.
    pub property: MotionProperty,
    /// Duration in milliseconds.
    pub duration_ms: u16,
    /// Easing family.
    pub easing: MotionEasing,
}

impl MotionTrack {
    /// Create a motion track.
    pub fn new(id: &str, property: MotionProperty, duration_ms: u16) -> Self {
        Self {
            id: id.to_compact_string(),
            property,
            duration_ms,
            easing: MotionEasing::Out,
        }
    }

    /// Select an easing family.
    pub fn easing(mut self, easing: MotionEasing) -> Self {
        self.easing = easing;
        self
    }
}

/// Return the default native motion contract.
pub fn contract() -> MotionContract {
    MotionContract::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_react_compiler_safe_native_motion() {
        let contract = contract();

        assert_eq!(contract.engine, MotionEngine::UfNative);
        assert!(contract.compiler_safe);
        assert!(contract.server_component_safe);
        assert!(contract.reduced_motion_default);
    }

    #[test]
    fn builds_motion_timeline_without_runtime_mutation() {
        let contract = MotionContract::default().track(
            MotionTrack::new("dialog-enter", MotionProperty::Opacity, 120)
                .easing(MotionEasing::Spring),
        );

        assert_eq!(contract.tracks.len(), 1);
        assert_eq!(contract.tracks[0].id, "dialog-enter");
        assert_eq!(contract.tracks[0].easing, MotionEasing::Spring);
    }
}
