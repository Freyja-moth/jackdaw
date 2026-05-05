pub mod constants;
pub mod controls;
pub mod display;
pub mod tokens;

use bevy::{asset::embedded_asset, prelude::*};

pub struct JackdawFeathersCorePlugin;

impl Plugin for JackdawFeathersCorePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "assets/fonts/lucide-icons.ttf");
    }
}
