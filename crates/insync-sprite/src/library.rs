//! Manifest + PNG loading. The manifest is the single source of truth for
//! frame sizes, frame counts, fps and loop flags; PNGs must match or the
//! library refuses to load. Sheets come either from the filesystem or from
//! in-memory bytes (embedded assets).

use image::{GenericImageView, RgbaImage};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpriteError {
    #[error("read manifest: {0}")]
    ManifestIo(String),
    #[error("parse manifest: {0}")]
    ManifestParse(String),
    #[error("unsupported manifest format {0}")]
    ManifestVersion(u32),
    #[error("read sprite sheet {path}: {error}")]
    SheetIo { path: String, error: String },
    #[error("decode sprite sheet {path}: {error}")]
    SheetDecode { path: String, error: String },
    #[error("sheet {path}: expected {expected} frames of {w}x{h}, got {actual_w}x{actual_h}")]
    SheetShape {
        path: String,
        expected: usize,
        w: u32,
        h: u32,
        got: usize,
        actual_w: u32,
        actual_h: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationClip {
    pub frames: usize,
    pub fps: u32,
    pub looped: bool,
}

/// A loaded character: frame geometry plus per-animation metadata. Actual
/// frame pixels stay in the sheet bitmap; slicing happens per draw so the
/// hot path can share one texture.
#[derive(Debug, Clone)]
pub struct CharacterSprites {
    pub frame_width: u32,
    pub frame_height: u32,
    /// animation name -> (clip metadata, sheet key)
    pub animations: HashMap<String, (AnimationClip, std::path::PathBuf)>,
    /// decoded sheets: key -> bitmap (shared across animations)
    pub sheets: HashMap<std::path::PathBuf, RgbaImage>,
}

impl CharacterSprites {
    /// The decoded horizontal slice for `(animation, frame_index)`; wraps
    /// and clamps so a bad index can never panic the render loop.
    pub fn frame(&self, animation: &str, index: usize) -> Option<RgbaImage> {
        let (clip, path) = self.animations.get(animation)?;
        let sheet = self.sheets.get(path)?;
        if clip.frames == 0 {
            return None;
        }
        let index = index % clip.frames;
        let x = (index as u32) * self.frame_width;
        Some(sheet.view(x, 0, self.frame_width, self.frame_height).to_image())
    }

    pub fn clip(&self, animation: &str) -> Option<AnimationClip> {
        self.animations.get(animation).map(|(clip, _)| *clip)
    }
}

#[derive(Debug)]
pub struct SpriteLibrary {
    pub characters: HashMap<String, CharacterSprites>,
}

impl SpriteLibrary {
    /// Loads `manifest.json` and every referenced sheet from disk.
    pub fn load(manifest_path: &Path) -> Result<Self, SpriteError> {
        let text = std::fs::read_to_string(manifest_path)
            .map_err(|e| SpriteError::ManifestIo(e.to_string()))?;
        let manifest: ManifestFile = serde_json::from_str(&text)
            .map_err(|e| SpriteError::ManifestParse(e.to_string()))?;
        Self::build(manifest, &|path| std::fs::read(path).ok())
    }

    /// Loads a library from in-memory bytes (embedded assets). `sheets`
    /// maps the manifest's relative file paths to their PNG bytes.
    pub fn from_embedded(
        manifest_json: &str,
        sheets: &[(&str, &[u8])],
    ) -> Result<Self, SpriteError> {
        let manifest: ManifestFile = serde_json::from_str(manifest_json)
            .map_err(|e| SpriteError::ManifestParse(e.to_string()))?;
        Self::build(manifest, &|path| {
            let path_text = path.to_string_lossy();
            sheets
                .iter()
                .find(|(name, _)| *name == path_text)
                .map(|(_, bytes)| bytes.to_vec())
        })
    }

    /// Shared builder: resolves each animation's sheet through `lookup`
    /// (filesystem or embedded) and validates every shape.
    fn build(
        manifest: ManifestFile,
        lookup: &dyn Fn(&Path) -> Option<Vec<u8>>,
    ) -> Result<Self, SpriteError> {
        if manifest.format_version != 1 {
            return Err(SpriteError::ManifestVersion(manifest.format_version));
        }
        let mut characters = HashMap::new();
        for (name, character) in manifest.characters {
            let mut sheets: HashMap<std::path::PathBuf, RgbaImage> = HashMap::new();
            let mut animations = HashMap::new();
            for (anim_name, raw) in character.animations {
                let path = std::path::PathBuf::from(&raw.file);
                if !sheets.contains_key(&path) {
                    let bytes = lookup(&path).ok_or_else(|| SpriteError::SheetIo {
                        path: raw.file.clone(),
                        error: "sheet not found".to_string(),
                    })?;
                    let sheet = decode_sheet(
                        &bytes,
                        &raw.file,
                        character.frame_width,
                        character.frame_height,
                        raw.frames,
                    )?;
                    sheets.insert(path.clone(), sheet);
                }
                animations.insert(
                    anim_name,
                    (
                        AnimationClip {
                            frames: raw.frames,
                            fps: raw.fps,
                            looped: raw.looped,
                        },
                        path,
                    ),
                );
            }
            characters.insert(
                name,
                CharacterSprites {
                    frame_width: character.frame_width,
                    frame_height: character.frame_height,
                    animations,
                    sheets,
                },
            );
        }
        Ok(Self { characters })
    }

    pub fn character(&self, name: &str) -> Option<&CharacterSprites> {
        self.characters.get(name)
    }
}

fn decode_sheet(
    bytes: &[u8],
    display: &str,
    frame_width: u32,
    frame_height: u32,
    frames: usize,
) -> Result<RgbaImage, SpriteError> {
    let decoded = image::load_from_memory(bytes).map_err(|e| SpriteError::SheetDecode {
        path: display.to_string(),
        error: e.to_string(),
    })?;
    let rgba = decoded.to_rgba8();
    if rgba.width() != frame_width * frames as u32 || rgba.height() != frame_height {
        return Err(SpriteError::SheetShape {
            path: display.to_string(),
            expected: frames,
            w: frame_width,
            h: frame_height,
            got: frames,
            actual_w: rgba.width(),
            actual_h: rgba.height(),
        });
    }
    Ok(rgba)
}

/// Manifest uses `"loop"`, which is a Rust keyword.
#[derive(Debug, Deserialize)]
struct ManifestAnimationRaw {
    file: String,
    frames: usize,
    fps: u32,
    #[serde(rename = "loop", default = "default_true")]
    looped: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct ManifestCharacter {
    frame_width: u32,
    frame_height: u32,
    animations: HashMap<String, ManifestAnimationRaw>,
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    format_version: u32,
    characters: HashMap<String, ManifestCharacter>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a tiny 2-frame sheet PNG in memory.
    fn sheet_png(frames: usize, w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbaImage::new(w * frames as u32, h);
        for f in 0..frames {
            for y in 0..h {
                for x in 0..w {
                    img.put_pixel(x + f as u32 * w, y, image::Rgba([f as u8 * 100, 0, 0, 255]));
                }
            }
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    const MANIFEST: &str = r#"{
        "format_version": 1,
        "characters": {
            "tester": {
                "frame_width": 4, "frame_height": 6,
                "animations": {
                    "idle": {"file": "red.png", "frames": 2, "fps": 4, "loop": true}
                }
            }
        }
    }"#;

    #[test]
    fn loads_from_embedded_bytes() {
        let png = sheet_png(2, 4, 6);
        let sheets: Vec<(&str, &[u8])> = vec![("red.png", png.as_slice())];
        let library = SpriteLibrary::from_embedded(MANIFEST, &sheets).unwrap();
        let character = library.character("tester").unwrap();
        assert_eq!(character.clip("idle").unwrap().frames, 2);
        let f0 = character.frame("idle", 0).unwrap();
        let f1 = character.frame("idle", 1).unwrap();
        assert_eq!(f0.get_pixel(0, 0).0[0], 0);
        assert_eq!(f1.get_pixel(0, 0).0[0], 100);
        // index wrapping is safe, not a panic
        assert_eq!(character.frame("idle", 5).unwrap().get_pixel(0, 0).0[0], 100);
        assert!(character.frame("missing", 0).is_none());
    }

    #[test]
    fn loads_from_disk_and_rejects_bad_shapes() {
        let dir = std::env::temp_dir().join(format!("insync-sprite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("red.png"), sheet_png(2, 4, 6)).unwrap();
        std::fs::write(dir.join("manifest.json"), MANIFEST).unwrap();
        let library = SpriteLibrary::load(&dir.join("manifest.json")).unwrap();
        assert!(library.character("tester").is_some());

        std::fs::write(dir.join("short.png"), sheet_png(1, 4, 6)).unwrap();
        std::fs::write(
            dir.join("m.json"),
            r#"{"format_version":1,"characters":{"t":{"frame_width":4,"frame_height":6,
               "animations":{"idle":{"file":"short.png","frames":2,"fps":1,"loop":true}}}}}"#,
        )
        .unwrap();
        assert!(matches!(
            SpriteLibrary::load(&dir.join("m.json")),
            Err(SpriteError::SheetShape { .. })
        ));

        std::fs::write(dir.join("v.json"), r#"{"format_version": 9, "characters": {}}"#).unwrap();
        assert!(matches!(
            SpriteLibrary::load(&dir.join("v.json")),
            Err(SpriteError::ManifestVersion(9))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
