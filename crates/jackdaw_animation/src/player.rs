//! Transport + target adoption. Watches the selected clip and wires up
//! the target entity with the runtime components Bevy needs —
//! [`AnimationPlayer`], [`AnimationGraphHandle`], [`AnimationTargetId`],
//! [`AnimatedBy`] — so Bevy samples the curve automatically. Play,
//! pause, seek, and stop then drive through Bevy's own API.
//!
//! None of the runtime components are persisted to the AST. They're
//! installed when a clip is bound to its target and stripped when the
//! selection changes; the scene serializer's skip prefix for
//! `bevy_animation::` is a defense-in-depth against any future registry
//! changes. The only authored data is the clip + tracks + keyframes
//! from `clip.rs`.
//!
//! [`AnimationPlayer`]: bevy::animation::AnimationPlayer
//! [`AnimationGraphHandle`]: bevy::animation::graph::AnimationGraphHandle
//! [`AnimationTargetId`]: bevy::animation::AnimationTargetId
//! [`AnimatedBy`]: bevy::animation::AnimatedBy

use bevy::animation::{AnimatedBy, AnimationPlayer, AnimationTargetId, graph::AnimationGraphHandle};
use bevy::prelude::*;

use crate::clip::{Clip, SelectedClip};
use crate::compile::{CompiledClip, clip_display_duration};

/// The (clip, target) pair whose runtime `AnimationPlayer` is
/// currently installed on the target entity.
///
/// Named after Bevy's [`ActiveAnimation`]: an "active animation target"
/// is the entity that's being driven by the editor right now. The
/// [`auto_bind_player`] system consults this resource to decide
/// whether to strip stale runtime components before installing new
/// ones. A resource rather than a component because there's only ever
/// one active target at a time in the Phase 5A single-entity model.
///
/// [`ActiveAnimation`]: bevy::animation::ActiveAnimation
/// [`auto_bind_player`]: crate::auto_bind_player
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ActiveAnimationTarget {
    pub clip: Option<Entity>,
    pub target: Option<Entity>,
}

/// Whether the user is currently driving the selected clip. The target
/// entity's runtime animation components (`AnimationPlayer`, etc.) are
/// only installed while engagement is `Active`. In `Idle` the target
/// is free to edit like any other scene entity — gizmo drag, inspector
/// fields, manual Transform edits all work normally.
///
/// Transitions:
/// - `Idle → Active`: scrubber drag start, or Play button pressed
/// - `Active → Idle`: scrubber drag end, or Pause/Stop pressed, or
///   selection changes (handled implicitly by re-binding)
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineEngagement {
    #[default]
    Idle,
    Active,
}

/// Shared timeline state for the editor's transport bar. The widget writes
/// into this resource; the transport systems read it and drive Bevy.
#[derive(Resource, Debug, Clone, Copy)]
pub struct TimelineCursor {
    /// Playhead time in seconds from the clip start.
    pub time: f32,
    /// True while the transport is actively playing. Set by `AnimationPlay`,
    /// cleared by `AnimationPause`/`AnimationStop`.
    pub is_playing: bool,
}

impl Default for TimelineCursor {
    fn default() -> Self {
        Self {
            time: 0.0,
            is_playing: false,
        }
    }
}

/// User pressed Play on the timeline. Ensures runtime components are
/// present on the target and starts playback from the current cursor.
#[derive(Message, Debug, Clone, Copy)]
pub struct AnimationPlay;

/// User pressed Pause. Leaves the active animation in place so Resume is
/// cheap.
#[derive(Message, Debug, Clone, Copy)]
pub struct AnimationPause;

/// User pressed Stop. Clears active animations and rewinds the cursor.
#[derive(Message, Debug, Clone, Copy)]
pub struct AnimationStop;

/// User dragged the playhead to a specific time. Updates the cursor and,
/// if an active animation exists, seeks it there.
#[derive(Message, Debug, Clone, Copy)]
pub struct AnimationSeek(pub f32);

/// Install or strip the runtime animation components on the clip's
/// target entity based on [`TimelineEngagement`]. Only installs while
/// engagement is `Active` (scrubbing or playing). In `Idle` the target
/// is stripped, leaving its Transform freely editable via gizmos or
/// the inspector — otherwise Bevy's `animate_targets` would clobber
/// every manual edit with the sampled curve value.
///
/// Re-binds eagerly when either the selection or the engagement
/// changes, so transitioning Idle → Active → Idle within a few frames
/// (the scrub-drag case) still works.
#[allow(clippy::too_many_arguments)]
pub fn auto_bind_player(
    selected: Res<SelectedClip>,
    engagement: Res<TimelineEngagement>,
    mut bound: ResMut<ActiveAnimationTarget>,
    mut cursor: ResMut<TimelineCursor>,
    compiled: Query<&CompiledClip>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    mut commands: Commands,
) {
    let want_bound = *engagement == TimelineEngagement::Active && selected.0.is_some();
    let currently_bound = bound.target.is_some() && bound.clip == selected.0;

    if want_bound == currently_bound && !want_bound {
        // Idle and already stripped — nothing to do.
        return;
    }
    if want_bound && currently_bound {
        // Already bound to the right clip. Nothing to do.
        return;
    }

    // Strip the previous bind (covers both "deactivating" and
    // "switching clips while active") so we can't leave stale
    // components behind.
    if let Some(old_target) = bound.target.take() {
        commands.queue(move |world: &mut World| {
            if let Ok(mut ent) = world.get_entity_mut(old_target) {
                ent.remove::<AnimationPlayer>();
                ent.remove::<AnimationGraphHandle>();
                ent.remove::<AnimationTargetId>();
                ent.remove::<AnimatedBy>();
            }
        });
    }
    bound.clip = None;

    if !want_bound {
        cursor.is_playing = false;
        return;
    }

    // From here on: engagement is Active and we need to install a
    // player on the clip's target (the clip entity's parent).
    let Some(clip_entity) = selected.0 else {
        return;
    };
    // Clip not compiled yet (compile runs in PostUpdate; we're in
    // Update). Retry next frame.
    let Ok(compiled) = compiled.get(clip_entity) else {
        return;
    };
    // The target entity is the clip's parent — we don't search by
    // name anymore since authoring data lives under its target.
    let Ok(clip_parent) = parents.get(clip_entity) else {
        return;
    };
    let target_entity = clip_parent.parent();
    let Ok(target_name) = names.get(target_entity) else {
        return;
    };
    let target_id = AnimationTargetId::from_name(target_name);
    let graph = compiled.graph.clone();
    let root_node = compiled.root_node;
    let seek_time = cursor.time;
    let start_playing = cursor.is_playing;

    commands.queue(move |world: &mut World| {
        // Build the player with an active animation seeded at the
        // current cursor. Bevy evaluates paused animations at their
        // `seek_time` without advancing time, so the scrub flow can
        // leave `paused = true` and still preview correctly. Play
        // inserts an already-running animation.
        let mut player = AnimationPlayer::default();
        {
            let active = player.play(root_node);
            active.seek_to(seek_time);
            if !start_playing {
                active.pause();
            }
        }
        world.entity_mut(target_entity).insert((
            player,
            AnimationGraphHandle(graph),
            target_id,
            AnimatedBy(target_entity),
        ));
    });

    bound.clip = Some(clip_entity);
    bound.target = Some(target_entity);
}

pub fn handle_play(
    mut events: MessageReader<AnimationPlay>,
    mut cursor: ResMut<TimelineCursor>,
    mut engagement: ResMut<TimelineEngagement>,
    bound: Res<ActiveAnimationTarget>,
    clips: Query<&CompiledClip>,
    mut players: Query<&mut AnimationPlayer>,
) {
    if events.read().count() == 0 {
        return;
    }
    cursor.is_playing = true;
    *engagement = TimelineEngagement::Active;

    // If we happen to already be bound (e.g. coming out of a pause),
    // resume the player in place. If we're Idle, auto_bind_player
    // will install a freshly-unpaused player on the next frame based
    // on `cursor.is_playing == true`.
    let (Some(clip_entity), Some(target_entity)) = (bound.clip, bound.target) else {
        return;
    };
    let Ok(compiled) = clips.get(clip_entity) else {
        return;
    };
    if let Ok(mut player) = players.get_mut(target_entity) {
        if player.animation_mut(compiled.root_node).is_none() {
            player.play(compiled.root_node);
        }
        if let Some(active) = player.animation_mut(compiled.root_node) {
            active.seek_to(cursor.time);
            active.resume();
        }
    }
}

pub fn handle_pause(
    mut events: MessageReader<AnimationPause>,
    mut cursor: ResMut<TimelineCursor>,
    bound: Res<ActiveAnimationTarget>,
    clips: Query<&CompiledClip>,
    mut players: Query<&mut AnimationPlayer>,
) {
    if events.read().count() == 0 {
        return;
    }
    cursor.is_playing = false;
    // Deliberately leave engagement alone: pausing keeps the target
    // bound so the user can see the frozen frame. Stop is the action
    // that releases the target.
    let (Some(clip_entity), Some(target_entity)) = (bound.clip, bound.target) else {
        return;
    };
    let Ok(compiled) = clips.get(clip_entity) else {
        return;
    };
    if let Ok(mut player) = players.get_mut(target_entity) {
        if let Some(active) = player.animation_mut(compiled.root_node) {
            active.pause();
        }
    }
}

pub fn handle_stop(
    mut events: MessageReader<AnimationStop>,
    mut cursor: ResMut<TimelineCursor>,
    mut engagement: ResMut<TimelineEngagement>,
) {
    if events.read().count() == 0 {
        return;
    }
    cursor.time = 0.0;
    cursor.is_playing = false;
    // Drop engagement to Idle — auto_bind_player will strip the
    // runtime components on the next frame, releasing the target so
    // the user can edit its Transform via gizmos again.
    *engagement = TimelineEngagement::Idle;
}

pub fn handle_seek(
    mut events: MessageReader<AnimationSeek>,
    mut cursor: ResMut<TimelineCursor>,
    bound: Res<ActiveAnimationTarget>,
    clips: Query<&CompiledClip>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Some(AnimationSeek(time)) = events.read().last().copied() else {
        return;
    };
    cursor.time = time;
    let (Some(clip_entity), Some(target_entity)) = (bound.clip, bound.target) else {
        return;
    };
    let Ok(compiled) = clips.get(clip_entity) else {
        return;
    };
    if let Ok(mut player) = players.get_mut(target_entity) {
        if let Some(active) = player.animation_mut(compiled.root_node) {
            active.seek_to(time);
        }
    }
}

/// While playing, mirror the Bevy animation's seek time back into the
/// cursor so the timeline widget draws an accurate playhead. The clip
/// duration is derived from the keyframe data at every call, not
/// stored as authored data.
pub fn sync_cursor_from_player(
    mut cursor: ResMut<TimelineCursor>,
    bound: Res<ActiveAnimationTarget>,
    compiled: Query<&CompiledClip>,
    clips: Query<(&Clip, Option<&Children>)>,
    players: Query<&AnimationPlayer>,
) {
    if !cursor.is_playing {
        return;
    }
    let (Some(clip_entity), Some(target_entity)) = (bound.clip, bound.target) else {
        return;
    };
    let Ok(compiled) = compiled.get(clip_entity) else {
        return;
    };
    let duration = clip_display_duration(clip_entity, &clips);
    if let Ok(player) = players.get(target_entity) {
        if let Some(active) = player.animation(compiled.root_node) {
            cursor.time = active.seek_time().clamp(0.0, duration);
        }
    }
}

