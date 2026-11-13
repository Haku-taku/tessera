//! The showcase's four tabs: the reference app's own pages, built from this crate's controls.

use ui_core::geom::{Color, Insets};
use ui_core::layout::{Anchor, Side};
use ui_core::{Decl, Id, NodeKind, Weight};
use ui_platform::{Container, Element, from_decl};
use ui_style::Style;
use ui_widgets::text::Text;
use ui_widgets::{Cx, IconSource};
use ui_widgets_glass::{
    GlassButton, GlassButtonParams, GlassButtonStyle, GlassCard, GlassCardParams, GlassChip,
    GlassChipParams, GlassDialog, GlassDialogAction, GlassDialogParams, GlassListTile,
    GlassListTileParams, GlassMenu, GlassMenuEntry, GlassMenuParams, GlassProgress,
    GlassProgressParams, GlassSegment, GlassSegmentedControl, GlassSlider, GlassSwitch,
    GlassTextField, GlassTextFieldParams,
};

use crate::app::{CATEGORIES, LiquidGlassApp, Tab, ink, ink_muted, pack};

/// One page of the current tab.
pub(crate) fn page(app: &LiquidGlassApp, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let style = pack(cx);
    let mut col = Container::new(Id::str("page"), NodeKind::Column)
        .padding(Insets::new(28.0, 28.0, 28.0, 24.0))
        .gap(20.0)
        .cross_align(0.0);
    for block in match app.tab {
        Tab::Explore => explore(app, cx, &style),
        Tab::Widgets => widgets(app, cx, &style),
        Tab::Demos => demos(app, cx, &style),
        Tab::Examples => examples(app, cx, &style),
    } {
        col = col.child(block);
    }
    Box::new(col)
}

fn explore(app: &LiquidGlassApp, cx: &mut Cx<'_, '_>, style: &Style) -> Vec<Box<dyn Element>> {
    let mut out = vec![
        text(
            Id::str("explore/title"),
            "Liquid Glass Widgets",
            34.0,
            ink(),
            720.0,
        ),
        text(
            Id::str("explore/sub"),
            "The iOS 26 glass surface, drawn by this engine: refraction, lensing and light.",
            16.0,
            ink_muted(),
            620.0,
        ),
    ];
    // The hero card opens the player, as the reference's featured card does.
    let open = button(
        cx,
        style,
        Id::str("explore/music"),
        "play.fill",
        "Open the player",
    );
    out.push(card(
        Id::str("explore/hero"),
        style,
        10.0,
        vec![
            text(
                Id::str("explore/hero-title"),
                "Apple Music",
                22.0,
                ink(),
                480.0,
            ),
            text(
                Id::str("explore/hero-sub"),
                "A player built entirely from glass: cards, sliders and capsules.",
                14.0,
                ink_muted(),
                480.0,
            ),
            Box::new(
                Container::new(Id::str("explore/hero-row"), NodeKind::Row)
                    .gap(10.0)
                    .cross_align(0.0)
                    .child(open),
            ),
        ],
    ));
    out.push(text(
        Id::str("explore/what"),
        "What's inside",
        20.0,
        ink(),
        620.0,
    ));
    let mut cards = Vec::new();
    for (k, (name, blurb)) in [
        ("Containers", "Cards and containers: one glass, one solid."),
        ("Feedback", "Progress, chips and toast surfaces."),
        ("Interactive", "Buttons, switches and sliders with jelly."),
    ]
    .into_iter()
    .enumerate()
    {
        cards.push(card(
            Id::of(("explore/tile", k)),
            style,
            6.0,
            vec![
                text(Id::of(("explore/name", k)), name, 17.0, ink(), 260.0),
                text(
                    Id::of(("explore/blurb", k)),
                    blurb,
                    13.0,
                    ink_muted(),
                    260.0,
                ),
            ],
        ));
    }
    out.push(Box::new(
        Container::new(Id::str("explore/cards"), NodeKind::Row)
            .gap(16.0)
            .cross_align(0.0)
            .children(cards),
    ));
    let _ = app;
    out
}

fn widgets(app: &LiquidGlassApp, cx: &mut Cx<'_, '_>, style: &Style) -> Vec<Box<dyn Element>> {
    let segments: Vec<GlassSegment> = CATEGORIES
        .iter()
        .enumerate()
        .map(|(k, name)| GlassSegment::label(Id::of(("cat", k)), *name))
        .collect();
    let id = Id::str("cats");
    let control =
        GlassSegmentedControl::new(id, segments, app.category, 720.0, &mut cx.at(id), style);
    let mut out = vec![cx.element(Box::new(control))];
    out.extend(category(app, cx, style));
    out
}

fn category(app: &LiquidGlassApp, cx: &mut Cx<'_, '_>, style: &Style) -> Vec<Box<dyn Element>> {
    let mut out = Vec::new();
    match CATEGORIES[app.category.min(CATEGORIES.len() - 1)] {
        "Containers" => {
            for (k, solid) in [(0, false), (1, true)] {
                let name = if solid { "Solid card" } else { "Glass card" };
                let mut params = GlassCardParams::card(Id::of(("cont/card", k)));
                if solid {
                    params = params.solid();
                }
                out.push(card_with(
                    params,
                    style,
                    6.0,
                    vec![
                        text(Id::of(("cont/name", k)), name, 17.0, ink(), 300.0),
                        text(
                            Id::of(("cont/blurb", k)),
                            "The reference's own inset and corner, over the photo behind.",
                            13.0,
                            ink_muted(),
                            300.0,
                        ),
                    ],
                ));
                let _ = params;
            }
        }
        "Feedback" => {
            let ring = GlassProgress::new(
                GlassProgressParams::ring(Id::str("ring"), Some(app.slider)),
                cx,
                style,
            );
            let bar = GlassProgress::new(
                GlassProgressParams::bar(Id::str("bar"), Some(app.slider)),
                cx,
                style,
            );
            let chip = GlassChip::new(
                GlassChipParams::new(Id::str("chip"), "Selected chip")
                    .selected(app.chip_on)
                    .deletable(),
                cx,
                style,
            );
            out.push(card(
                Id::str("feedback/card"),
                style,
                16.0,
                vec![
                    text(Id::str("feedback/label"), "Progress", 15.0, ink(), 400.0),
                    cx.element(Box::new(ring)),
                    cx.element(Box::new(bar)),
                    cx.element(Box::new(chip)),
                ],
            ));
        }
        "Input" => {
            let field = GlassTextField::new(
                GlassTextFieldParams::new(Id::str("field"), &app.text, 360.0)
                    .placeholder("Your name")
                    .leading(IconSource::sf("person")),
                cx,
                style,
            );
            out.push(card(
                Id::str("input/card"),
                style,
                14.0,
                vec![
                    text(
                        Id::str("input/label"),
                        "Glass text field",
                        15.0,
                        ink(),
                        400.0,
                    ),
                    cx.element(Box::new(field)),
                    text(
                        Id::str("input/echo"),
                        format!("The app owns the buffer: {:?}", app.text),
                        13.0,
                        ink_muted(),
                        420.0,
                    ),
                ],
            ));
        }
        "Interactive" => {
            let toggle = GlassSwitch::new(
                Id::str("toggle"),
                app.toggle_on,
                "Wi-Fi",
                &mut cx.at(Id::str("toggle")),
                style,
            );
            let slider = GlassSlider::new(
                Id::str("slider"),
                app.slider,
                "Volume",
                &mut cx.at(Id::str("slider")),
                style,
            );
            let filled = button(cx, style, Id::of(("btn", 0)), "", "Filled");
            let plain = button_style(
                cx,
                style,
                Id::of(("btn", 1)),
                "",
                "Plain",
                GlassButtonStyle::Transparent,
            );
            out.push(card(
                Id::str("inter/card"),
                style,
                16.0,
                vec![
                    text(Id::str("inter/label"), "Controls", 15.0, ink(), 400.0),
                    Box::new(
                        Container::new(Id::str("inter/row"), NodeKind::Row)
                            .gap(10.0)
                            .cross_align(0.0)
                            .child(filled)
                            .child(plain),
                    ),
                    cx.element(Box::new(toggle)),
                    cx.element(Box::new(slider)),
                ],
            ));
        }
        "Overlays" => {
            let anchor = Id::str("menu/anchor");
            let menu_button = button(cx, style, anchor, "ellipsis.circle", "Open menu");
            let items = [
                GlassMenuEntry::item(Id::str("menu/copy"), "Copy")
                    .icon(IconSource::sf("doc.on.doc")),
                GlassMenuEntry::item(Id::str("menu/share"), "Share")
                    .icon(IconSource::sf("square.and.arrow.up")),
                GlassMenuEntry::divider(Id::str("menu/rule")),
                GlassMenuEntry::item(Id::str("menu/delete"), "Delete")
                    .icon(IconSource::sf("trash"))
                    .destructive(),
            ];
            let menu = GlassMenu::new(
                GlassMenuParams {
                    id: Id::str("menu"),
                    anchor: Anchor::Node(anchor),
                    side: Side::Below,
                    entries: &items,
                    open: app.menu_open,
                },
                cx,
                style,
            );
            out.push(card(
                Id::str("overlay/card"),
                style,
                14.0,
                vec![
                    text(Id::str("overlay/label"), "Menus", 15.0, ink(), 400.0),
                    Box::new(
                        Container::new(Id::str("overlay/row"), NodeKind::Row)
                            .cross_align(0.0)
                            .child(menu_button),
                    ),
                    cx.element(Box::new(menu)),
                ],
            ));
        }
        _ => {
            let mut tiles: Vec<Box<dyn Element>> = Vec::new();
            for (k, (title, subtitle)) in [
                ("Album", "Artwork behind frosted glass"),
                ("Playlist", "Twelve tracks, one surface"),
                ("Artist", "Everything in one list"),
            ]
            .into_iter()
            .enumerate()
            {
                let tile = GlassListTile::new(
                    GlassListTileParams::new(Id::of(("tile", k)), title)
                        .subtitle(subtitle)
                        .standalone(),
                    cx,
                    style,
                );
                tiles.push(cx.element(Box::new(tile)));
            }
            out.push(card(Id::str("surfaces/card"), style, 10.0, tiles));
        }
    }
    let _ = app;
    out
}

fn demos(app: &LiquidGlassApp, cx: &mut Cx<'_, '_>, style: &Style) -> Vec<Box<dyn Element>> {
    let show = button(cx, style, Id::str("dialog/show"), "", "Show dialog");
    let mut out = vec![
        text(Id::str("demos/title"), "Demos", 26.0, ink(), 620.0),
        text(
            Id::str("demos/sub"),
            "Each card is a live control: press it and the app answers.",
            15.0,
            ink_muted(),
            620.0,
        ),
        card(
            Id::str("demos/card"),
            style,
            14.0,
            vec![
                text(Id::str("demos/label"), "Dialogs", 15.0, ink(), 400.0),
                Box::new(
                    Container::new(Id::str("demos/row"), NodeKind::Row)
                        .cross_align(0.0)
                        .child(show),
                ),
            ],
        ),
    ];
    if app.dialog_open {
        let actions = vec![
            GlassDialogAction::new(Id::str("dialog/cancel"), "Cancel"),
            GlassDialogAction::new(Id::str("dialog/ok"), "Delete").destructive(),
        ];
        let dialog = GlassDialog::new(
            GlassDialogParams::new(
                Id::str("dialog"),
                "Delete this album?",
                "It will be removed from every device signed in to this account.",
                actions,
            ),
            cx,
            style,
        );
        out.push(cx.element(Box::new(dialog)));
    }
    out
}

fn examples(app: &LiquidGlassApp, cx: &mut Cx<'_, '_>, style: &Style) -> Vec<Box<dyn Element>> {
    let tracks = ["Morning Light", "Glass Houses", "Refraction"];
    let mut list: Vec<Box<dyn Element>> = Vec::new();
    for (k, name) in tracks.iter().enumerate() {
        let tile = GlassListTile::new(
            GlassListTileParams::new(Id::of(("track", k)), name).subtitle("Liquid Glass Sessions"),
            cx,
            style,
        );
        list.push(cx.element(Box::new(tile)));
    }
    let playing = if app.playing {
        "pause.fill"
    } else {
        "play.fill"
    };
    let play = button(cx, style, Id::str("music/play"), playing, "");
    let slider = GlassSlider::new(
        Id::str("music/progress"),
        app.progress,
        "0:42",
        &mut cx.at(Id::str("music/progress")),
        style,
    );
    vec![
        text(Id::str("music/title"), "Now Playing", 26.0, ink(), 620.0),
        card(
            Id::str("music/card"),
            style,
            18.0,
            vec![
                text(
                    Id::str("music/track"),
                    tracks[app.track.min(tracks.len() - 1)],
                    22.0,
                    ink(),
                    420.0,
                ),
                text(
                    Id::str("music/artist"),
                    "Liquid Glass Sessions",
                    14.0,
                    ink_muted(),
                    420.0,
                ),
                cx.element(Box::new(slider)),
                Box::new(
                    Container::new(Id::str("music/row"), NodeKind::Row)
                        .cross_align(0.0)
                        .child(play),
                ),
            ],
        ),
        text(Id::str("music/upnext"), "Up next", 18.0, ink(), 420.0),
        Box::new(
            Container::new(Id::str("music/list"), NodeKind::Column)
                .gap(10.0)
                .cross_align(0.0)
                .children(list),
        ),
    ]
}

/// A glass card whose insides stay live.
fn card(id: Id, style: &Style, gap: f32, children: Vec<Box<dyn Element>>) -> Box<dyn Element> {
    card_with(GlassCardParams::card(id), style, gap, children)
}

/// The same, with the caller's own surface: solid where the sample asks for it.
fn card_with(
    params: GlassCardParams,
    style: &Style,
    gap: f32,
    children: Vec<Box<dyn Element>>,
) -> Box<dyn Element> {
    let surface = GlassCard::new(params, style).resolve();
    Box::new(Container::from_decl(surface).gap(gap).children(children))
}

/// A labelled capsule with an optional SF Symbol.
fn button(
    cx: &mut Cx<'_, '_>,
    style: &Style,
    id: Id,
    symbol: &str,
    label: &str,
) -> Box<dyn Element> {
    button_style(cx, style, id, symbol, label, GlassButtonStyle::Filled)
}

fn button_style(
    cx: &mut Cx<'_, '_>,
    style: &Style,
    id: Id,
    symbol: &str,
    label: &str,
    kind: GlassButtonStyle,
) -> Box<dyn Element> {
    let icon = (!symbol.is_empty()).then(|| IconSource::sf(symbol));
    let params = if label.is_empty() {
        GlassButtonParams::icon(id, icon.unwrap_or_else(|| IconSource::sf("circle")))
    } else {
        GlassButtonParams::labelled(id, icon, label)
    }
    .style(kind);
    let control = GlassButton::new(params, &mut cx.at(id), style);
    cx.element(Box::new(control))
}

/// A line of text as an element.
fn text(id: Id, body: impl Into<String>, size: f32, colour: Color, max_w: f32) -> Box<dyn Element> {
    from_decl(label(id, body, size, colour, max_w))
}

fn label(id: Id, body: impl Into<String>, size: f32, colour: Color, max_w: f32) -> Decl {
    let mut text = Text::new(id, body, size, colour).max_width(max_w);
    text.weight = Weight::REGULAR;
    text.resolve()
}
