# crabsnap-ui

The UI framework, as an independent workspace: the application depends on it,
never the other way round. Build and test from this directory.

## Crates

| crate | role |
|---|---|
| `ui-core` | retained tree, single-pass layout, draw list, damage, hit testing, frame clock |
| `ui-style` | style objects, common plane, materials, motion tables |
| `ui-design` | design-system data: MD3, Fluent, WinUI, Apple token tables |
| `ui-text` | font stack, fallback chain, line breaking, bidi, subpixel phases |
| `ui-text-fontique` | platform font provider, shaping, glyph rasterizer, atlas |
| `ui-paint` | `Primitives` → premultiplied pixels (CPU rasterizer, the reference) |
| `ui-paint-gpu` | GPU backend: EGL + GLES, SDF shapes, glyph atlas, effects, dma-buf targets |
| `ui-gpu-egl` | EGL context and dma-buf plumbing |
| `ui-platform` | app loop, view adapter, presentation targets, keyboard and pointer routing |
| `ui-platform-wayland` | XDG window, `wl_shm` and dma-buf buffers, fractional scale, IME |
| `ui-widgets` | control primitives shared by the control sets |
| `ui-widgets-md3` | Material 3 controls |
| `ui-widgets-fluent` | Fluent (WinUI) controls |
| `ui-widgets-glass` | Apple-style glass controls |
| `ui-i18n` | Fluent catalogs; message keys are compile-checked |
| `ui-color-hct` | HCT colour space, palettes, contrast solving |
| `ui-demo`, `ui-gallery` | the runnable demos; `ui-gallery` is the reference page set |

## Running

```bash
cargo build
cargo test --workspace --features ui-text-fontique/system
cargo run --release -p ui-gallery --example gallery
```

Graphical checks run inside a nested session (`../scripts/nested-session.sh`),
so they cannot disturb the compositor in use.

## Documentation

`docs/src/` holds the design documents: the engine's working model, the layout
and text contracts, the control sets and their parity notes, and the platform
boundary. Start at [`../docs/src/SUMMARY.md`](../docs/src/SUMMARY.md).
