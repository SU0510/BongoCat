//! Embedded sprite assets. The generated sheets (resources/insync/sprites)
//! are compiled into the binary so a released app never depends on the
//! repository layout; `SpriteLibrary::from_embedded` decodes them lazily.

use insync_sprite::SpriteLibrary;
use std::sync::OnceLock;

/// The 32x32 tray icon rendered from the idle frame.
pub fn tray_icon_bytes() -> &'static [u8] {
    include_bytes!("../../../resources/insync/icons/tray.png")
}

/// A short alarm chime shipped with the app.
pub fn alarm_sound_path() -> Option<std::path::PathBuf> {
    Some(resources_root().join("sounds").join("alarm.wav"))
}

pub fn resources_root() -> std::path::PathBuf {
    crate::resources_root()
}

/// The momo character's 13 animation sheets + manifest, compiled in.
pub fn sprite_library() -> &'static SpriteLibrary {
    static LIBRARY: OnceLock<SpriteLibrary> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let manifest = include_str!("../../../resources/insync/sprites/manifest.json");
        let sheets: Vec<(&str, &[u8])> = vec![
            (
                "momo/idle.png",
                include_bytes!("../../../resources/insync/sprites/momo/idle.png"),
            ),
            (
                "momo/blink.png",
                include_bytes!("../../../resources/insync/sprites/momo/blink.png"),
            ),
            (
                "momo/wave.png",
                include_bytes!("../../../resources/insync/sprites/momo/wave.png"),
            ),
            (
                "momo/walk.png",
                include_bytes!("../../../resources/insync/sprites/momo/walk.png"),
            ),
            (
                "momo/sleep.png",
                include_bytes!("../../../resources/insync/sprites/momo/sleep.png"),
            ),
            (
                "momo/work.png",
                include_bytes!("../../../resources/insync/sprites/momo/work.png"),
            ),
            (
                "momo/stretch.png",
                include_bytes!("../../../resources/insync/sprites/momo/stretch.png"),
            ),
            (
                "momo/react.png",
                include_bytes!("../../../resources/insync/sprites/momo/react.png"),
            ),
            (
                "momo/poke_react.png",
                include_bytes!("../../../resources/insync/sprites/momo/poke_react.png"),
            ),
            (
                "momo/message_react.png",
                include_bytes!("../../../resources/insync/sprites/momo/message_react.png"),
            ),
            (
                "momo/invite_react.png",
                include_bytes!("../../../resources/insync/sprites/momo/invite_react.png"),
            ),
            (
                "momo/celebrate.png",
                include_bytes!("../../../resources/insync/sprites/momo/celebrate.png"),
            ),
            (
                "momo/rest.png",
                include_bytes!("../../../resources/insync/sprites/momo/rest.png"),
            ),
        ];
        SpriteLibrary::from_embedded(manifest, &sheets).expect("embedded sprite manifest valid")
    })
}
