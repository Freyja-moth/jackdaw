use bevy::{app::PropagateOver, feathers::font_styles::InheritableFont, prelude::*};
use lucide_icons::Icon;

use crate::constants::fonts;

#[derive(Component)]
pub struct FontIcon;

pub fn font_icon(icon: Icon) -> impl Bundle {
    (
        FontIcon,
        Text::new(icon.unicode()),
        PropagateOver::<TextFont>::default(),
        TextFont::default(),
        TextColor::default(),
    )
}

// TODO: Replace system with bsn template
fn set_icon_font(
    insert: On<Insert, FontIcon>,
    mut text_font: Query<&mut TextFont>,
    asset_server: Res<AssetServer>,
) -> Result<(), BevyError> {
    let mut text_font = text_font.get_mut(insert.entity)?;

    text_font.font = asset_server.load(fonts::LUCIDE_ICONS);

    Ok(())
}

pub struct IconPlugin;

impl Plugin for IconPlugin {
    fn build(&self, app: &mut App) {}
}
