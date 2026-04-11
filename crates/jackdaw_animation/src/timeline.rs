//! Timeline UI for the authored-clip editor.
//!
//! The bundle function [`timeline_panel`] spawns the root node, then
//! [`rebuild_timeline`] repopulates its children whenever the selection
//! or a dirty flag says so. Layout follows Blender's model: left column
//! for track labels only, right column for a single pickable grid
//! (ruler + strips) that receives scrub clicks.
//!
//! The widget only reads and displays. All mutations go through the
//! main editor's existing `SpawnEntity` / `SetJsnField` / `DespawnEntity`
//! command primitives — see [`crate::commands`] for the rationale.

use bevy::prelude::*;
use bevy::ui::ComputedNode;
use bevy::ui::ui_transform::UiGlobalTransform;
use jackdaw_feathers::button::{
    ButtonClickEvent, ButtonProps, ButtonSize, ButtonVariant, IconButtonProps, button, icon_button,
};
use jackdaw_feathers::icons::IconFont;
use jackdaw_feathers::tokens;
use lucide_icons::Icon;

use crate::clip::{
    AnimTrack, Clip, F32Keyframe, QuatKeyframe, SelectedClip, SelectedKeyframes, TimelineSnap,
    TimelineSnapHint, Vec3Keyframe,
};
use crate::compile::clip_display_duration;
use crate::player::{
    AnimationPause, AnimationPlay, AnimationStop, TimelineCursor, TimelineEngagement,
};

// Row heights, picked so the left-column labels vertically align with
// the right-column strips and the top-of-column spacer aligns with the
// ruler.
const RULER_HEIGHT: f32 = 24.0;
const TRACK_ROW_HEIGHT: f32 = 24.0;
const TRACK_LABEL_COLUMN_WIDTH: f32 = 200.0;

/// Root marker placed on the panel container. Children are rebuilt by
/// [`rebuild_timeline`] whenever the selection or keyframes change.
#[derive(Component, Default)]
pub struct TimelinePanelRoot;

/// Marker on the clickable playhead scrubber region.
#[derive(Component)]
pub struct TimelineScrubber {
    pub clip: Entity,
}

/// Marker on the moving playhead indicator inside the scrubber.
#[derive(Component)]
pub struct TimelinePlayheadIndicator;

#[derive(Component, Clone, Copy)]
pub struct TimelinePlayButton;

#[derive(Component, Clone, Copy)]
pub struct TimelinePauseButton;

#[derive(Component, Clone, Copy)]
pub struct TimelineStopButton;

/// Marker for the placeholder "Create Clip for Selection" button shown
/// when no clip is selected. Clicking it asks the main editor's
/// `on_create_clip_for_selection` observer to `SpawnEntity` a new clip.
#[derive(Component, Clone, Copy)]
pub struct TimelineCreateClipButton;

/// Marker for the "add keyframe" button on a track row. Carries the
/// track entity so the observer can read the track's target + field
/// path and spawn the right keyframe type.
#[derive(Component, Clone, Copy)]
pub struct TimelineAddKeyframeButton {
    pub track: Entity,
}

/// Marker on a rendered keyframe diamond inside a track strip. Links
/// the visual node back to the authoring keyframe entity so clicks
/// and highlight updates can address it.
#[derive(Component, Clone, Copy)]
pub struct TimelineKeyframeHandle {
    pub keyframe: Entity,
}

/// **Deprecated alias kept temporarily for consumers migrating to the
/// path-addressed track model.** Will be removed in Phase 5B once no
/// code outside the animation crate references it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackField {
    Translation,
    Rotation,
    Scale,
}

/// Marker on the duration text field in the header — reserved for
/// future phases when explicit duration storage comes back (e.g. for
/// trimming clips). Phase 5A derives duration from keyframes, so this
/// marker currently goes unused but is kept to preserve the public API
/// that the main editor observer binds to.
#[derive(Component, Clone, Copy)]
pub struct TimelineDurationInput {
    pub clip: Entity,
}

/// Bump this to force a timeline rebuild on the next frame. Commands
/// that mutate clip/track/keyframe entities set this flag.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct TimelineDirty(pub bool);

/// Bundle for the panel root. Spawn this wherever you want the timeline
/// to live (currently: inside `AnimationCenter`).
pub fn timeline_panel() -> impl Bundle {
    (
        TimelinePanelRoot,
        Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(tokens::PANEL_BG),
    )
}

/// Flag the timeline as dirty whenever any authored animation data
/// changes this frame. Runs before [`rebuild_timeline`] so
/// inspector-driven edits (e.g. typing a new `Clip.duration` in the
/// component inspector, or moving a `Vec3Keyframe.time` via
/// `SetJsnField`) repaint the widget immediately instead of waiting
/// for the user to deselect and reselect the entity.
///
/// The query set matches the one `compile_clips` watches, so the
/// visual rebuild and the Bevy-asset rebuild stay in lockstep — if
/// the compile step had something to recompile this frame, the
/// timeline widget will redraw on the same frame.
pub fn mark_timeline_dirty_on_data_change(
    mut dirty: ResMut<TimelineDirty>,
    changed_clips: Query<(), Changed<Clip>>,
    changed_tracks: Query<(), Changed<AnimTrack>>,
    changed_vec3: Query<(), Changed<Vec3Keyframe>>,
    changed_quat: Query<(), Changed<QuatKeyframe>>,
    changed_f32: Query<(), Changed<F32Keyframe>>,
) {
    if !changed_clips.is_empty()
        || !changed_tracks.is_empty()
        || !changed_vec3.is_empty()
        || !changed_quat.is_empty()
        || !changed_f32.is_empty()
    {
        dirty.0 = true;
    }
}

/// Repopulates the timeline panel whenever the selection or any
/// timeline data changed. Cheap — the widget is small and redrawn at
/// 60 Hz at most.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_timeline(
    mut commands: Commands,
    selected: Res<SelectedClip>,
    cursor: Res<TimelineCursor>,
    mut dirty: ResMut<TimelineDirty>,
    panels: Query<(Entity, Option<&Children>), With<TimelinePanelRoot>>,
    clips: Query<(&Clip, Option<&Children>)>,
    tracks: Query<(&AnimTrack, Option<&Children>)>,
    vec3_keyframes: Query<&Vec3Keyframe>,
    quat_keyframes: Query<&QuatKeyframe>,
    f32_keyframes: Query<&F32Keyframe>,
    names: Query<&Name>,
    icon_font: Option<Res<IconFont>>,
    mut last_built_for: Local<Option<Entity>>,
) {
    let Some(icon_font) = icon_font else {
        return;
    };

    let selection_changed = *last_built_for != selected.0;

    for (panel_entity, panel_children) in &panels {
        // Rebuild whenever: selection changed, explicit dirty flag set,
        // or the panel is empty (just spawned). The empty check is
        // load-bearing because `SelectedClip` initializes during plugin
        // build long before the panel exists.
        let panel_is_empty = panel_children.map(|c| c.is_empty()).unwrap_or(true);
        let needs_rebuild = selection_changed || dirty.0 || panel_is_empty;
        if !needs_rebuild {
            continue;
        }

        if let Some(children) = panel_children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }

        match selected.0.and_then(|e| clips.get(e).ok().map(|c| (e, c))) {
            None => {
                spawn_placeholder(&mut commands, panel_entity);
            }
            Some((clip_entity, (_clip, clip_children))) => {
                let clip_name = names
                    .get(clip_entity)
                    .map(|n| n.as_str().to_string())
                    .unwrap_or_else(|_| "Untitled Clip".to_string());
                let duration = clip_display_duration(clip_entity, &clips);
                spawn_header(
                    &mut commands,
                    panel_entity,
                    clip_entity,
                    &clip_name,
                    cursor.time,
                    duration,
                    &icon_font,
                );
                spawn_body(
                    &mut commands,
                    panel_entity,
                    clip_entity,
                    duration,
                    clip_children,
                    &tracks,
                    &vec3_keyframes,
                    &quat_keyframes,
                    &f32_keyframes,
                );
            }
        }
    }

    *last_built_for = selected.0;
    dirty.0 = false;
}

fn spawn_placeholder(commands: &mut Commands, parent: Entity) {
    let wrapper = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(tokens::SPACING_MD),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();

    commands.spawn((
        Text::new("No animation clip on selection. Pick a named entity and create one."),
        TextColor(tokens::TEXT_MUTED_COLOR.into()),
        TextFont {
            font_size: tokens::FONT_SM,
            ..default()
        },
        ChildOf(wrapper),
    ));
    commands.spawn((
        TimelineCreateClipButton,
        button(
            ButtonProps::new("Create Clip")
                .with_variant(ButtonVariant::Default)
                .with_left_icon(Icon::Plus),
        ),
        ChildOf(wrapper),
    ));
}

/// Header bar row: transport buttons on the left, clip name centered,
/// cursor readout and editable duration on the right. The duration
/// field routes through `SetJsnField` via the main editor's
/// `on_duration_input_commit` observer, so edits here flow through
/// the AST and participate in undo/redo + save/load.
fn spawn_header(
    commands: &mut Commands,
    parent: Entity,
    clip_entity: Entity,
    clip_name: &str,
    cursor_time: f32,
    duration: f32,
    icon_font: &IconFont,
) {
    let header = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(tokens::SPACING_SM),
                width: Val::Percent(100.0),
                height: Val::Px(32.0),
                padding: UiRect::axes(Val::Px(tokens::SPACING_SM), Val::Px(tokens::SPACING_XS)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(tokens::PANEL_HEADER_BG),
            BorderColor::all(tokens::BORDER_SUBTLE),
            ChildOf(parent),
        ))
        .id();

    commands.spawn((
        TimelinePlayButton,
        icon_button(IconButtonProps::new(Icon::Play), &icon_font.0),
        ChildOf(header),
    ));
    commands.spawn((
        TimelinePauseButton,
        icon_button(IconButtonProps::new(Icon::Pause), &icon_font.0),
        ChildOf(header),
    ));
    commands.spawn((
        TimelineStopButton,
        icon_button(IconButtonProps::new(Icon::Square), &icon_font.0),
        ChildOf(header),
    ));

    commands.spawn((
        Text::new(clip_name.to_string()),
        TextColor(tokens::TEXT_PRIMARY),
        TextFont {
            font_size: tokens::FONT_MD,
            ..default()
        },
        Node {
            flex_grow: 1.0,
            margin: UiRect::horizontal(Val::Px(tokens::SPACING_MD)),
            ..default()
        },
        ChildOf(header),
    ));

    // Cursor time readout (read-only).
    commands.spawn((
        Text::new(format!("{cursor_time:.2}s")),
        TextColor(tokens::TEXT_SECONDARY),
        TextFont {
            font_size: tokens::FONT_SM,
            ..default()
        },
        Node {
            margin: UiRect::right(Val::Px(tokens::SPACING_SM)),
            ..default()
        },
        ChildOf(header),
    ));

    // Editable duration. Two-entity structure so we can set a fixed
    // wrapper width without conflicting with the `Node` that
    // `text_edit()` provides internally — same pattern as the
    // animation keyframe diamond in the inspector. The
    // `TimelineDurationInput` marker sits on the wrapper; the main
    // editor's `on_duration_input_commit` observer walks up
    // `ChildOf` from the inner input entity to find it.
    let duration_wrapper = commands
        .spawn((
            TimelineDurationInput { clip: clip_entity },
            Node {
                width: Val::Px(96.0),
                ..default()
            },
            ChildOf(header),
        ))
        .id();
    commands.spawn((
        jackdaw_feathers::text_edit::text_edit(
            jackdaw_feathers::text_edit::TextEditProps::default()
                .numeric_f32()
                .with_suffix("s")
                .with_min(0.01)
                .with_max(3600.0)
                .with_default_value(format!("{duration:.2}")),
        ),
        ChildOf(duration_wrapper),
    ));
}

/// Body row: fixed-width label column on the left, flex-grow timeline
/// grid on the right. The timeline grid is one big pickable surface
/// that receives scrub clicks.
#[allow(clippy::too_many_arguments)]
fn spawn_body(
    commands: &mut Commands,
    parent: Entity,
    clip_entity: Entity,
    duration: f32,
    clip_children: Option<&Children>,
    tracks: &Query<(&AnimTrack, Option<&Children>)>,
    vec3_keyframes: &Query<&Vec3Keyframe>,
    quat_keyframes: &Query<&QuatKeyframe>,
    f32_keyframes: &Query<&F32Keyframe>,
) {
    let body = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();

    let left_col = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: Val::Px(TRACK_LABEL_COLUMN_WIDTH),
                flex_shrink: 0.0,
                border: UiRect::right(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(tokens::PANEL_HEADER_BG),
            BorderColor::all(tokens::BORDER_SUBTLE),
            ChildOf(body),
        ))
        .id();

    // Spacer at the top of the left column matching the ruler.
    commands.spawn((
        Node {
            height: Val::Px(RULER_HEIGHT),
            width: Val::Percent(100.0),
            border: UiRect::bottom(Val::Px(1.0)),
            ..default()
        },
        BorderColor::all(tokens::BORDER_SUBTLE),
        ChildOf(left_col),
    ));

    let timeline_col = commands
        .spawn((
            TimelineScrubber { clip: clip_entity },
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                position_type: PositionType::Relative,
                ..default()
            },
            BackgroundColor(tokens::PANEL_BG),
            Pickable::default(),
            ChildOf(body),
        ))
        .id();

    // Ruler strip along the top of the timeline column, with time
    // labels at nice round intervals so you can tell where the
    // playhead is without guessing.
    let ruler = commands
        .spawn((
            Node {
                height: Val::Px(RULER_HEIGHT),
                width: Val::Percent(100.0),
                position_type: PositionType::Relative,
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(tokens::PANEL_HEADER_BG),
            BorderColor::all(tokens::BORDER_SUBTLE),
            Pickable::IGNORE,
            ChildOf(timeline_col),
        ))
        .id();
    spawn_ruler_ticks(commands, ruler, timeline_col, duration);

    // For each track: spawn a label row in the left column and a
    // strip row in the timeline column at matching height.
    if let Some(clip_children) = clip_children {
        for track_entity in clip_children.iter() {
            let Ok((track, track_children)) = tracks.get(track_entity) else {
                continue;
            };
            let keyframes = collect_keyframes(
                track_children,
                vec3_keyframes,
                quat_keyframes,
                f32_keyframes,
            );
            spawn_track_label(commands, left_col, track_entity, track);
            spawn_track_strip(commands, timeline_col, duration, keyframes);
        }
    }

    // Playhead overlay: absolutely-positioned vertical line spanning
    // the full height of the timeline column. `left` is updated each
    // frame by [`update_playhead_position`].
    commands.spawn((
        TimelinePlayheadIndicator,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(0.0),
            top: Val::Px(0.0),
            width: Val::Px(2.0),
            height: Val::Percent(100.0),
            margin: UiRect::left(Val::Px(-1.0)),
            ..default()
        },
        BackgroundColor(tokens::ACCENT_BLUE),
        Pickable::IGNORE,
        ChildOf(timeline_col),
    ));
}

/// Spawn time labels + vertical gridlines at a "nice" interval so the
/// user can see where they are in the clip at a glance. The interval
/// is picked so there are roughly 4–10 ticks across the visible range.
fn spawn_ruler_ticks(
    commands: &mut Commands,
    ruler: Entity,
    timeline_col: Entity,
    duration: f32,
) {
    if duration <= 0.0 {
        return;
    }
    let step = pick_tick_step(duration);
    let mut t = 0.0_f32;
    while t <= duration + f32::EPSILON {
        let percent = (t / duration).clamp(0.0, 1.0) * 100.0;

        // Time label in the ruler. Absolutely positioned so we can
        // place it by percentage independent of flex flow.
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(percent),
                top: Val::Px(0.0),
                height: Val::Percent(100.0),
                margin: UiRect::left(Val::Px(2.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(ruler),
            children![(
                Text::new(format!("{t:.2}s")),
                TextColor(tokens::TEXT_MUTED_COLOR.into()),
                TextFont {
                    font_size: tokens::FONT_SM,
                    ..default()
                },
            )],
        ));

        // Faint vertical gridline extending from the ruler down
        // through the track strips. Skips t=0 and t=duration because
        // those sit on the column border.
        if t > 0.0 && (duration - t).abs() > f32::EPSILON {
            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(percent),
                    top: Val::Px(0.0),
                    width: Val::Px(1.0),
                    height: Val::Percent(100.0),
                    margin: UiRect::left(Val::Px(-0.5)),
                    ..default()
                },
                BackgroundColor(Color::WHITE.with_alpha(0.05)),
                Pickable::IGNORE,
                ChildOf(timeline_col),
            ));
        }

        t += step;
    }
}

/// Pick a "nice" tick interval for the given duration. Aims for
/// between 4 and 10 labels across the visible range.
fn pick_tick_step(duration: f32) -> f32 {
    const CANDIDATES: &[f32] = &[
        0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0,
    ];
    for &step in CANDIDATES {
        if duration / step <= 10.0 {
            return step;
        }
    }
    *CANDIDATES.last().unwrap()
}

/// Pull `(entity, time)` pairs from all three typed keyframe queries
/// in one pass. At most one query will match any given child entity.
/// Returning the entity lets the diamond spawner attach a
/// [`TimelineKeyframeHandle`] so selection and delete can address
/// the authoring keyframe by its stable entity id.
fn collect_keyframes(
    children: Option<&Children>,
    vec3_keyframes: &Query<&Vec3Keyframe>,
    quat_keyframes: &Query<&QuatKeyframe>,
    f32_keyframes: &Query<&F32Keyframe>,
) -> Vec<(Entity, f32)> {
    let Some(children) = children else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|c| {
            if let Ok(k) = vec3_keyframes.get(c) {
                Some((c, k.time))
            } else if let Ok(k) = quat_keyframes.get(c) {
                Some((c, k.time))
            } else if let Ok(k) = f32_keyframes.get(c) {
                Some((c, k.time))
            } else {
                None
            }
        })
        .collect()
}

/// Spawn a single label row in the left column. Shows the track's
/// target name and field path, plus a small `+` button that snapshots
/// the target entity at the current cursor time.
fn spawn_track_label(
    commands: &mut Commands,
    parent: Entity,
    track_entity: Entity,
    track: &AnimTrack,
) {
    let row = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(tokens::SPACING_SM),
                width: Val::Percent(100.0),
                height: Val::Px(TRACK_ROW_HEIGHT),
                padding: UiRect::horizontal(Val::Px(tokens::SPACING_SM)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(tokens::BORDER_SUBTLE),
            ChildOf(parent),
        ))
        .id();

    // The track's label is just the property path — the target is
    // implied by the clip's parent in the scene tree.
    commands.spawn((
        Text::new(track.field_path.clone()),
        TextColor(tokens::TEXT_TERTIARY),
        TextFont {
            font_size: tokens::FONT_SM,
            ..default()
        },
        Node {
            flex_grow: 1.0,
            ..default()
        },
        ChildOf(row),
    ));

    commands.spawn((
        TimelineAddKeyframeButton {
            track: track_entity,
        },
        icon_button_small(Icon::Plus),
        ChildOf(row),
    ));
}

/// Spawn a strip row in the timeline column at the same height as the
/// matching label row. Keyframes are rendered as diamonds positioned
/// proportionally to the clip duration. Each diamond carries a
/// [`TimelineKeyframeHandle`] so click observers and the highlight
/// system can address the underlying authoring keyframe entity.
fn spawn_track_strip(
    commands: &mut Commands,
    parent: Entity,
    duration: f32,
    keyframes: Vec<(Entity, f32)>,
) {
    let strip = commands
        .spawn((
            Node {
                position_type: PositionType::Relative,
                width: Val::Percent(100.0),
                height: Val::Px(TRACK_ROW_HEIGHT),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(tokens::BORDER_SUBTLE),
            Pickable::IGNORE,
            ChildOf(parent),
        ))
        .id();

    for (keyframe_entity, t) in keyframes {
        let percent = if duration > 0.0 {
            (t / duration).clamp(0.0, 1.0) * 100.0
        } else {
            0.0
        };
        commands.spawn((
            TimelineKeyframeHandle {
                keyframe: keyframe_entity,
            },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(percent),
                top: Val::Px((TRACK_ROW_HEIGHT - 10.0) * 0.5),
                width: Val::Px(10.0),
                height: Val::Px(10.0),
                margin: UiRect::left(Val::Px(-5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(tokens::ACCENT_BLUE),
            BorderColor::all(Color::WHITE.with_alpha(0.4)),
            Pickable::default(),
            ChildOf(strip),
        ));
    }
}

fn icon_button_small(icon: Icon) -> impl Bundle {
    button(
        ButtonProps::new("")
            .with_variant(ButtonVariant::Ghost)
            .with_size(ButtonSize::IconSM)
            .with_left_icon(icon),
    )
}

/// Keep the playhead indicator's `left` in sync with [`TimelineCursor`].
pub fn update_playhead_position(
    cursor: Res<TimelineCursor>,
    scrubbers: Query<(&TimelineScrubber, &Children)>,
    clips: Query<(&Clip, Option<&Children>)>,
    mut indicators: Query<&mut Node, With<TimelinePlayheadIndicator>>,
) {
    for (scrubber, children) in &scrubbers {
        let duration = clip_display_duration(scrubber.clip, &clips);
        let percent = if duration > 0.0 {
            (cursor.time / duration).clamp(0.0, 1.0) * 100.0
        } else {
            0.0
        };
        for child in children.iter() {
            if let Ok(mut node) = indicators.get_mut(child) {
                node.left = Val::Percent(percent);
            }
        }
    }
}

/// Route transport button clicks into the animation message channel.
pub fn handle_transport_button_click(
    event: On<ButtonClickEvent>,
    play: Query<(), With<TimelinePlayButton>>,
    pause: Query<(), With<TimelinePauseButton>>,
    stop: Query<(), With<TimelineStopButton>>,
    mut play_writer: MessageWriter<AnimationPlay>,
    mut pause_writer: MessageWriter<AnimationPause>,
    mut stop_writer: MessageWriter<AnimationStop>,
) {
    if play.contains(event.entity) {
        play_writer.write(AnimationPlay);
    } else if pause.contains(event.entity) {
        pause_writer.write(AnimationPause);
    } else if stop.contains(event.entity) {
        stop_writer.write(AnimationStop);
    }
}

/// Observer for clicks on the per-track `+` button. Reads the track's
/// `AnimTrack.property_path()`, looks up the target entity by name,
/// snapshots the current value of the target's animated field, and
/// `SpawnEntity`-s the right typed keyframe component at the cursor
/// time. This is the one place in the widget layer that bridges
/// "which field does this track animate" (strings in the AST) to
/// "which keyframe component type to spawn" (a concrete Rust type) —
/// mirroring the dispatch in `compile.rs`.
pub fn handle_add_keyframe_click(
    event: On<ButtonClickEvent>,
    buttons: Query<&TimelineAddKeyframeButton>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(event.entity) else {
        return;
    };
    let track_entity = button.track;

    commands.queue(move |world: &mut World| {
        let cursor_time = world
            .get_resource::<TimelineCursor>()
            .map(|c| c.time)
            .unwrap_or(0.0);

        let Some(track) = world.get::<AnimTrack>(track_entity).cloned() else {
            return;
        };

        // Walk up from the track to the owning clip, then from the clip
        // to its parent — the parent entity is the animation target.
        // The target is always the clip's parent in the new parenting
        // model; no name lookup needed.
        let Some(clip_entity) = world.get::<ChildOf>(track_entity).map(|c| c.parent()) else {
            warn!("Add keyframe: track has no parent clip");
            return;
        };
        let Some(target_entity) = world.get::<ChildOf>(clip_entity).map(|c| c.parent()) else {
            warn!("Add keyframe: clip has no parent target entity");
            return;
        };

        // Snapshot the target's current field value and spawn the
        // right typed keyframe component as a child of the track.
        let Some(transform) = world.get::<Transform>(target_entity).copied() else {
            warn!("Add keyframe: target has no Transform component");
            return;
        };
        match (track.component_type_path.as_str(), track.field_path.as_str()) {
            (
                "bevy_transform::components::transform::Transform",
                "translation",
            ) => {
                world.spawn((
                    Vec3Keyframe {
                        time: cursor_time,
                        value: transform.translation,
                    },
                    ChildOf(track_entity),
                ));
            }
            (
                "bevy_transform::components::transform::Transform",
                "rotation",
            ) => {
                world.spawn((
                    QuatKeyframe {
                        time: cursor_time,
                        value: transform.rotation,
                    },
                    ChildOf(track_entity),
                ));
            }
            (
                "bevy_transform::components::transform::Transform",
                "scale",
            ) => {
                world.spawn((
                    Vec3Keyframe {
                        time: cursor_time,
                        value: transform.scale,
                    },
                    ChildOf(track_entity),
                ));
            }
            (component, field) => {
                warn!(
                    "Add keyframe: no snapshot dispatch for {component}.{field} — \
                     add one in handle_add_keyframe_click"
                );
            }
        }

        // Auto-extend the clip's authored duration so the new keyframe
        // is visible on the timeline. Without this, dropping a
        // keyframe at t=5 on a clip with duration=2 would spawn the
        // keyframe correctly but leave it outside the visual range.
        if let Some(mut clip) = world.get_mut::<Clip>(clip_entity) {
            if cursor_time > clip.duration {
                clip.duration = cursor_time;
            }
        }

        // Ensure the timeline repaints to show the new diamond.
        if let Some(mut dirty) = world.get_resource_mut::<TimelineDirty>() {
            dirty.0 = true;
        }
    });
}

/// Observer: clicking on the scrubber bar seeks the playhead to the
/// corresponding time, snapping to the nearest tick or keyframe
/// unless Shift is held.
#[allow(clippy::too_many_arguments)]
pub fn handle_scrubber_click(
    mut event: On<Pointer<Click>>,
    scrubbers: Query<(&TimelineScrubber, &ComputedNode, &UiGlobalTransform)>,
    clips: Query<(&Clip, Option<&Children>)>,
    tracks: Query<(&AnimTrack, Option<&Children>)>,
    vec3_keyframes: Query<&Vec3Keyframe>,
    quat_keyframes: Query<&QuatKeyframe>,
    f32_keyframes: Query<&F32Keyframe>,
    snap: Res<TimelineSnap>,
    mut hint: ResMut<TimelineSnapHint>,
    keys: Res<ButtonInput<KeyCode>>,
    mut seek: MessageWriter<crate::player::AnimationSeek>,
) {
    let Ok((scrubber, computed, global_tf)) = scrubbers.get(event.event_target()) else {
        return;
    };
    let duration = clip_display_duration(scrubber.clip, &clips);
    let raw_time = scrubber_time_for_cursor(
        event.pointer_location.position.x,
        computed,
        global_tf,
        duration,
    );
    let result = resolve_snap(
        raw_time,
        duration,
        scrubber.clip,
        &snap,
        &keys,
        &clips,
        &tracks,
        &vec3_keyframes,
        &quat_keyframes,
        &f32_keyframes,
    );
    hint.hovered_keyframe = result.hovered_keyframe;
    seek.write(crate::player::AnimationSeek(result.time));
    event.propagate(false);
}

/// Observer: dragging across the scrubber emits a stream of
/// [`AnimationSeek`] events so the target entity visibly follows the
/// playhead in real time. Snaps to ticks and keyframes unless Shift
/// is held.
#[allow(clippy::too_many_arguments)]
pub fn handle_scrubber_drag(
    mut event: On<Pointer<Drag>>,
    scrubbers: Query<(&TimelineScrubber, &ComputedNode, &UiGlobalTransform)>,
    clips: Query<(&Clip, Option<&Children>)>,
    tracks: Query<(&AnimTrack, Option<&Children>)>,
    vec3_keyframes: Query<&Vec3Keyframe>,
    quat_keyframes: Query<&QuatKeyframe>,
    f32_keyframes: Query<&F32Keyframe>,
    snap: Res<TimelineSnap>,
    mut hint: ResMut<TimelineSnapHint>,
    keys: Res<ButtonInput<KeyCode>>,
    mut seek: MessageWriter<crate::player::AnimationSeek>,
) {
    let Ok((scrubber, computed, global_tf)) = scrubbers.get(event.event_target()) else {
        return;
    };
    let duration = clip_display_duration(scrubber.clip, &clips);
    let raw_time = scrubber_time_for_cursor(
        event.pointer_location.position.x,
        computed,
        global_tf,
        duration,
    );
    let result = resolve_snap(
        raw_time,
        duration,
        scrubber.clip,
        &snap,
        &keys,
        &clips,
        &tracks,
        &vec3_keyframes,
        &quat_keyframes,
        &f32_keyframes,
    );
    hint.hovered_keyframe = result.hovered_keyframe;
    seek.write(crate::player::AnimationSeek(result.time));
    event.propagate(false);
}

/// Glue between the scrubber observers and [`apply_snap`]: resolves
/// the clip's keyframe set and honors the Shift modifier. Uses the
/// idiomatic `any_pressed([ShiftLeft, ShiftRight])` pattern matching
/// the rest of the editor's input handling (`src/snapping.rs` etc.).
#[allow(clippy::too_many_arguments)]
fn resolve_snap(
    raw_time: f32,
    duration: f32,
    clip_entity: Entity,
    snap: &TimelineSnap,
    keys: &ButtonInput<KeyCode>,
    clips: &Query<(&Clip, Option<&Children>)>,
    tracks: &Query<(&AnimTrack, Option<&Children>)>,
    vec3_keyframes: &Query<&Vec3Keyframe>,
    quat_keyframes: &Query<&QuatKeyframe>,
    f32_keyframes: &Query<&F32Keyframe>,
) -> SnapResult {
    // Shift temporarily disables snapping for precise positioning —
    // matches Jackdaw's convention for grid-snap and viewport
    // operations elsewhere in the editor.
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if shift || !snap.enabled {
        return SnapResult {
            time: raw_time,
            hovered_keyframe: None,
        };
    }
    let keyframes = all_keyframes_for_clip(
        clip_entity,
        clips,
        tracks,
        vec3_keyframes,
        quat_keyframes,
        f32_keyframes,
    );
    apply_snap(raw_time, duration, snap, &keyframes)
}

/// Observer: scrubber drag ends — clear the snap hint so hover
/// highlights don't linger after the user releases the mouse.
pub fn clear_snap_hint_on_drag_end(
    mut event: On<Pointer<DragEnd>>,
    scrubbers: Query<&TimelineScrubber>,
    mut hint: ResMut<TimelineSnapHint>,
) {
    if scrubbers.get(event.event_target()).is_err() {
        return;
    }
    hint.hovered_keyframe = None;
    event.propagate(false);
}

/// Observer: scrubber drag begins — mark the timeline as actively
/// engaged so [`crate::auto_bind_player`] installs the runtime
/// components on the next frame. Without this, the target Transform
/// stays free to edit even while the user is scrubbing.
pub fn handle_scrubber_drag_start(
    mut event: On<Pointer<DragStart>>,
    scrubbers: Query<&TimelineScrubber>,
    mut engagement: ResMut<TimelineEngagement>,
) {
    if scrubbers.get(event.event_target()).is_err() {
        return;
    }
    *engagement = TimelineEngagement::Active;
    event.propagate(false);
}

/// Observer: scrubber drag ends — release the target by transitioning
/// to idle. [`crate::auto_bind_player`] will strip the runtime
/// components on the next frame.
pub fn handle_scrubber_drag_end(
    mut event: On<Pointer<DragEnd>>,
    scrubbers: Query<&TimelineScrubber>,
    mut engagement: ResMut<TimelineEngagement>,
) {
    if scrubbers.get(event.event_target()).is_err() {
        return;
    }
    *engagement = TimelineEngagement::Idle;
    event.propagate(false);
}

fn scrubber_time_for_cursor(
    logical_cursor_x: f32,
    computed: &ComputedNode,
    global_tf: &UiGlobalTransform,
    duration: f32,
) -> f32 {
    let (_, _, physical_center) = global_tf.to_scale_angle_translation();
    let inv_scale = computed.inverse_scale_factor();
    let center = physical_center * inv_scale;
    let size = computed.size() * inv_scale;
    let left = center.x - size.x * 0.5;
    let ratio = ((logical_cursor_x - left) / size.x.max(1.0)).clamp(0.0, 1.0);
    ratio * duration
}

/// The result of a snap attempt: the final time, plus the keyframe
/// entity that was snapped onto (if any). Callers use the entity to
/// set [`TimelineSnapHint`] for visual feedback.
#[derive(Debug, Clone, Copy)]
struct SnapResult {
    time: f32,
    hovered_keyframe: Option<Entity>,
}

/// Snap a raw scrub time to the nearest ruler tick or existing
/// keyframe time, whichever is closer, provided the candidate falls
/// within `snap.threshold_ratio * duration` of the raw time.
///
/// Returns a [`SnapResult`] whose `hovered_keyframe` is `Some` only
/// when the final time came from a keyframe snap (not a tick snap
/// and not the raw time). Tick snaps don't get visual feedback
/// because the ruler already shows tick positions.
fn apply_snap(
    raw_time: f32,
    duration: f32,
    snap: &TimelineSnap,
    keyframes: &[(Entity, f32)],
) -> SnapResult {
    if !snap.enabled || duration <= 0.0 {
        return SnapResult {
            time: raw_time,
            hovered_keyframe: None,
        };
    }
    let threshold = snap.threshold_ratio * duration;
    let mut best = raw_time;
    let mut best_dist = threshold;
    let mut hovered: Option<Entity> = None;

    if snap.snap_to_ticks {
        let step = pick_tick_step(duration);
        if step > 0.0 {
            let snapped = (raw_time / step).round() * step;
            let dist = (snapped - raw_time).abs();
            if dist < best_dist {
                best_dist = dist;
                best = snapped.clamp(0.0, duration);
                hovered = None;
            }
        }
    }

    if snap.snap_to_keyframes {
        for &(entity, kf_time) in keyframes {
            let dist = (kf_time - raw_time).abs();
            if dist < best_dist {
                best_dist = dist;
                best = kf_time;
                hovered = Some(entity);
            }
        }
    }

    SnapResult {
        time: best,
        hovered_keyframe: hovered,
    }
}

/// Collect every `(entity, time)` pair in the clip, across all
/// tracks. Used by [`apply_snap`] for candidate positions and by
/// future arrow-key navigation to step through keyframes.
fn all_keyframes_for_clip(
    clip_entity: Entity,
    clips: &Query<(&Clip, Option<&Children>)>,
    tracks: &Query<(&AnimTrack, Option<&Children>)>,
    vec3_keyframes: &Query<&Vec3Keyframe>,
    quat_keyframes: &Query<&QuatKeyframe>,
    f32_keyframes: &Query<&F32Keyframe>,
) -> Vec<(Entity, f32)> {
    let Ok((_, clip_children)) = clips.get(clip_entity) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for track_entity in clip_children.into_iter().flatten() {
        let Ok((_, track_children)) = tracks.get(*track_entity) else {
            continue;
        };
        for kf in track_children.into_iter().flatten() {
            if let Ok(k) = vec3_keyframes.get(*kf) {
                out.push((*kf, k.time));
            } else if let Ok(k) = quat_keyframes.get(*kf) {
                out.push((*kf, k.time));
            } else if let Ok(k) = f32_keyframes.get(*kf) {
                out.push((*kf, k.time));
            }
        }
    }
    out
}

/// Observer: clicking a keyframe diamond toggles the keyframe's
/// selection in [`SelectedKeyframes`]. Ctrl+click toggles; plain
/// click replaces the selection with just that keyframe. Stops
/// propagation so the underlying scrubber doesn't also receive the
/// click and seek to the diamond's time.
pub fn handle_keyframe_click(
    mut event: On<Pointer<Click>>,
    handles: Query<&TimelineKeyframeHandle>,
    mut selected: ResMut<SelectedKeyframes>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let Ok(handle) = handles.get(event.event_target()) else {
        return;
    };
    let multi = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if multi {
        selected.toggle(handle.keyframe);
    } else {
        selected.select_only(handle.keyframe);
    }
    event.propagate(false);
}

/// Paint every rendered keyframe diamond based on its state:
/// selected, snap-hovered, or default. Runs every frame — cheap
/// because there are only a handful of diamonds, and it means the
/// visual picks up any change without a full rebuild.
///
/// Precedence: selection wins over snap-hover. A keyframe that's
/// both selected and being snapped onto stays in its selection
/// color, since selection is the user's committed choice.
pub fn update_keyframe_highlight(
    selected: Res<SelectedKeyframes>,
    hint: Res<TimelineSnapHint>,
    mut handles: Query<(&TimelineKeyframeHandle, &mut BackgroundColor, &mut BorderColor)>,
) {
    for (handle, mut bg, mut border) in &mut handles {
        if selected.is_selected(handle.keyframe) {
            // Selected — amber with white border.
            bg.0 = Color::srgb(1.0, 0.78, 0.12);
            *border = BorderColor::all(Color::WHITE);
        } else if hint.hovered_keyframe == Some(handle.keyframe) {
            // Snap hover — brighter accent blue with white border
            // so the user sees exactly where their drag is landing.
            bg.0 = Color::srgb(0.38, 0.72, 1.0);
            *border = BorderColor::all(Color::WHITE);
        } else {
            bg.0 = tokens::ACCENT_BLUE;
            *border = BorderColor::all(Color::WHITE.with_alpha(0.4));
        }
    }
}

/// Keyboard handler: `Delete` or `Backspace` despawns every entity
/// in [`SelectedKeyframes`] via `DespawnEntity` commands so the edit
/// is undo-aware. Cleared after handling. Clears its own selection
/// set on completion; the [`compile_clips`] + `mark_timeline_dirty`
/// pipeline then rebuilds the clip asset and the widget.
pub fn handle_delete_selected_keyframes(
    keys: Res<ButtonInput<KeyCode>>,
    mut selected: ResMut<SelectedKeyframes>,
    mut dirty: ResMut<TimelineDirty>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Delete) && !keys.just_pressed(KeyCode::Backspace) {
        return;
    }
    if selected.entities.is_empty() {
        return;
    }
    let to_delete: Vec<Entity> = selected.entities.iter().copied().collect();
    selected.clear();
    for entity in to_delete {
        commands.entity(entity).despawn();
    }
    dirty.0 = true;
}
