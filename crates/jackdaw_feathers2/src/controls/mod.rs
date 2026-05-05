pub mod backdrop;
pub mod button;
pub mod font_icon;
pub mod icon_button;

use bevy::prelude::*;

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {}
}
