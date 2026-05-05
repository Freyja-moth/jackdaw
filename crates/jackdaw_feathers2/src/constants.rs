pub mod fonts {
    pub const LUCIDE_ICONS: &str = "embedded://jackdaw_feathers2/assets/fonts/lucide-icon.ttf";
}

pub mod size {
    // ---------------------------------------------------------------------------
    // Typography
    // ---------------------------------------------------------------------------

    use bevy::prelude::*;

    /// Common row size for buttons, sliders, spinners, etc.
    pub const ROW_HEIGHT: Val = Val::Px(24.0);

    /// Width and height of a checkbox
    pub const CHECKBOX_SIZE: Val = Val::Px(18.0);

    /// Height for pane headers
    pub const HEADER_HEIGHT: Val = Val::Px(30.0);

    /// Width and height of a radio button
    pub const RADIO_SIZE: Val = Val::Px(18.0);

    /// Width of a toggle switch
    pub const TOGGLE_WIDTH: Val = Val::Px(32.0);

    /// Height of a toggle switch
    pub const TOGGLE_HEIGHT: Val = Val::Px(18.0);

    /// Regular font size, used for most widget captions
    pub const MEDIUM_FONT: f32 = 14.0;

    /// Slightly smaller font size, used for text inputs
    pub const COMPACT_FONT: f32 = 13.0;

    /// Small font size
    pub const SMALL_FONT: f32 = 12.0;

    /// Extra-small font size
    pub const EXTRA_SMALL_FONT: f32 = 11.0;

    pub const TEXT_SIZE_SM: f32 = 11.0;
    pub const TEXT_SIZE: f32 = 13.0;
    pub const TEXT_SIZE_LG: f32 = 13.0;
    pub const TEXT_SIZE_XL: f32 = 18.0;

    // Keep old names as aliases for existing code
    pub const FONT_SM: f32 = TEXT_SIZE_SM;
    pub const FONT_MD: f32 = TEXT_SIZE;
    pub const FONT_LG: f32 = TEXT_SIZE_LG;

    // ---------------------------------------------------------------------------
    // Icon sizes (Lucide frame sizes)
    // ---------------------------------------------------------------------------

    /// Small icon size, standard Lucide icons (15px frame)
    pub const ICON_SM: f32 = 15.0;
    /// Medium icon size, sidebar icons (17px)
    pub const ICON_MD: f32 = 17.0;
    /// Large icon size (24px)
    pub const ICON_LG: f32 = 24.0;

    // ---------------------------------------------------------------------------
    // Spacing
    // ---------------------------------------------------------------------------

    pub const SPACING_XS: f32 = 2.0;
    pub const SPACING_SM: f32 = 4.0;
    pub const SPACING_MD: f32 = 8.0;
    pub const SPACING_LG: f32 = 12.0;

    // ---------------------------------------------------------------------------
    // Layout dimensions
    // ---------------------------------------------------------------------------

    pub const STATUS_BAR_HEIGHT: f32 = 22.0;
    pub const MENU_BAR_HEIGHT: f32 = 28.0;
    pub const INPUT_HEIGHT: f32 = 28.0;

    /// Panel tab bar height (Figma: 30px)
    pub const PANEL_TAB_HEIGHT: f32 = 30.0;
    /// Gap between panels in the layout (Figma: 4px)
    pub const PANEL_GAP: f32 = 4.0;
    /// Component card corner radius (Figma: 5px)
    pub const COMPONENT_CARD_RADIUS: f32 = 5.0;
    /// Breadcrumb bar height
    pub const BREADCRUMB_HEIGHT: f32 = 34.0;
    /// Asset browser sidebar width
    pub const SIDEBAR_WIDTH: f32 = 30.0;
    /// Search input default width
    pub const SEARCH_INPUT_WIDTH: f32 = 200.0;

    // ---------------------------------------------------------------------------
    // Border radii (numeric)
    // ---------------------------------------------------------------------------

    pub const BORDER_RADIUS_SM: f32 = 3.0;
    pub const BORDER_RADIUS_MD: f32 = 4.0;
    pub const BORDER_RADIUS_LG: f32 = 5.0;
}
