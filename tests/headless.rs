use bevy::{
    prelude::*,
    render::{
        RenderPlugin,
        settings::{RenderCreation, WgpuSettings},
    },
    state::app::StatesPlugin,
    winit::WinitPlugin,
};
use bevy_enhanced_input::prelude::*;
use jackdaw::prelude::*;

#[test]
fn smoke_test_headless_update() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(WgpuSettings {
                    backends: None,
                    ..default()
                }),
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .add_plugins(EditorPlugin)
    .finish();

    for _ in 0..10 {
        app.update();
    }
}
