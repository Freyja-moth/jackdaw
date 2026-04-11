//! Per-property "animate" diamond on inspector field rows.
//!
//! Watches newly-spawned [`FieldBinding`] components and, for the
//! animatable allowlist of `(component_type_path, field_path)` pairs,
//! appends a small diamond icon button next to the row. Clicking the
//! diamond finds-or-creates a [`jackdaw_animation::Clip`] child of the
//! field's source entity, finds-or-creates an [`AnimTrack`] for the
//! bound property under that clip, and spawns the correct typed
//! keyframe at the current cursor time.
//!
//! This is the Blender/Godot "hold I over a property to insert a
//! keyframe" flow, adapted to Jackdaw's BSN-first model — every
//! operation goes through `world.spawn` / `world.get_mut` on reflected
//! components, and the resulting clip round-trips through JSN.
//!
//! The allowlist currently covers Transform's three fields; adding a
//! new animatable property means one new entry in
//! [`ANIMATABLE_FIELDS`] plus matching arms in [`spawn_typed_keyframe`]
//! and in `jackdaw_animation::compile::build_curve_for_track`.
//!
//! [`FieldBinding`]: super::FieldBinding
//! [`jackdaw_animation::Clip`]: jackdaw_animation::Clip
//! [`AnimTrack`]: jackdaw_animation::AnimTrack

use bevy::prelude::*;
use jackdaw_animation::{
    AnimTrack, Clip, F32Keyframe, QuatKeyframe, SelectedClip, TimelineCursor, TimelineDirty,
    Vec3Keyframe,
};
use jackdaw_feathers::button::{ButtonClickEvent, ButtonProps, ButtonSize, ButtonVariant, button};
use jackdaw_feathers::icons::Icon;

use super::InspectorFieldRow;

const TRANSFORM: &str = "bevy_transform::components::transform::Transform";

/// The `(component_type_path, field_path)` pairs that get a keyframe
/// diamond in the inspector. Keep in sync with the compile dispatch
/// in `jackdaw_animation::compile::build_curve_for_track` and with
/// [`spawn_typed_keyframe`] below.
const ANIMATABLE_FIELDS: &[(&str, &str)] = &[
    (TRANSFORM, "translation"),
    (TRANSFORM, "rotation"),
    (TRANSFORM, "scale"),
];

/// Marker on the diamond button. The click observer reads this to
/// know which source entity + property to keyframe.
#[derive(Component, Clone, Debug)]
pub struct AnimDiamondButton {
    pub source_entity: Entity,
    pub component_type_path: String,
    pub field_path: String,
}

/// True if the given `(component_type_path, field_path)` is in the
/// animatable allowlist.
fn is_animatable(component_type_path: &str, field_path: &str) -> bool {
    ANIMATABLE_FIELDS
        .iter()
        .any(|(t, f)| *t == component_type_path && *f == field_path)
}

/// Spawn a diamond button on every newly-added `InspectorFieldRow`
/// whose root property is animatable. Runs in `Update` and fires only
/// when rows are (re-)spawned, so it's cheap.
///
/// The `InspectorFieldRow` marker sits on the row's **outer column
/// container**, which `reflect_fields.rs` spawns with `position_type:
/// Relative` specifically so absolutely-positioned children land in
/// the row's coordinate space. That lets us tuck the diamond into
/// the top-right corner next to the field label without reflowing
/// the column's flex layout, and gives us exactly one diamond per
/// composite field (not one per scalar axis input inside it).
pub fn decorate_animatable_fields(
    new_rows: Query<(Entity, &InspectorFieldRow), Added<InspectorFieldRow>>,
    mut commands: Commands,
) {
    for (row_entity, row) in &new_rows {
        if !is_animatable(&row.type_path, &row.field_path) {
            continue;
        }
        // Two-entity structure: an absolutely-positioned wrapper node
        // that takes care of where the diamond sits in the row, and a
        // child button that carries the `AnimDiamondButton` marker
        // plus the feathers `button()` bundle (which brings its own
        // Node). Splitting it this way avoids the duplicate-Node
        // panic that happens when you stack a custom Node alongside
        // a bundle that already includes one.
        let wrapper = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.0),
                    right: Val::Px(4.0),
                    ..default()
                },
                ChildOf(row_entity),
            ))
            .id();

        commands.spawn((
            AnimDiamondButton {
                source_entity: row.source_entity,
                component_type_path: row.type_path.clone(),
                field_path: row.field_path.clone(),
            },
            button(
                ButtonProps::new("")
                    .with_variant(ButtonVariant::Ghost)
                    .with_size(ButtonSize::IconSM)
                    .with_left_icon(Icon::Diamond),
            ),
            ChildOf(wrapper),
        ));
    }
}

/// Observer: when a diamond button is clicked, ensure a clip + track
/// exist for the bound property and spawn a keyframe at the current
/// cursor time.
pub fn on_diamond_click(
    event: On<ButtonClickEvent>,
    buttons: Query<&AnimDiamondButton>,
    mut commands: Commands,
) {
    let Ok(button_ref) = buttons.get(event.entity) else {
        return;
    };
    let source_entity = button_ref.source_entity;
    let component_type_path = button_ref.component_type_path.clone();
    let field_path = button_ref.field_path.clone();

    commands.queue(move |world: &mut World| {
        let cursor_time = world
            .get_resource::<TimelineCursor>()
            .map(|c| c.time)
            .unwrap_or(0.0);

        // Step 1: find or create a Clip as a child of the source
        // entity. The clip's name reuses the source's Name if set.
        let clip_entity = find_or_create_clip(world, source_entity);
        let Some(clip_entity) = clip_entity else {
            warn!(
                "Diamond click: source entity {source_entity} has no Name — \
                 give it one in the inspector first so the clip's target can \
                 resolve"
            );
            return;
        };

        // Step 2: find or create a track for this property under the
        // clip.
        let track_entity =
            find_or_create_track(world, clip_entity, &component_type_path, &field_path);

        // Step 3: snapshot the current reflected field value and
        // spawn the right typed keyframe component as a child of the
        // track.
        spawn_typed_keyframe(
            world,
            source_entity,
            track_entity,
            &component_type_path,
            &field_path,
            cursor_time,
        );

        // Step 4: grow the clip's authored duration if needed so the
        // new keyframe is visible in the timeline view.
        if let Some(mut clip) = world.get_mut::<Clip>(clip_entity) {
            if cursor_time > clip.duration {
                clip.duration = cursor_time;
            }
        }

        // Step 5: make this the active clip and force a timeline
        // rebuild so the new diamond/keyframe row appears.
        if let Some(mut selected) = world.get_resource_mut::<SelectedClip>() {
            selected.0 = Some(clip_entity);
        }
        if let Some(mut dirty) = world.get_resource_mut::<TimelineDirty>() {
            dirty.0 = true;
        }
    });
}

/// Return an existing `Clip` child of `source_entity`, or spawn one
/// and return its entity. Returns `None` if the source entity has no
/// `Name`, because name is required for the compile step to derive
/// the `AnimationTargetId`.
fn find_or_create_clip(world: &mut World, source_entity: Entity) -> Option<Entity> {
    let target_name = world
        .get::<Name>(source_entity)
        .map(|n| n.as_str().to_string())?;

    // Check existing Clip children.
    if let Some(children) = world.get::<Children>(source_entity) {
        let children_vec: Vec<Entity> = children.iter().collect();
        for child in children_vec {
            if world.get::<Clip>(child).is_some() {
                return Some(child);
            }
        }
    }

    // None exist — spawn one as a child of the source.
    let clip = world
        .spawn((
            Clip::default(),
            Name::new(format!("{target_name} Clip")),
            ChildOf(source_entity),
        ))
        .id();
    Some(clip)
}

/// Return an existing `AnimTrack` child of `clip_entity` matching
/// `(component_type_path, field_path)`, or spawn a new one.
fn find_or_create_track(
    world: &mut World,
    clip_entity: Entity,
    component_type_path: &str,
    field_path: &str,
) -> Entity {
    if let Some(children) = world.get::<Children>(clip_entity) {
        let children_vec: Vec<Entity> = children.iter().collect();
        for child in children_vec {
            if let Some(track) = world.get::<AnimTrack>(child) {
                if track.component_type_path == component_type_path
                    && track.field_path == field_path
                {
                    return child;
                }
            }
        }
    }

    let label = format!("/ {field_path}");
    world
        .spawn((
            AnimTrack::new(component_type_path.to_string(), field_path.to_string()),
            Name::new(label),
            ChildOf(clip_entity),
        ))
        .id()
}

/// Snapshot the current value of the animated field on the source
/// entity and spawn the appropriate typed keyframe component.
///
/// This is the dispatch mirror of
/// `jackdaw_animation::compile::build_curve_for_track` and
/// `jackdaw_animation::timeline::handle_add_keyframe_click`. Adding a
/// new animatable property means a new arm here plus a new arm
/// there. Keep them in sync.
fn spawn_typed_keyframe(
    world: &mut World,
    source_entity: Entity,
    track_entity: Entity,
    component_type_path: &str,
    field_path: &str,
    time: f32,
) {
    match (component_type_path, field_path) {
        (TRANSFORM, "translation") => {
            let Some(transform) = world.get::<Transform>(source_entity).copied() else {
                warn!("Diamond click: source has no Transform");
                return;
            };
            world.spawn((
                Vec3Keyframe {
                    time,
                    value: transform.translation,
                },
                ChildOf(track_entity),
            ));
        }
        (TRANSFORM, "rotation") => {
            let Some(transform) = world.get::<Transform>(source_entity).copied() else {
                warn!("Diamond click: source has no Transform");
                return;
            };
            world.spawn((
                QuatKeyframe {
                    time,
                    value: transform.rotation,
                },
                ChildOf(track_entity),
            ));
        }
        (TRANSFORM, "scale") => {
            let Some(transform) = world.get::<Transform>(source_entity).copied() else {
                warn!("Diamond click: source has no Transform");
                return;
            };
            world.spawn((
                Vec3Keyframe {
                    time,
                    value: transform.scale,
                },
                ChildOf(track_entity),
            ));
        }
        _ => {
            let _ = F32Keyframe::default();
            warn!(
                "Diamond click: no snapshot dispatch for {component_type_path}.{field_path}",
            );
        }
    }
}
