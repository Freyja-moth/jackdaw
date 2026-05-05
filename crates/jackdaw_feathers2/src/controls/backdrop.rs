use crate::tokens;
use bevy::{ecs::entity_disabling::Disabled, prelude::*, ui_widgets::observe};
use jackdaw_widgets::backdrop::Backdrop;

pub fn backdrop() -> impl Bundle {
    (
        Backdrop,
        bevy::ui::FocusPolicy::Block,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..Default::default()
        },
        BackgroundColor(tokens::DIALOG_BACKDROP),
        GlobalZIndex(100),
    )
}
