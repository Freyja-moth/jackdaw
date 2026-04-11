//! Animation authoring and playback for the Jackdaw editor.
//!
//! A thin UI layer over Bevy's built-in animation framework:
//! [`AnimationClip`], [`AnimationGraph`], and [`AnimationPlayer`]. The
//! authored data lives in the scene AST as reflected components
//! ([`Clip`], [`AnimTrack`], [`Vec3Keyframe`] / [`QuatKeyframe`] /
//! [`F32Keyframe`]) and compiles into real Bevy animation assets at
//! runtime. Jackdaw never writes its own curve evaluator — every
//! authored keyframe flows through Bevy's own playback path.
//!
//! See [`clip`] for a full table mapping each Jackdaw type to its
//! Bevy counterpart.
//!
//! ## AST vs runtime
//!
//! **Persisted** through JSN/BSN — clips live parented to the entity
//! they animate, which makes the target resolution structural rather
//! than a name lookup:
//!
//! - [`Clip`] with `duration` + Bevy's [`Name`] component + `ChildOf`
//!   pointing at the target entity
//! - [`AnimTrack`] with `(component_type_path, field_path,
//!   interpolation)` — the target is implicit (the clip's parent)
//! - [`Vec3Keyframe`] / [`QuatKeyframe`] / [`F32Keyframe`], one
//!   component type per value type (not per semantic role)
//!
//! **Runtime-only**, rebuilt from authored data each frame:
//!
//! - [`CompiledClip`] — `(Handle<AnimationClip>, Handle<AnimationGraph>, AnimationNodeIndex)`
//! - Bevy's own [`AnimationPlayer`], `AnimationGraphHandle`,
//!   `AnimationTargetId`, `AnimatedBy` — installed on the target
//!   entity by [`auto_bind_player`] while [`TimelineEngagement`] is
//!   `Active`, stripped on `Idle`, and also gated by a
//!   `bevy_animation::` skip prefix in the scene serializer as
//!   defense-in-depth
//!
//! **Resources** (UI state, never saved):
//!
//! - [`SelectedClip`], [`TimelineCursor`], [`ActiveAnimationTarget`],
//!   [`TimelineEngagement`], [`TimelineDirty`]
//!
//! ## Mutation path
//!
//! All authoring operations are plain AST edits via
//! `jackdaw::commands::{SpawnEntity, SetJsnField, DespawnEntity}` in
//! the main editor. The animation crate exports no custom
//! `EditorCommand` types — see [`commands`] for the rationale.
//!
//! [`AnimationClip`]: bevy::animation::AnimationClip
//! [`AnimationGraph`]: bevy::animation::graph::AnimationGraph
//! [`AnimationPlayer`]: bevy::animation::AnimationPlayer
//! [`Name`]: bevy::prelude::Name

use bevy::prelude::*;

pub mod clip;
pub mod commands;
pub mod compile;
pub mod player;
pub mod timeline;

pub use clip::{
    AnimTrack, Clip, F32Keyframe, Interpolation, QuatKeyframe, SelectedClip, SelectedKeyframes,
    TimelineSnap, TimelineSnapHint, Vec3Keyframe,
};
pub use compile::{CompiledClip, clip_display_duration, compile_clips, max_keyframe_time};
pub use player::{
    AnimationPause, AnimationPlay, AnimationSeek, AnimationStop, ActiveAnimationTarget, TimelineCursor,
    TimelineEngagement, auto_bind_player, handle_pause, handle_play, handle_seek, handle_stop,
    sync_cursor_from_player,
};
pub use timeline::{
    TimelineAddKeyframeButton, TimelineCreateClipButton, TimelineDirty, TimelineDurationInput,
    TimelineKeyframeHandle, TimelinePanelRoot, TrackField, clear_snap_hint_on_drag_end,
    handle_add_keyframe_click, handle_delete_selected_keyframes, handle_keyframe_click,
    handle_scrubber_click, handle_scrubber_drag, handle_scrubber_drag_end,
    handle_scrubber_drag_start, handle_transport_button_click,
    mark_timeline_dirty_on_data_change, rebuild_timeline, timeline_panel,
    update_keyframe_highlight, update_playhead_position,
};

/// Plugin that registers the animation authoring data model and wires
/// up the compile + playback systems. Add this to the editor app once,
/// after the Bevy default plugins and the JSN AST layer.
pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedClip>()
            .init_resource::<SelectedKeyframes>()
            .init_resource::<TimelineCursor>()
            .init_resource::<TimelineDirty>()
            .init_resource::<TimelineSnap>()
            .init_resource::<TimelineSnapHint>()
            .init_resource::<ActiveAnimationTarget>()
            .init_resource::<TimelineEngagement>()
            .add_message::<AnimationPlay>()
            .add_message::<AnimationPause>()
            .add_message::<AnimationStop>()
            .add_message::<AnimationSeek>()
            .register_type::<Clip>()
            .register_type::<AnimTrack>()
            .register_type::<Interpolation>()
            .register_type::<Vec3Keyframe>()
            .register_type::<QuatKeyframe>()
            .register_type::<F32Keyframe>()
            .add_observer(handle_transport_button_click)
            .add_observer(handle_add_keyframe_click)
            .add_observer(handle_scrubber_click)
            .add_observer(handle_scrubber_drag)
            .add_observer(handle_scrubber_drag_start)
            .add_observer(handle_scrubber_drag_end)
            .add_observer(clear_snap_hint_on_drag_end)
            .add_observer(handle_keyframe_click)
            .add_systems(PostUpdate, compile_clips)
            .add_systems(
                Update,
                (
                    auto_bind_player,
                    handle_play,
                    handle_pause,
                    handle_stop,
                    handle_seek,
                    sync_cursor_from_player,
                    handle_delete_selected_keyframes,
                    mark_timeline_dirty_on_data_change,
                    rebuild_timeline,
                    update_playhead_position,
                    update_keyframe_highlight,
                )
                    .chain(),
            );
    }
}
