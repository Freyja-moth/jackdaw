//! BSN scene function for displaying a plain text string in the correct font.
use bevy::{app::PropagateOver, prelude::*};
// use bevy_scene::{bsn, Scene};
// use bevy_text::{FontSourceTemplate, FontWeight, TextFont};
// use bevy_ui::widget::Text;

use crate::{
    constants::{fonts, size},
    tokens,
};

/// A text label.
pub fn label(text: impl Into<String>) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font: FontSourceTemplate::Handle(fonts::REGULAR),
            font_size: size::MEDIUM_FONT,
            weight: FontWeight::NORMAL,
            ..Default::default()
        },
        PropagateOver::<TextFont>::default(),
        ThemeTextColor(tokens::TEXT_MAIN),
    )
}

/// A text label with a dimmed color.
pub fn label_dim(text: impl Into<String>) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font: FontSourceTemplate::Handle(fonts::REGULAR),
            font_size: size::MEDIUM_FONT,
            weight: FontWeight::NORMAL,
            ..Default::default()
        },
        PropagateOver::<TextFont>::default(),
        ThemeTextColor(tokens::TEXT_DIM),
    )
}

/// A small text label, used for field captions.
pub fn label_small(text: impl Into<String>) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font: FontSourceTemplate::Handle(fonts::REGULAR),
            font_size: size::EXTRA_SMALL_FONT,
            weight: FontWeight::NORMAL,
            ..Default::default()
        },
        PropagateOver::<TextFont>::default(),
        ThemeTextColor(tokens::TEXT_MAIN),
    )
}
