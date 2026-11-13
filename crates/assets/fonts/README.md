# Vendored fonts

`SFSymbols.ttf` — Apple's SF Symbols font, taken from the reference package
`liquid_glass_widgets/example/assets/fonts/`. It is proprietary Apple material: it
ships here only because this repository is the owner's own working copy, and it
must not be redistributed. The engine itself carries no font; the gallery registers
this file at start-up under `ui_widgets::SF_SYMBOLS_FAMILY`, and the name → codepoint
table in `ui-widgets/src/sf_symbol_codepoints.rs` is generated from its cmap by
`scripts/gen_sf_symbol_codepoints.py`.

`InterVariable.ttf` — the Latin UI face, SIL Open Font License 1.1 (`Inter-LICENSE.txt`,
`rsms/inter` v4.1). Inter is the closest open design to Apple's SF Pro, which is why the
gallery leads its stack with SF Pro and falls back to it; Chinese comes from the system face
behind it (PingFang SC on Apple hosts, Noto Sans SC elsewhere). The gallery registers it
under `TEXT_FAMILY` in `shell.rs`.

`FluentSystemIcons-Regular.ttf` — Microsoft's Fluent System Icons, MIT-licensed
(`microsoft/fluentui-system-icons`, `fonts/FluentSystemIcons-Regular.ttf`). WinUI's
own icon face, Segoe Fluent Icons, cannot be redistributed; this is the same glyph
set Microsoft publishes freely. The gallery registers it under
`ui_widgets_fluent::glyphs::ICON_FAMILY`, and the name → codepoint table in
`ui-widgets-fluent/src/fluent_codepoints.rs` is generated from its cmap by
`scripts/gen_fluent_codepoints.py`.
