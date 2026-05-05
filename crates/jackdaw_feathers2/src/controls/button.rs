use bevy::{
    ecs::relationship::RelatedSpawner,
    feathers::{
        constants::fonts, cursor::EntityCursor, font_styles::InheritableFont,
        handle_or_path::HandleOrPath,
    },
    prelude::*,
    window::SystemCursorIcon,
};
use lucide_icons::Icon;

use crate::{
    constants::size,
    controls::font_icon,
    tokens::{
        CORNER_RADIUS_LG, PRIMARY_COLOR, TEXT_BODY_COLOR, TEXT_DISPLAY_COLOR, TEXT_MUTED_COLOR,
    },
};

#[derive(Component)]
pub struct LeftIcon;

#[derive(Component)]
pub struct Label;

#[derive(Component)]
pub struct Subtitle;

#[derive(Component)]
pub struct RightIcon;

#[derive(Component, Default, Clone, Copy, PartialEq)]
pub enum ButtonVariant {
    #[default]
    Default,
    Primary,
    Destructive,
    Ghost,
    Active,
    ActiveAlt,
}

#[derive(Component, Default, Clone, Copy)]
pub enum ButtonSize {
    #[default]
    MD,
    Icon,
    IconSM,
}

impl ButtonVariant {
    pub fn bg_color(&self, hovered: bool, interaction_disabled: bool) -> Srgba {
        use bevy::color::palettes::tailwind;

        if interaction_disabled {
            return TEXT_BODY_COLOR;
        }

        match self {
            Self::Default => tailwind::ZINC_700,
            Self::Ghost | Self::ActiveAlt => TEXT_BODY_COLOR,
            Self::Primary | Self::Active => PRIMARY_COLOR,
            Self::Destructive if hovered => tailwind::RED_600,
            Self::Destructive => tailwind::RED_500,
        }
    }
    pub fn bg_opacity(&self, hovered: bool, interaction_disabled: bool) -> f32 {
        if interaction_disabled {
            return 0.0;
        }

        match (self, hovered) {
            (Self::Ghost, false) => 0.0,
            (Self::Active, false) => 0.1,
            (Self::Active, true) => 0.15,
            (Self::ActiveAlt, _) => 0.05,
            (Self::Default, false) => 0.5,
            (Self::Default, true) => 0.8,
            (Self::Ghost, true) => 0.05,
            (Self::Primary | Self::Destructive, false) => 1.0,
            (Self::Primary | Self::Destructive, true) => 0.9,
        }
    }
    pub fn text_color(&self, interaction_disabled: bool) -> Srgba {
        if interaction_disabled {
            return TEXT_MUTED_COLOR;
        }

        match self {
            Self::Default | Self::Ghost | Self::ActiveAlt => TEXT_BODY_COLOR,
            Self::Primary | Self::Destructive => TEXT_DISPLAY_COLOR,
            Self::Active => PRIMARY_COLOR.lighter(0.05),
        }
    }
    pub fn border_color(&self, interaction_disabled: bool) -> Srgba {
        use bevy::color::palettes::tailwind;

        if interaction_disabled {
            return tailwind::ZINC_700;
        }

        match self {
            Self::Default | Self::Ghost => tailwind::ZINC_700,
            Self::Primary | Self::Active => PRIMARY_COLOR,
            Self::Destructive => tailwind::RED_500,
            Self::ActiveAlt => TEXT_BODY_COLOR,
        }
    }
    pub fn border(&self) -> Val {
        match self {
            Self::Default | Self::ActiveAlt => Val::Px(1.0),
            _ => Val::Px(0.0),
        }
    }
    pub fn border_opacity(&self, hovered: bool, interaction_disabled: bool) -> f32 {
        if interaction_disabled {
            return 0.0;
        }

        match self {
            Self::Ghost if !hovered => 0.0,
            Self::ActiveAlt => 0.2,
            _ => 1.0,
        }
    }
}

impl ButtonSize {
    fn width(&self) -> Val {
        match self {
            Self::Icon => Val::Px(28.0),
            Self::IconSM => Val::Px(24.0),
            Self::MD => Val::Auto,
        }
    }
    fn height(&self) -> Val {
        match self {
            Self::IconSM => Val::Px(24.0),
            _ => Val::Px(28.0),
        }
    }
    fn padding(&self) -> Val {
        match self {
            Self::MD => px(12.0),
            Self::Icon | Self::IconSM => px(0.0),
        }
    }
    fn icon_size(&self) -> f32 {
        match self {
            Self::IconSM => 14.0,
            _ => 16.0,
        }
    }
}

#[derive(Default)]
pub struct ButtonProps {
    pub label: String,
    pub variant: ButtonVariant,
    pub size: ButtonSize,
    pub align_left: bool,
    pub left_icon: Option<Icon>,
    pub right_icon: Option<Icon>,
    pub direction: FlexDirection,
    pub subtitle: Option<String>,
}

impl ButtonProps {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..default()
        }
    }
    pub fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn with_size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }
    pub fn align_left(mut self) -> Self {
        self.align_left = true;
        self
    }
    pub fn with_left_icon(mut self, icon: Icon) -> Self {
        self.left_icon = Some(icon);
        self
    }
    pub fn with_right_icon(mut self, icon: Icon) -> Self {
        self.right_icon = Some(icon);
        self
    }
    pub fn with_direction(mut self, direction: FlexDirection) -> Self {
        self.direction = direction;
        self
    }
    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
}

pub fn button(props: ButtonProps) -> impl Bundle {
    let ButtonProps {
        label,
        variant,
        size,
        align_left,
        left_icon,
        right_icon,
        direction,
        subtitle,
    } = props;

    let is_column = direction == FlexDirection::Column;
    (
        Button,
        variant,
        size,
        EntityCursor::System(SystemCursorIcon::Pointer),
        InheritableFont {
            font: fonts::REGULAR.into(),
            font_size: size::TEXT_SIZE,
        },
        Node {
            width: if align_left {
                percent(100)
            } else {
                size.width()
            },
            height: if is_column { Val::Auto } else { size.height() },
            padding: UiRect::axes(size.padding(), if is_column { px(6.0) } else { px(0.0) }),
            border: variant.border().all(),
            border_radius: BorderRadius::all(CORNER_RADIUS_LG),
            flex_direction: direction,
            column_gap: px(6.0),
            row_gap: px(6.0),
            justify_content: if align_left {
                JustifyContent::Start
            } else {
                JustifyContent::Center
            },
            align_items: if is_column {
                AlignItems::Start
            } else {
                AlignItems::Center
            },
            ..default()
        },
        BackgroundColor::default(),
        BorderColor::default(),
        Children::spawn(SpawnWith(move |parent: &mut RelatedSpawner<ChildOf>| {
            if let Some(icon) = left_icon {
                parent.spawn((LeftIcon, font_icon::font_icon(icon)));
            }

            parent.spawn((
                Label,
                Text::new(label),
                TextFont {
                    font_size: size::TEXT_SIZE,
                    weight: FontWeight::MEDIUM,
                    ..default()
                },
                TextColor(variant.text_color(false).into()),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));

            if let Some(ref subtitle) = subtitle {
                parent.spawn((
                    Subtitle,
                    Text::new(subtitle),
                    TextColor(TEXT_MUTED_COLOR.into()),
                    Node {
                        margin: UiRect::top(px(-6.0)),
                        ..default()
                    },
                ));
            }

            if let Some(icon) = right_icon {
                parent.spawn((RightIcon, font_icon::font_icon(icon)));
            }
        })),
    )
}

