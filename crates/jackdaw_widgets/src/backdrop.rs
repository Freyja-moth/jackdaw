use bevy::{ecs::entity_disabling::Disabled, prelude::*};

#[derive(Component)]
pub struct Backdrop;

fn disable_backdrop_on_click(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    backdrops: Query<(), With<Backdrop>>,
) {
    if backdrops.contains(click.entity) {
        return;
    }

    commands
        .entity(click.entity)
        .insert_recursive::<Children>(Disabled);
}

pub struct BackdropPlugin;

impl Plugin for BackdropPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(disable_backdrop_on_click);
    }
}
