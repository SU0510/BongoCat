# ADR-0090: InSync sprite pet rendering replaces the Live2D overlay path

- Status: accepted
- Date: 2026-10-11
- Depends on: ADR-0089, ADR-0030

## Decision

InSync renders its desk pet with **pre-rendered sprite-sheet frames** composited
on the CPU inside a dedicated per-platform window, implemented in
`insync-pet-window`. It does not reuse `bongocat-overlay`'s
`ProductOverlaySession`.

### Why not reuse bongocat-overlay

`ProductOverlaySession::start` requires a `RuntimeClient`, an `InputProducer`,
a `CursorProducer` and a `GamepadAxisProducer`. Using it unchanged would force
InSync to start a BongoCat runtime (Live2D model management) and the platform
global-input services (CGEventTap / Raw Input) whose only product purpose is
the keyboard cat. InSync must not monitor global input at all (privacy), and
carrying the Cubism runtime for a static sprite is unjustified cost. Making all
of those dependencies optional inside `bongocat-overlay` would refactor roughly
1.5k lines of proven platform session code for a product that does not need the
features that code exists for (model switching handshake, hover/idle input
presentation, input diagnostics), and most of that refactor could not be
verified on Windows from this environment.

### The replacement

- macOS: borderless non-activating `NSPanel`, transparent, floating level,
  `movableByWindowBackground`, `ignoresMouseEvents` for click-through; frames
  are premultiplied RGBA buffers pushed to a `CALayer` as `CGImage`s on the
  main thread. The panel setup mirrors the proven values used by
  `bongocat-overlay/src/macos/renderer.rs` (level, collection behavior, opaque
  flags) minus the Metal surface.
- Windows: `WS_POPUP` layered window with per-pixel alpha via
  `UpdateLayeredWindow` (premultiplied ARGB), `WS_EX_TRANSPARENT |
  WS_EX_LAYERED` for click-through, `HTCAPTION` on hit-test for native drag,
  `WM_CONTEXTMENU` for the context menu, per-monitor DPI via
  `GetDpiForWindow`. This is the classic desktop-pet path: no D3D11 swapchain,
  no DirectComposition, alpha behavior guaranteed by the OS.
- Both backends live behind one safe API in `insync-pet-window`
  (`PetWindowHandle`), with `unsafe` confined to those two platform modules and
  documented caller invariants, per the workspace unsafe policy.
- Rendering is a straight blit: the animation state machine
  (`insync-sprite`) selects a frame; the window backend scales it with
  nearest-neighbor into a premultiplied buffer. At pet sizes (≤400 px) and pet
  frame rates (≤15 fps) CPU cost is negligible, so no GPU pipeline is
  warranted (ADR-0030 ladder: OS capability before GPU dependencies).
- Multi-monitor, DPI, placement clamping and position persistence are
  implemented in `insync-pet-window` and persisted by the app through the same
  window-state JSON store pattern BongoCat uses.

### Consequences

- InSync has no dependency on Cubism, D3D11 or Metal and no global input hook.
- Sprite assets are plain PNGs plus a JSON manifest; adding animations never
  touches rendering code.
- Non-integer scaling is handled by pre-upscaling assets (4x) at generation
  time; the window always blits at device-pixel resolution to stay crisp.
- The pet window is the only product surface that leaves GPUI; all panels
  (chat, hub, settings) remain GPUI windows.
