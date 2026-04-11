//! Authored-clip data model. Every type here is a reflected component
//! that lives in the scene AST and round-trips through JSN/BSN
//! unchanged.
//!
//! ## Relationship to Bevy's animation API
//!
//! Jackdaw's types are the **authoring** representation. At compile
//! time they produce real Bevy assets and runtime components via the
//! [`compile_clips`] system, and from that point on playback goes
//! through Bevy's own [`AnimationPlayer`] evaluator. Nothing in this
//! crate interprets keyframes or samples curves — we build Bevy's
//! data structures and hand them off.
//!
//! | Jackdaw authoring type       | Bevy runtime type / function                                     | Where the bridge happens               |
//! |------------------------------|-------------------------------------------------------------------|-----------------------------------------|
//! | [`Clip`] (component)          | [`bevy_animation::AnimationClip`] (asset)                        | [`compile_clips`] in `compile.rs`       |
//! | [`AnimTrack`] (component)     | one [`AnimatableCurve`] per track, wrapped in the clip           | [`build_curve_for_track`] dispatch      |
//! | [`Vec3Keyframe`] / [`QuatKeyframe`] / [`F32Keyframe`] | `(f32, T)` samples fed to [`AnimatableKeyframeCurve::new`] | `collect_vec3_keyframes` / `collect_quat_keyframes` |
//! | `ChildOf(clip) → Clip.parent` | [`AnimationTargetId::from_name`] derived from the parent's `Name` | [`target_for_clip`] in `compile.rs`     |
//! | [`Interpolation::Linear`]     | [`AnimatableKeyframeCurve`] + `Animatable::interpolate`           | compile dispatch                         |
//! | [`Interpolation::Step`]       | future `StepCurve<T>` — scaffolded, not yet compiled              | compile dispatch (warns today)           |
//! | [`CompiledClip`] (runtime)    | `(Handle<AnimationClip>, Handle<AnimationGraph>, AnimationNodeIndex)` | `compile_clips` output                  |
//! | [`SelectedClip`] (resource)   | no Bevy analog — editor UI state                                  | manipulated by `follow_scene_selection_to_clip` |
//! | [`TimelineCursor`] (resource) | mirrors [`ActiveAnimation::seek_time`]                            | `sync_cursor_from_player`                |
//! | [`TimelineEngagement`] (resource) | no Bevy analog — gates whether the target is driven          | `auto_bind_player` reads it              |
//! | [`ActiveAnimationTarget`] (resource) | tracks which entity has Bevy's [`AnimationPlayer`] installed | `auto_bind_player` maintains it          |
//! | [`AnimationPlay`] / [`AnimationPause`] / [`AnimationStop`] / [`AnimationSeek`] messages | [`AnimationPlayer::play`] / [`ActiveAnimation::pause`] / [`AnimationPlayer::stop_all`] / [`ActiveAnimation::seek_to`] | transport observers in `player.rs`       |
//!
//! ## Design rules
//!
//! - **No wrappers over Bevy types.** Keyframe `value` fields are
//!   `Vec3` / `Quat` / `f32` directly — not `TranslationValue` or
//!   similar. Clip names use Bevy's [`Name`] component rather than a
//!   custom field.
//! - **Property addressing mirrors `SetJsnField`.** A track identifies
//!   the animated property via `(component_type_path, field_path)` —
//!   the same tuple the inspector and the AST mutation command use.
//!   One address space, shared by all editing surfaces.
//! - **Targets are structural, not named.** A clip lives as a child
//!   of the entity it animates. The compile step walks `ChildOf` up
//!   one level to read the parent's `Name` and hand it to
//!   [`AnimationTargetId::from_name`]. Renaming the target can't
//!   silently break a clip because the reference is an `Entity`, not
//!   a string.
//! - **All mutations go through `SpawnEntity` / `SetJsnField` /
//!   `DespawnEntity`.** The animation crate exports *no* custom
//!   commands. Creating a clip is a plain entity spawn; moving a
//!   keyframe in time is a `SetJsnField` on `Vec3Keyframe.time`.
//!
//! ## Hierarchy
//!
//! Authoring data lives under the entity it animates:
//!
//! ```text
//! (Door: Transform + Mesh + Name("Door"))
//!   ├── (Clip + Name("Door Open") + duration: 2.0)
//!   │     ├── (AnimTrack { component_type_path: "..Transform",
//!   │     │                 field_path: "translation",
//!   │     │                 interpolation: Linear })
//!   │     │     ├── (Vec3Keyframe { time: 0.0, value: [0,0,0] })
//!   │     │     └── (Vec3Keyframe { time: 2.0, value: [2,0,0] })
//!   │     └── (AnimTrack { ..., field_path: "rotation", ... })
//!   │           └── (QuatKeyframe { time: 1.0, value: ... })
//!   └── (Clip + Name("Door Close") + ... )
//! ```
//!
//! [`AnimatableCurve`]: bevy::animation::animation_curves::AnimatableCurve
//! [`AnimatableKeyframeCurve`]: bevy::animation::animation_curves::AnimatableKeyframeCurve
//! [`AnimatableKeyframeCurve::new`]: bevy::animation::animation_curves::AnimatableKeyframeCurve::new
//! [`ActiveAnimation`]: bevy::animation::ActiveAnimation
//! [`ActiveAnimation::pause`]: bevy::animation::ActiveAnimation::pause
//! [`ActiveAnimation::seek_to`]: bevy::animation::ActiveAnimation::seek_to
//! [`ActiveAnimation::seek_time`]: bevy::animation::ActiveAnimation::seek_time
//! [`AnimationPlayer`]: bevy::animation::AnimationPlayer
//! [`AnimationPlayer::play`]: bevy::animation::AnimationPlayer::play
//! [`AnimationPlayer::stop_all`]: bevy::animation::AnimationPlayer::stop_all
//! [`AnimationTargetId::from_name`]: bevy::animation::AnimationTargetId::from_name
//! [`bevy_animation::AnimationClip`]: bevy::animation::AnimationClip
//! [`Name`]: bevy::prelude::Name
//! [`ActiveAnimationTarget`]: crate::ActiveAnimationTarget
//! [`AnimationPause`]: crate::AnimationPause
//! [`AnimationPlay`]: crate::AnimationPlay
//! [`AnimationSeek`]: crate::AnimationSeek
//! [`AnimationStop`]: crate::AnimationStop
//! [`CompiledClip`]: crate::CompiledClip
//! [`SelectedClip`]: crate::SelectedClip
//! [`TimelineCursor`]: crate::TimelineCursor
//! [`TimelineEngagement`]: crate::TimelineEngagement
//! [`build_curve_for_track`]: crate::compile
//! [`compile_clips`]: crate::compile_clips
//! [`target_for_clip`]: crate::target_for_clip

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Top-level component on a clip entity.
///
/// `duration` is the clip's authored length in seconds — both the
/// visual range of the timeline widget and the playback duration
/// Bevy's `AnimationPlayer` honors (the compile step explicitly sets
/// `bevy_animation::AnimationClip.duration` to this value). Storing it
/// rather than deriving from `max(keyframe.time)` means the visual
/// range stays stable as you edit, so a new keyframe lands where you
/// clicked instead of always appearing at the right edge.
///
/// The clip's display name lives in Bevy's standard [`Name`] component;
/// tracks are child entities with [`AnimTrack`]; keyframes are in turn
/// child entities of their track.
///
/// [`Name`]: bevy::prelude::Name
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Copy)]
#[reflect(Component, Serialize, Deserialize)]
pub struct Clip {
    pub duration: f32,
}

impl Default for Clip {
    fn default() -> Self {
        Self { duration: 2.0 }
    }
}

/// Interpolation mode for an [`AnimTrack`]. `Linear` is what you want
/// for smooth Transform animation; `Step` is for discrete values like
/// booleans, enums, or "portal-jump" Vec3 positions that should snap
/// between keyframes rather than blend.
#[derive(
    Reflect, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, Hash,
)]
pub enum Interpolation {
    /// Linear blend between adjacent keyframes via Bevy's
    /// `Animatable::interpolate`. Works for `Vec3`, `Quat`, `f32`, and
    /// other types that implement `bevy::animation::Animatable`.
    #[default]
    Linear,
    /// Hold the most recent keyframe's value until the next one. Works
    /// for any `Reflect + FromReflect + Clone` value type, including
    /// bools, enums, and arbitrary structs. **Scaffolded in the type
    /// system but not yet implemented in the compile step — the
    /// compile step logs a warning and skips `Step` tracks for now.**
    Step,
}

/// A single track on a clip. Identifies what property the track
/// animates via the same `(component_type_path, field_path)` convention
/// the reflected-field inspector uses, so every surface in the editor
/// refers to the same property namespace.
///
/// The **target entity** is not stored on the track: a clip lives as a
/// child of the entity it animates, and the compile step walks up
/// `ChildOf` from the clip to read the target's `Name` and feed it to
/// `AnimationTargetId::from_name`. This ties authoring data to the
/// scene structure so renaming a target can't silently break a track
/// and so deleting a target cascades its animation data cleanly.
///
/// [`AnimationTargetId::from_name`]: bevy::animation::AnimationTargetId::from_name
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Default)]
#[reflect(Component, Serialize, Deserialize)]
pub struct AnimTrack {
    pub component_type_path: String,
    pub field_path: String,
    pub interpolation: Interpolation,
}

impl AnimTrack {
    /// Convenience constructor — most call sites want `Linear` interp.
    pub fn new(
        component_type_path: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            component_type_path: component_type_path.into(),
            field_path: field_path.into(),
            interpolation: Interpolation::Linear,
        }
    }

    /// Path pair used to dispatch in the compile step.
    pub fn property_path(&self) -> (&str, &str) {
        (&self.component_type_path, &self.field_path)
    }
}

// ─────────────────────────────────────────────────────────────────────
// Keyframe components, one per stored value type.
//
// These are named after the Bevy value type they hold, not after the
// field they target. `Vec3Keyframe` covers `Transform::translation`,
// `Transform::scale`, and any future Vec3-valued animated field.
// Adding a new value type (e.g. `BoolKeyframe` for step-interpolated
// booleans) is a new component here plus a dispatch arm in
// `compile.rs` — no schema churn elsewhere.
// ─────────────────────────────────────────────────────────────────────

/// A keyframe that stores a [`Vec3`] value. Used for translation,
/// scale, and future Vec3-valued animated fields.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Copy, Default)]
#[reflect(Component, Serialize, Deserialize)]
pub struct Vec3Keyframe {
    pub time: f32,
    pub value: Vec3,
}

/// A keyframe that stores a [`Quat`] value. Used for rotation.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Copy)]
#[reflect(Component, Serialize, Deserialize)]
pub struct QuatKeyframe {
    pub time: f32,
    pub value: Quat,
}

impl Default for QuatKeyframe {
    fn default() -> Self {
        Self {
            time: 0.0,
            value: Quat::IDENTITY,
        }
    }
}

/// A keyframe that stores an [`f32`] value. Used for light intensity,
/// weights, camera FOV, or any scalar animated field.
#[derive(Component, Reflect, Serialize, Deserialize, Debug, Clone, Copy, Default)]
#[reflect(Component, Serialize, Deserialize)]
pub struct F32Keyframe {
    pub time: f32,
    pub value: f32,
}

// ─────────────────────────────────────────────────────────────────────
// Future: AnimEventTrack + AnimEvent. A separate track kind that
// compiles into `bevy_animation::animation_event` registrations rather
// than a `Curve`. Out of scope for the initial clip-authoring phase;
// when gameplay code wants "fire an `EnableHitbox` event at t=0.3",
// add those two types and a parallel dispatch arm in compile.rs.
// ─────────────────────────────────────────────────────────────────────

/// Editor-state resource: which clip the timeline panel is currently
/// editing. `None` means the panel shows its create-clip placeholder.
/// Not persisted — rebuilt on editor open.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct SelectedClip(pub Option<Entity>);

/// Editor-state resource: which keyframe entities the user has
/// currently selected in the timeline. Clicks on keyframe diamonds
/// add or toggle entries here; the Delete key reads this set and
/// issues `DespawnEntity` commands. Not persisted.
#[derive(Resource, Default, Debug, Clone)]
pub struct SelectedKeyframes {
    pub entities: std::collections::HashSet<Entity>,
}

impl SelectedKeyframes {
    pub fn clear(&mut self) {
        self.entities.clear();
    }
    pub fn is_selected(&self, entity: Entity) -> bool {
        self.entities.contains(&entity)
    }
    pub fn toggle(&mut self, entity: Entity) {
        if !self.entities.insert(entity) {
            self.entities.remove(&entity);
        }
    }
    pub fn select_only(&mut self, entity: Entity) {
        self.entities.clear();
        self.entities.insert(entity);
    }
}

/// Snap behavior for the timeline scrubber. Holding Shift at scrub
/// time disables snapping temporarily, matching Jackdaw's existing
/// convention (see `src/snapping.rs` for the grid-snap equivalent).
/// The `threshold_ratio` is a fraction of the clip's visible range —
/// a raw time falling within `threshold_ratio * duration` of a snap
/// candidate gets pulled to that candidate.
#[derive(Resource, Debug, Clone, Copy)]
pub struct TimelineSnap {
    pub enabled: bool,
    pub snap_to_ticks: bool,
    pub snap_to_keyframes: bool,
    pub threshold_ratio: f32,
}

impl Default for TimelineSnap {
    fn default() -> Self {
        Self {
            enabled: true,
            snap_to_ticks: true,
            snap_to_keyframes: true,
            threshold_ratio: 0.015,
        }
    }
}

/// Short-lived feedback state: which keyframe the scrubber is
/// currently snapped onto during an active drag. The highlight
/// system reads this resource every frame and paints the target
/// diamond with a "hover" color, giving the user a visual cue that
/// their drag is going to land on an existing keyframe. Cleared on
/// drag-end.
///
/// `None` means either no drag in progress, or the drag isn't
/// snapped to a keyframe (snapped to a tick, or Shift-held, or out
/// of threshold).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct TimelineSnapHint {
    pub hovered_keyframe: Option<Entity>,
}
