# Changelog

## Unreleased

### ✨ Features

- This repository now also carries **InSync**, a new desktop companionship app for couples (a separate product from BongoCat, documented under `docs/insync/`). It ships an original pixel desk pet ("Momo", 13 hand-authored-style animations), a two-person realtime layer (chat, letters, pokes, joint focus sessions, shared todos/notes/anniversaries, presence), local-first daily tools (personal todos, notes, anniversaries, alarms, clock), and a self-hostable Rust server (axum + SQLite) with a versioned realtime event protocol. Desktop builds for macOS and Windows are produced by the new `insync-verify` workflow; server deployment is Docker-based (`docker compose up -d --build`, see `docs/insync/deployment.md`). BongoCat itself is unchanged.
- InSync now versions and releases on its own line, starting at 0.1.0 with `insync-v*` tags, so it no longer shares BongoCat's tag namespace or installer channel. Each release carries the macOS (Apple Silicon) and Windows (x86_64) `insync-app` / `insync-server` binaries plus `SHA256SUMS`, and those binaries are the ones a green `insync-verify` run already built and tested for that commit — nothing is rebuilt for a release. They are bare binaries rather than installers: the macOS builds are unsigned and unnotarized, and the Windows builds may show a SmartScreen prompt.
- InSync adds a second desk-pet character, "Nana", redesigned from a real photo: dark twin braids with the hair combed back from the hairline (no fringe), a rose-pink sleeveless top and a crossbody bag strap, a soft tapered face and a warm ivory skin tone, drawn in the big-head/short-leg chibi proportions the reference calls for, playing the same 13 animations (the braids sway while walking). The InSync settings panel gained a Preferences card with a character picker; the desk pet, header avatar and empty-state illustration all follow the selection, and existing configurations keep Momo.
- Model folders can now contain multiple levels of nested folders. A single model imports directly; folders containing multiple models let you select which to import, then choose conversion modes as needed.
- Added a "Model behavior → Allow overlapping motions" setting, off by default. Different motions can play together with independent timing and fades; newer starts blend over older motions on shared parameters, and motion audio still plays one clip at a time.
- The Windows installer now asks which language to use and offers Simplified Chinese, Traditional Chinese, English, Arabic, Vietnamese, Brazilian Portuguese and Korean. The system language is preselected, with English as the fallback. The choice only changes the installer and uninstaller wizard; the app language is still decided in Settings. A computer that already has BongoCat installed keeps the installer language it stored earlier.

### 🐛 Bug Fixes

- Fixed imported models that showed "The selected model could not be activated" when switching to them and only enabled after several attempts, even though they render correctly. Switching now succeeds on the first attempt.

## 2.2.0 - 2026-10-07

### ✨ Features

- Added a "Model window → Window behavior → Hide when idle" setting.
- Added a "Model window → Window behavior → Idle hide delay" setting.
- Added a "Model window → Window behavior → Hold a modifier key to interact" setting.
- Added an "Input & interaction → Mouse → Force mouse movement" setting.
- Added a "Model behavior → Turn off an expression when triggered again" setting.
- Added renaming support to "Shortcuts → Model behavior shortcuts".

### 🐛 Bug Fixes

- Fixed noticeable soft fringes around model texture edges.
- Fixed misplaced key images in some converted Bongo-Cat-Mver models.

### 🎨 Interface

- Renamed "App & system → Startup & desktop → Open at login" to "Run at startup".

## 2.1.1 - 2026-10-03

### 🐛 Bug Fixes

- Fixed the app crashing on macOS when opening System Settings from the permission prompt.

## 2.1.0 - 2026-10-03

### ✨ Features

- Added a "Show every held key" setting under Input & interaction → Keyboard.
- Improved the macOS Input Monitoring permission flow with a guided panel that walks you through the authorization.

### 🐛 Bug Fixes

- Fixed some BongoCat Mver models failing to import with an "invalid model package" message.
- Fixed BongoCat freezing and quitting on Windows after clicking "Report an issue".
- Fixed the menu bar icon missing on macOS after updating to a new version and restarting.
- Fixed Xbox-mode controllers going unresponsive on Windows once another window took focus.
- Fixed flickering on Windows when a model uses a gamepad mode.
- Fixed dropdown menus in Settings being too narrow to show their options.

### 🌍 Localization

Added Korean.

## 2.0.1 - 2026-09-29

### 🐛 Bug Fixes

- Fixed some models failing to import with an "invalid model package" message.

### 🌍 Localization

Arabic, Vietnamese, Traditional Chinese, and Portuguese are now available.

## 2.0.0 - 2026-09-29

### ⚠️ Upgrade Notice

- BongoCat 2.0.0 uses a new native Rust architecture.
- Uninstall the previous version first, then download and install the new version manually.
- BongoCat is now licensed under the Apache License 2.0 instead of MIT.

### ✨ Features

- Mouse, keyboard and gamepad input can each be ignored separately, and each has a global shortcut.
- Gamepad stick and trigger dead zones can be adjusted.
- Models can switch automatically when a gamepad connects or disconnects, or you can pin the switch to a specific model.
- Model import captures a cover image automatically.
- BongoCat Mver apps can be imported and converted automatically.
- Built-in and imported models can be renamed and given a new cover.
- Random motions or expressions can be played, choosing expressions only, motions only, or both.
- Each model remembers the expression you last used on it, and restores it on the next launch or when you switch back to that model.
- Mouse tracking can also be flipped vertically.
- Logs have an adjustable level and retention, and old logs are cleaned up automatically.
- Window shortcuts and model-behaviour shortcuts have separate switches.

### 🐛 Bug Fixes

- Held keys and gamepad buttons no longer stay stuck after device, lock, sleep and permission changes.
- Switching models or resizing the window no longer leaves the model window black, transparent or flickering.

### 🗑️ Removals

- The "Key release timeout" setting is removed.
- Traditional Chinese, Portuguese and Vietnamese are no longer available.

### 💻 Support Changes

- Supported platforms are Windows 10 1903+ (x64) and macOS 12+ (Intel/Apple Silicon).
- Windows ARM runs the x64 build through emulation.
- **Windows x86, Windows ARM64 and Linux builds are not provided.**
