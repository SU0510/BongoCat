//! The panel's product palette.
//!
//! The desk pet's own sprite colours are the source of this theme: cream
//! `#f8f0e0`, warm brown `#483838`, rose `#f0a0b0` and peach `#f0d0b8`. The
//! panel paints the same family in a light ("cream") and a dark ("cocoa")
//! variant, so the window and the character on the desk read as one product
//! instead of a warm picture on a grey form.
//!
//! [`install`] writes the palette into gpui-kit's [`Theme`]. Every component —
//! tabs, buttons, inputs, scrollbars, chat bubbles — reads its colours from
//! there, so no widget carries a private colour of its own. The text and
//! surface pairs the panel depends on are pinned by WCAG contrast assertions in
//! the tests below, which is also how the palette is edited: change a value,
//! run the tests, and a pair that stops being legible fails the build.
//!
//! Both variants are checked in at full length rather than derived from one
//! another. A dark palette is not a light palette with the lightness flipped —
//! the rose that reads as pink on cream reads as neon on cocoa, so the two are
//! tuned separately and reviewed side by side.

use gpui_kit::component::theme::{Theme, ThemeColor, ThemeMode};
use gpui_kit::{App, Hsla, px, rgba};

/// One variant of the panel theme. Field names match [`Theme`]'s colour fields
/// one for one, so [`Palette::apply`] can copy them by name; values are
/// `0xrrggbbaa` so translucency is visible where it is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Palette {
    // Base surfaces and text.
    background: u32,
    foreground: u32,
    popover: u32,
    popover_foreground: u32,
    muted: u32,
    muted_foreground: u32,
    border: u32,
    input: u32,
    ring: u32,
    caret: u32,
    selection: u32,
    overlay: u32,
    window_border: u32,

    // Calls to action.
    primary: u32,
    primary_foreground: u32,
    primary_hover: u32,
    primary_active: u32,
    secondary: u32,
    secondary_foreground: u32,
    secondary_hover: u32,
    secondary_active: u32,
    accent: u32,
    accent_foreground: u32,
    link: u32,
    link_hover: u32,
    link_active: u32,

    // Status.
    danger: u32,
    danger_foreground: u32,
    danger_hover: u32,
    danger_active: u32,
    success: u32,
    success_foreground: u32,
    success_hover: u32,
    success_active: u32,
    warning: u32,
    warning_foreground: u32,
    warning_hover: u32,
    warning_active: u32,
    info: u32,
    info_foreground: u32,
    info_hover: u32,
    info_active: u32,

    // Buttons mirror the roles above; the component library reads these
    // instead of the semantic names.
    button: u32,
    button_foreground: u32,
    button_hover: u32,
    button_active: u32,
    button_primary: u32,
    button_primary_foreground: u32,
    button_primary_hover: u32,
    button_primary_active: u32,
    button_secondary: u32,
    button_secondary_foreground: u32,
    button_secondary_hover: u32,
    button_secondary_active: u32,
    button_danger: u32,
    button_danger_foreground: u32,
    button_danger_hover: u32,
    button_danger_active: u32,
    button_success: u32,
    button_success_foreground: u32,
    button_success_hover: u32,
    button_success_active: u32,
    button_warning: u32,
    button_warning_foreground: u32,
    button_warning_hover: u32,
    button_warning_active: u32,
    button_info: u32,
    button_info_foreground: u32,
    button_info_hover: u32,
    button_info_active: u32,

    // Containers.
    group_box: u32,
    group_box_foreground: u32,
    accordion: u32,
    description_list_label: u32,
    description_list_label_foreground: u32,
    list: u32,
    list_hover: u32,
    list_active: u32,
    list_active_border: u32,
    list_even: u32,
    list_head: u32,
    table: u32,
    table_hover: u32,
    table_active: u32,
    table_active_border: u32,
    table_even: u32,
    table_head: u32,
    table_head_foreground: u32,
    table_foot: u32,
    table_foot_foreground: u32,
    table_row_border: u32,
    tab: u32,
    tab_active: u32,
    tab_active_foreground: u32,
    tab_bar: u32,
    tab_bar_segmented: u32,
    tab_foreground: u32,
    title_bar: u32,
    title_bar_border: u32,
    status_bar: u32,
    status_bar_border: u32,
    sidebar: u32,
    sidebar_foreground: u32,
    sidebar_border: u32,
    sidebar_accent: u32,
    sidebar_accent_foreground: u32,
    sidebar_primary: u32,
    sidebar_primary_foreground: u32,

    // Indicators.
    progress_bar: u32,
    slider_bar: u32,
    slider_thumb: u32,
    switch: u32,
    switch_thumb: u32,
    skeleton: u32,
    scrollbar: u32,
    scrollbar_thumb: u32,
    scrollbar_thumb_hover: u32,
    drag_border: u32,
    drop_target: u32,

    // Charts and the named base hues.
    chart_1: u32,
    chart_2: u32,
    chart_3: u32,
    chart_4: u32,
    chart_5: u32,
    chart_bullish: u32,
    chart_bearish: u32,
    chart_grid: u32,
    red: u32,
    red_light: u32,
    green: u32,
    green_light: u32,
    blue: u32,
    blue_light: u32,
    yellow: u32,
    yellow_light: u32,
    magenta: u32,
    magenta_light: u32,
    cyan: u32,
    cyan_light: u32,
}

impl Palette {
    /// Warm off-white with rose accents — the palette the panel opens with.
    const CREAM: Self = Self {
        background: 0xfffaf5ff,
        foreground: 0x453936ff,
        popover: 0xffffffff,
        popover_foreground: 0x453936ff,
        muted: 0xf8ede4ff,
        muted_foreground: 0x7a6660ff,
        border: 0xebd7cbff,
        input: 0xffffffff,
        ring: 0xe68ca0ff,
        caret: 0x453936ff,
        selection: 0xfbd9e2ff,
        overlay: 0x0000000d,
        window_border: 0xebd7cbff,

        primary: 0xf0a0b0ff,
        primary_foreground: 0x4a3238ff,
        primary_hover: 0xf5a9b8ff,
        primary_active: 0xea96a7ff,
        secondary: 0xf8ede4ff,
        secondary_foreground: 0x6b5651ff,
        secondary_hover: 0xf3e5daff,
        secondary_active: 0xeddccfff,
        accent: 0xfdefe6ff,
        accent_foreground: 0x7a5b52ff,
        link: 0xb84a63ff,
        link_hover: 0xa84058ff,
        link_active: 0x96374cff,

        danger: 0xc9463fff,
        danger_foreground: 0xffffffff,
        danger_hover: 0xd15048ff,
        danger_active: 0xbc3d37ff,
        success: 0x3a7f6aff,
        success_foreground: 0xffffffff,
        success_hover: 0x428c75ff,
        success_active: 0x35705dff,
        warning: 0xe0a33aff,
        warning_foreground: 0x3f2e10ff,
        warning_hover: 0xe8ae4eff,
        warning_active: 0xd2962bff,
        info: 0x45738cff,
        info_foreground: 0xffffffff,
        info_hover: 0x4e8099ff,
        info_active: 0x3c677dff,

        button: 0xf0a0b0ff,
        button_foreground: 0x4a3238ff,
        button_hover: 0xf3a7b6ff,
        button_active: 0xe593a4ff,
        button_primary: 0xf0a0b0ff,
        button_primary_foreground: 0x4a3238ff,
        button_primary_hover: 0xf5a9b8ff,
        button_primary_active: 0xea96a7ff,
        button_secondary: 0xf8ede4ff,
        button_secondary_foreground: 0x6b5651ff,
        button_secondary_hover: 0xf3e5daff,
        button_secondary_active: 0xeddccfff,
        button_danger: 0xc9463fff,
        button_danger_foreground: 0xffffffff,
        button_danger_hover: 0xd15048ff,
        button_danger_active: 0xbc3d37ff,
        button_success: 0x3a7f6aff,
        button_success_foreground: 0xffffffff,
        button_success_hover: 0x428c75ff,
        button_success_active: 0x35705dff,
        button_warning: 0xe0a33aff,
        button_warning_foreground: 0x3f2e10ff,
        button_warning_hover: 0xe8ae4eff,
        button_warning_active: 0xd2962bff,
        button_info: 0x45738cff,
        button_info_foreground: 0xffffffff,
        button_info_hover: 0x4e8099ff,
        button_info_active: 0x3c677dff,

        group_box: 0xfdf5efff,
        group_box_foreground: 0x453936ff,
        accordion: 0xfdf5efff,
        description_list_label: 0xfdf5efff,
        description_list_label_foreground: 0x7a5b52ff,
        list: 0xffffffff,
        list_hover: 0xfaf0e8ff,
        list_active: 0xf9e3eaff,
        list_active_border: 0xf0a0b0ff,
        list_even: 0xfdf7f2ff,
        list_head: 0xfaf0e8ff,
        table: 0xffffffff,
        table_hover: 0xfaf0e8ff,
        table_active: 0xf9e3eaff,
        table_active_border: 0xf0a0b0ff,
        table_even: 0xfdf7f2ff,
        table_head: 0xfaf0e8ff,
        table_head_foreground: 0x7a6660ff,
        table_foot: 0xfaf0e8ff,
        table_foot_foreground: 0x7a6660ff,
        table_row_border: 0xf1e2d8ff,
        tab: 0x00000000,
        tab_active: 0xffffffff,
        tab_active_foreground: 0x453936ff,
        tab_bar: 0xfbf1e8ff,
        tab_bar_segmented: 0xf3e5daff,
        tab_foreground: 0x7a6660ff,
        title_bar: 0xfbf1e8ff,
        title_bar_border: 0xebd7cbff,
        status_bar: 0xfbf1e8ff,
        status_bar_border: 0xebd7cbff,
        sidebar: 0xfbf1e8ff,
        sidebar_foreground: 0x453936ff,
        sidebar_border: 0xebd7cbff,
        sidebar_accent: 0xf8ede4ff,
        sidebar_accent_foreground: 0x453936ff,
        sidebar_primary: 0xf0a0b0ff,
        sidebar_primary_foreground: 0x4a3238ff,

        progress_bar: 0xf0a0b0ff,
        slider_bar: 0xf0a0b0ff,
        slider_thumb: 0xffffffff,
        switch: 0xe8d5c9ff,
        switch_thumb: 0xffffffff,
        skeleton: 0xf5e8deff,
        scrollbar: 0x00000000,
        scrollbar_thumb: 0xe0c7b9ff,
        scrollbar_thumb_hover: 0xd3b4a3ff,
        drag_border: 0xe68ca0ff,
        drop_target: 0xf0a0b033,

        chart_1: 0xf0a0b0ff,
        chart_2: 0x7fc9aeff,
        chart_3: 0xf0c08aff,
        chart_4: 0x8fb8dcff,
        chart_5: 0xc0a0d8ff,
        chart_bullish: 0x3a7f6aff,
        chart_bearish: 0xc9463fff,
        chart_grid: 0xebd7cb99,
        red: 0xc9463fff,
        red_light: 0xe08a85ff,
        green: 0x3a7f6aff,
        green_light: 0x7fc9aeff,
        blue: 0x45738cff,
        blue_light: 0x8fb8dcff,
        yellow: 0xe0a33aff,
        yellow_light: 0xf0c08aff,
        magenta: 0xf0a0b0ff,
        magenta_light: 0xf6c2ccff,
        cyan: 0x5e93b0ff,
        cyan_light: 0x9cc6d8ff,
    };

    /// Cocoa background with the same rose — the variant behind a dark system
    /// appearance.
    const COCOA: Self = Self {
        background: 0x241e20ff,
        foreground: 0xf7ece3ff,
        popover: 0x2c2528ff,
        popover_foreground: 0xf7ece3ff,
        muted: 0x382f32ff,
        muted_foreground: 0xb5a29bff,
        border: 0x453a3dff,
        input: 0x2c2528ff,
        ring: 0xf2a6b4ff,
        caret: 0xf7ece3ff,
        selection: 0x6a3f4cff,
        overlay: 0x00000033,
        window_border: 0x453a3dff,

        primary: 0xf2a6b4ff,
        primary_foreground: 0x3a2a2eff,
        primary_hover: 0xf7b2beff,
        primary_active: 0xe79aa8ff,
        secondary: 0x3a3134ff,
        secondary_foreground: 0xf0e2d9ff,
        secondary_hover: 0x443a3dff,
        secondary_active: 0x4e4346ff,
        accent: 0x443739ff,
        accent_foreground: 0xf7ece3ff,
        link: 0xf0a0b0ff,
        link_hover: 0xf6b6c2ff,
        link_active: 0xe794a5ff,

        danger: 0xf08a8aff,
        danger_foreground: 0x3a2a2eff,
        danger_hover: 0xf59b9bff,
        danger_active: 0xe57d7dff,
        success: 0x7fc9aeff,
        success_foreground: 0x24312cff,
        success_hover: 0x92d5bdff,
        success_active: 0x6fbc9fff,
        warning: 0xefc06aff,
        warning_foreground: 0x3a2e12ff,
        warning_hover: 0xf4cc81ff,
        warning_active: 0xe4b357ff,
        info: 0x8fc0d8ff,
        info_foreground: 0x1e2c36ff,
        info_hover: 0xa1cce1ff,
        info_active: 0x7fb3cdff,

        button: 0xf2a6b4ff,
        button_foreground: 0x3a2a2eff,
        button_hover: 0xf7b2beff,
        button_active: 0xe79aa8ff,
        button_primary: 0xf2a6b4ff,
        button_primary_foreground: 0x3a2a2eff,
        button_primary_hover: 0xf7b2beff,
        button_primary_active: 0xe79aa8ff,
        button_secondary: 0x3a3134ff,
        button_secondary_foreground: 0xf0e2d9ff,
        button_secondary_hover: 0x443a3dff,
        button_secondary_active: 0x4e4346ff,
        button_danger: 0xf08a8aff,
        button_danger_foreground: 0x3a2a2eff,
        button_danger_hover: 0xf59b9bff,
        button_danger_active: 0xe57d7dff,
        button_success: 0x7fc9aeff,
        button_success_foreground: 0x24312cff,
        button_success_hover: 0x92d5bdff,
        button_success_active: 0x6fbc9fff,
        button_warning: 0xefc06aff,
        button_warning_foreground: 0x3a2e12ff,
        button_warning_hover: 0xf4cc81ff,
        button_warning_active: 0xe4b357ff,
        button_info: 0x8fc0d8ff,
        button_info_foreground: 0x1e2c36ff,
        button_info_hover: 0xa1cce1ff,
        button_info_active: 0x7fb3cdff,

        group_box: 0x2c2528ff,
        group_box_foreground: 0xf7ece3ff,
        accordion: 0x2c2528ff,
        description_list_label: 0x322a2dff,
        description_list_label_foreground: 0xf0e2d9ff,
        list: 0x2c2528ff,
        list_hover: 0x382f32ff,
        list_active: 0x4a3340ff,
        list_active_border: 0xf2a6b4ff,
        list_even: 0x2a2326ff,
        list_head: 0x382f32ff,
        table: 0x2c2528ff,
        table_hover: 0x382f32ff,
        table_active: 0x4a3340ff,
        table_active_border: 0xf2a6b4ff,
        table_even: 0x2a2326ff,
        table_head: 0x382f32ff,
        table_head_foreground: 0xb5a29bff,
        table_foot: 0x382f32ff,
        table_foot_foreground: 0xb5a29bff,
        table_row_border: 0x453a3dff,
        tab: 0x00000000,
        tab_active: 0x3a3134ff,
        tab_active_foreground: 0xf7ece3ff,
        tab_bar: 0x2c2528ff,
        tab_bar_segmented: 0x382f32ff,
        tab_foreground: 0xb5a29bff,
        title_bar: 0x2c2528ff,
        title_bar_border: 0x453a3dff,
        status_bar: 0x2c2528ff,
        status_bar_border: 0x453a3dff,
        sidebar: 0x2c2528ff,
        sidebar_foreground: 0xf7ece3ff,
        sidebar_border: 0x453a3dff,
        sidebar_accent: 0x382f32ff,
        sidebar_accent_foreground: 0xf7ece3ff,
        sidebar_primary: 0xf2a6b4ff,
        sidebar_primary_foreground: 0x3a2a2eff,

        progress_bar: 0xf2a6b4ff,
        slider_bar: 0xf2a6b4ff,
        slider_thumb: 0xf7ece3ff,
        switch: 0x4a3e41ff,
        switch_thumb: 0xf7ece3ff,
        skeleton: 0x382f32ff,
        scrollbar: 0x00000000,
        scrollbar_thumb: 0x55474aff,
        scrollbar_thumb_hover: 0x6a585cff,
        drag_border: 0xf2a6b4ff,
        drop_target: 0xf2a6b433,

        chart_1: 0xf2a6b4ff,
        chart_2: 0x7fc9aeff,
        chart_3: 0xefc06aff,
        chart_4: 0x8fc0d8ff,
        chart_5: 0xc0a0d8ff,
        chart_bullish: 0x7fc9aeff,
        chart_bearish: 0xf08a8aff,
        chart_grid: 0x453a3d99,
        red: 0xf08a8aff,
        red_light: 0xf5a3a3ff,
        green: 0x7fc9aeff,
        green_light: 0x9ad8c0ff,
        blue: 0x8fc0d8ff,
        blue_light: 0xa8cfe3ff,
        yellow: 0xefc06aff,
        yellow_light: 0xf4d18cff,
        magenta: 0xf2a6b4ff,
        magenta_light: 0xf8c3cdff,
        cyan: 0x8fc0d8ff,
        cyan_light: 0xa8cfe3ff,
    };

    /// Copies the palette onto the live theme and sets the control shapes the
    /// panel is drawn with.
    ///
    /// Radii come from the theme, not from each widget: `radius` is the small
    /// control radius and `radius_lg` the card radius, and the chat bubbles
    /// derive theirs from `radius` (`Theme::radius_2xl`).
    fn apply(&self, theme: &mut Theme) {
        // `Theme` derefs to its colour surface, which is also where the names
        // below live; going through it keeps `list` (a colour) apart from
        // `Theme::list` (list settings).
        let colors: &mut ThemeColor = theme;
        macro_rules! set {
            ($($field:ident),* $(,)?) => {
                $( colors.$field = color(self.$field); )*
            };
        }
        set!(
            background,
            foreground,
            popover,
            popover_foreground,
            muted,
            muted_foreground,
            border,
            input,
            ring,
            caret,
            selection,
            overlay,
            window_border,
            primary,
            primary_foreground,
            primary_hover,
            primary_active,
            secondary,
            secondary_foreground,
            secondary_hover,
            secondary_active,
            accent,
            accent_foreground,
            link,
            link_hover,
            link_active,
            danger,
            danger_foreground,
            danger_hover,
            danger_active,
            success,
            success_foreground,
            success_hover,
            success_active,
            warning,
            warning_foreground,
            warning_hover,
            warning_active,
            info,
            info_foreground,
            info_hover,
            info_active,
            button,
            button_foreground,
            button_hover,
            button_active,
            button_primary,
            button_primary_foreground,
            button_primary_hover,
            button_primary_active,
            button_secondary,
            button_secondary_foreground,
            button_secondary_hover,
            button_secondary_active,
            button_danger,
            button_danger_foreground,
            button_danger_hover,
            button_danger_active,
            button_success,
            button_success_foreground,
            button_success_hover,
            button_success_active,
            button_warning,
            button_warning_foreground,
            button_warning_hover,
            button_warning_active,
            button_info,
            button_info_foreground,
            button_info_hover,
            button_info_active,
            group_box,
            group_box_foreground,
            accordion,
            description_list_label,
            description_list_label_foreground,
            list,
            list_hover,
            list_active,
            list_active_border,
            list_even,
            list_head,
            table,
            table_hover,
            table_active,
            table_active_border,
            table_even,
            table_head,
            table_head_foreground,
            table_foot,
            table_foot_foreground,
            table_row_border,
            tab,
            tab_active,
            tab_active_foreground,
            tab_bar,
            tab_bar_segmented,
            tab_foreground,
            title_bar,
            title_bar_border,
            status_bar,
            status_bar_border,
            sidebar,
            sidebar_foreground,
            sidebar_border,
            sidebar_accent,
            sidebar_accent_foreground,
            sidebar_primary,
            sidebar_primary_foreground,
            progress_bar,
            slider_bar,
            slider_thumb,
            switch,
            switch_thumb,
            skeleton,
            scrollbar,
            scrollbar_thumb,
            scrollbar_thumb_hover,
            drag_border,
            drop_target,
            chart_1,
            chart_2,
            chart_3,
            chart_4,
            chart_5,
            chart_bullish,
            chart_bearish,
            chart_grid,
            red,
            red_light,
            green,
            green_light,
            blue,
            blue_light,
            yellow,
            yellow_light,
            magenta,
            magenta_light,
            cyan,
            cyan_light,
        );

        theme.radius = px(8.);
        theme.radius_lg = px(14.);
    }
}

fn color(value: u32) -> Hsla {
    rgba(value).into()
}

/// Installs the panel palette for the theme's current mode.
///
/// Call this after anything that reloads a registered theme — startup, and
/// every system appearance change — because reloading resets the colours to
/// the library's neutral greys.
pub fn install(cx: &mut App) {
    install_for(Theme::global(cx).mode, cx);
}

/// Installs the palette that belongs to `mode` on the live theme.
///
/// The panel tracks the window's own appearance rather than the theme's, so a
/// window that stays light while the system flips to dark keeps a readable
/// body text instead of inheriting the other scheme's colours.
pub fn install_for(mode: ThemeMode, cx: &mut App) {
    let palette = match mode {
        ThemeMode::Dark => Palette::COCOA,
        ThemeMode::Light => Palette::CREAM,
    };
    Theme::update(cx, |theme| {
        theme.mode = mode;
        palette.apply(theme);
    });
}

/// The palette a window's appearance maps to.
pub fn for_appearance(appearance: gpui_kit::WindowAppearance) -> ThemeMode {
    match appearance {
        gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark => {
            ThemeMode::Dark
        }
        gpui_kit::WindowAppearance::Light | gpui_kit::WindowAppearance::VibrantLight => {
            ThemeMode::Light
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative luminance, WCAG 2.1.
    fn luminance(value: u32) -> f64 {
        let channels = [(value >> 24) as u8, (value >> 16) as u8, (value >> 8) as u8];
        let linear = channels.map(|channel| {
            let channel = f64::from(channel) / 255.;
            if channel <= 0.039_28 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    fn contrast(foreground: u32, background: u32) -> f64 {
        let (a, b) = (luminance(foreground), luminance(background));
        let (high, low) = if a > b { (a, b) } else { (b, a) };
        (high + 0.05) / (low + 0.05)
    }

    /// Every text pair the panel draws, at the ratio it needs.
    ///
    /// 4.5:1 is WCAG AA for body text and is what the panel's 11-13px labels
    /// need; 3:1 is the AA threshold for large text and graphical objects and
    /// covers the icons and separators.
    fn text_pairs(palette: &Palette) -> Vec<(&'static str, u32, u32, f64)> {
        vec![
            (
                "body on canvas",
                palette.foreground,
                palette.background,
                4.5,
            ),
            ("body on card", palette.foreground, palette.popover, 4.5),
            ("body on header", palette.foreground, palette.muted, 4.5),
            (
                "body on tab strip",
                palette.foreground,
                palette.tab_bar,
                4.5,
            ),
            (
                "body on list row",
                palette.foreground,
                palette.list_hover,
                4.5,
            ),
            ("body on accent", palette.foreground, palette.accent, 4.5),
            // My own chat bubbles are `BubbleVariant::Filled` — `primary` with
            // `primary_foreground` — in both variants. The library's `Tinted`
            // variant mixes `primary` toward the canvas, which in the dark
            // variant stays light enough to swallow light text, so the panel
            // does not use it.
            (
                "body on my bubble",
                palette.primary_foreground,
                palette.primary,
                4.5,
            ),
            (
                "hint on canvas",
                palette.muted_foreground,
                palette.background,
                4.5,
            ),
            (
                "hint on card",
                palette.muted_foreground,
                palette.popover,
                4.5,
            ),
            (
                "hint on header",
                palette.muted_foreground,
                palette.muted,
                4.5,
            ),
            (
                "hint on tab strip",
                palette.muted_foreground,
                palette.tab_bar,
                4.5,
            ),
            (
                "hint on list row",
                palette.muted_foreground,
                palette.list_hover,
                4.5,
            ),
            ("inactive tab", palette.tab_foreground, palette.tab_bar, 4.5),
            (
                "active tab",
                palette.tab_active_foreground,
                palette.tab_active,
                4.5,
            ),
            (
                "secondary label",
                palette.secondary_foreground,
                palette.secondary,
                4.5,
            ),
            (
                "accent label",
                palette.accent_foreground,
                palette.accent,
                4.5,
            ),
            ("link on canvas", palette.link, palette.background, 4.5),
            (
                "danger fill",
                palette.danger_foreground,
                palette.danger,
                4.5,
            ),
            (
                "success fill",
                palette.success_foreground,
                palette.success,
                4.5,
            ),
            (
                "warning fill",
                palette.warning_foreground,
                palette.warning,
                4.5,
            ),
            ("info fill", palette.info_foreground, palette.info, 4.5),
            (
                "primary fill",
                palette.button_primary_foreground,
                palette.button_primary,
                4.5,
            ),
            // A plain `Button` is the main action of its panel, so the default
            // fill carries the primary rose and has to hold its label.
            (
                "main action label",
                palette.button_foreground,
                palette.button,
                4.5,
            ),
            (
                "danger button fill",
                palette.button_danger_foreground,
                palette.button_danger,
                4.5,
            ),
            (
                "switch knob on track",
                palette.switch_thumb,
                palette.switch,
                1.4,
            ),
            ("border on canvas", palette.border, palette.background, 1.2),
            ("border on card", palette.border, palette.popover, 1.2),
            (
                "focus ring on canvas",
                palette.ring,
                palette.background,
                2.0,
            ),
            (
                "scrollbar on card",
                palette.scrollbar_thumb,
                palette.popover,
                1.5,
            ),
        ]
    }

    #[test]
    fn every_text_pair_stays_legible() {
        for (name, palette) in [("cream", Palette::CREAM), ("cocoa", Palette::COCOA)] {
            for (pair, foreground, background, required) in text_pairs(&palette) {
                let ratio = contrast(foreground, background);
                assert!(
                    ratio >= required,
                    "{name}/{pair}: {foreground:#010x} on {background:#010x} is {ratio:.2}:1, \
                     below the required {required}:1"
                );
            }
        }
    }

    /// The two variants must not collapse into the same picture: a mode switch
    /// has to change the canvas, the text and the surfaces.
    #[test]
    fn the_two_variants_differ_where_it_matters() {
        for (field, cream, cocoa) in [
            (
                "background",
                Palette::CREAM.background,
                Palette::COCOA.background,
            ),
            (
                "foreground",
                Palette::CREAM.foreground,
                Palette::COCOA.foreground,
            ),
            ("popover", Palette::CREAM.popover, Palette::COCOA.popover),
            ("muted", Palette::CREAM.muted, Palette::COCOA.muted),
            ("border", Palette::CREAM.border, Palette::COCOA.border),
        ] {
            assert_ne!(cream, cocoa, "{field} is identical in both variants");
        }
        assert!(luminance(Palette::CREAM.background) > luminance(Palette::CREAM.foreground));
        assert!(luminance(Palette::COCOA.background) < luminance(Palette::COCOA.foreground));
    }

    /// The panel is a warm theme: its canvas is tinted toward red rather than
    /// sitting on the neutral axis, which is what separates it from the
    /// library's default palette.
    #[test]
    fn the_canvas_is_warm_not_neutral() {
        for palette in [Palette::CREAM, Palette::COCOA] {
            let red = (palette.background >> 24) as i32;
            let green = ((palette.background >> 16) & 0xff) as i32;
            let blue = ((palette.background >> 8) & 0xff) as i32;
            assert!(
                red > green && red > blue,
                "canvas {:#010x} is not red-dominant",
                palette.background
            );
            assert!(
                (red - blue) >= 4 && (red - green) <= 24,
                "canvas {:#010x} is not a warm tint",
                palette.background
            );
        }
    }
}
