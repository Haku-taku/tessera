// The materials page: a settings card on the left, a draggable stage on the right.

use crate::shell::*;
use ui_core::Sense;
use ui_widgets::Scroll;
use ui_widgets_glass::tuning::{self, Knob};

/// The four blocks the stage carries: two glass presets, acrylic and plain.
pub(crate) const MATERIAL_BLOCKS: usize = 4;
pub(crate) const MATERIAL_PAGE_H: f32 = 800.0;
const SETTINGS_W: f32 = 360.0;
const ROW_GAP: f32 = 20.0;
const BLOCK_W: f32 = 250.0;
const BLOCK_H: f32 = 156.0;
/// Stage size, so a drag can be clamped to it while the page is not laid out yet.
const STAGE_W: f32 = CONTENT_W - SETTINGS_W - ROW_GAP;
const STAGE_H: f32 = MATERIAL_PAGE_H;
const STAGE_PAD: f32 = 16.0;

/// What a block is made of: the two glass blocks differ by preset, not by shader.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MaterialKind {
    GlassPanel,
    GlassLens,
    Acrylic,
    Plain,
}

pub(crate) const MATERIAL_KINDS: [MaterialKind; MATERIAL_BLOCKS] = [
    MaterialKind::GlassPanel,
    MaterialKind::GlassLens,
    MaterialKind::Acrylic,
    MaterialKind::Plain,
];

/// One adjustable parameter, named once so the slider, the state and the material agree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Param {
    Sigma,
    BodyMix,
    Thickness,
    Refractive,
    Luminosity,
    Noise,
}

impl Param {
    /// The block's sliders, in the order the settings card lists them.
    pub(crate) fn of(kind: MaterialKind) -> &'static [Param] {
        match kind {
            MaterialKind::GlassPanel | MaterialKind::GlassLens => &[
                Param::Sigma,
                Param::BodyMix,
                Param::Thickness,
                Param::Refractive,
            ],
            MaterialKind::Acrylic => &[
                Param::Sigma,
                Param::BodyMix,
                Param::Luminosity,
                Param::Noise,
            ],
            MaterialKind::Plain => &[Param::Sigma],
        }
    }

    /// Slider labels are untranslated glyph-free names: this page tunes the engine, not a locale.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Param::Sigma => "sigma",
            Param::BodyMix => "body mix",
            Param::Thickness => "thickness",
            Param::Refractive => "refractive",
            Param::Luminosity => "luminosity",
            Param::Noise => "noise",
        }
    }

    /// The 0..1 slider value mapped into the parameter's real unit.
    fn real(self, t: f32) -> f32 {
        match self {
            Param::Sigma => t * 24.0,
            Param::BodyMix => t * 0.8,
            Param::Thickness => t * 40.0,
            Param::Refractive => 1.0 + t * 0.8,
            Param::Luminosity => t,
            Param::Noise => t * 0.08,
        }
    }
}

/// Everything the page owns: each block's knobs and where the finger left it.
pub(crate) struct MaterialState {
    /// Normalised 0..1 per parameter, indexed by `MATERIAL_KINDS`.
    value: [[f32; 6]; MATERIAL_BLOCKS],
    /// Normalised slider positions for the glass controls' knobs, in `Knob::ALL` order.
    glass: [f32; Knob::COUNT],
    /// The two presets the glass blocks start from, resolved once.
    panel: ui_core::draw::LiquidGlass,
    lens: ui_core::draw::LiquidGlass,
    pub(crate) pos: [(f32, f32); MATERIAL_BLOCKS],
    /// Where a drag started, so `Moved`'s total travel lands on the right base.
    drag_from: [Option<(f32, f32)>; MATERIAL_BLOCKS],
}

impl Default for MaterialState {
    fn default() -> Self {
        // The panel preset is the thick one; the lens preset is the clear one.
        // Each glass block starts on its preset's own numbers, so a drag departs from the
        // material the controls actually wear.
        let value = [
            [0.42, 0.6625, 0.25, 0.625, 0.0, 0.0],
            [0.42, 0.0, 0.15, 0.125, 0.0, 0.0],
            [0.66, 1.00, 0.0, 0.0, 0.35, 0.25],
            [0.33, 0.0, 0.0, 0.0, 0.0, 0.0],
        ];
        // Two rows centred on the stage: the same air above and below, room to drag anywhere.
        let top = (MATERIAL_PAGE_H - BLOCK_H * 2.0 - 40.0) * 0.5;
        let pos = [
            (STAGE_PAD, top),
            (STAGE_PAD + BLOCK_W + 24.0, top),
            (STAGE_PAD, top + BLOCK_H + 40.0),
            (STAGE_PAD + BLOCK_W + 24.0, top + BLOCK_H + 40.0),
        ];
        Self {
            value,
            glass: glass_sliders(),
            panel: preset(ui_widgets_glass::material::Control::Container),
            lens: preset(ui_widgets_glass::material::Control::Slider),
            pos,
            drag_from: [None; MATERIAL_BLOCKS],
        }
    }
}

/// The preset a control wears under the gallery's own pack.
fn preset(control: ui_widgets_glass::material::Control) -> ui_core::draw::LiquidGlass {
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    ui_widgets_glass::material::glass_recipe(&pack, control)
}

/// The kit the switch and slider wear, so the glass knobs start on its own values.
fn glass_kit() -> ui_core::draw::LiquidGlass {
    preset(ui_widgets_glass::material::Control::Switch)
}

/// Every glass knob's slider position at the recipe's own value.
fn glass_sliders() -> [f32; Knob::COUNT] {
    let kit = glass_kit();
    let mut out = [0.0; Knob::COUNT];
    for (slot, knob) in Knob::ALL.iter().enumerate() {
        out[slot] = tuning::normalised(*knob, tuning::default_of(*knob, &kit));
    }
    out
}

impl MaterialState {
    fn slot(param: Param) -> usize {
        match param {
            Param::Sigma => 0,
            Param::BodyMix => 1,
            Param::Thickness => 2,
            Param::Refractive => 3,
            Param::Luminosity => 4,
            Param::Noise => 5,
        }
    }

    pub(crate) fn get(&self, k: usize, param: Param) -> f32 {
        self.value[k][Self::slot(param)]
    }

    pub(crate) fn set(&mut self, k: usize, param: Param, v: f32) {
        self.value[k][Self::slot(param)] = v.clamp(0.0, 1.0);
    }

    /// The resolved material, its blur and its body wash for one block.
    fn resolve(&self, k: usize) -> (BackdropMaterial, f32, Color) {
        let sigma = Param::Sigma.real(self.get(k, Param::Sigma));
        let alpha = Param::BodyMix.real(self.get(k, Param::BodyMix));
        match MATERIAL_KINDS[k] {
            MaterialKind::Plain => (
                BackdropMaterial::Plain,
                sigma,
                Color::rgba(1.0, 1.0, 1.0, 0.0),
            ),
            MaterialKind::Acrylic => (
                BackdropMaterial::Acrylic(Acrylic {
                    // Acrylic's wash stops where glass's body starts: half the slider's range.
                    tint: Color::rgba(0.98, 0.98, 0.98, alpha * 0.5),
                    // A luminance target, not a wash colour: white would bleach the backdrop.
                    luminosity: Color::rgba(
                        0.5,
                        0.5,
                        0.5,
                        Param::Luminosity.real(self.get(k, Param::Luminosity)),
                    ),
                    noise: Param::Noise.real(self.get(k, Param::Noise)),
                    // The demo never becomes a panel, so it has no stand-in to name.
                    fallback: Color::TRANSPARENT,
                }),
                sigma,
                Color::TRANSPARENT,
            ),
            kind => {
                let base = match kind {
                    MaterialKind::GlassLens => self.lens,
                    _ => self.panel,
                };
                (
                    BackdropMaterial::LiquidGlass(ui_core::draw::LiquidGlass {
                        body: ui_core::draw::GlassBody {
                            mix: alpha,
                            ..base.body
                        },
                        thickness: Param::Thickness.real(self.get(k, Param::Thickness)),
                        refractive_index: Param::Refractive.real(self.get(k, Param::Refractive)),
                        ..base
                    }),
                    sigma,
                    Color::TRANSPARENT,
                )
            }
        }
    }
}

pub(crate) fn material_block_id(k: usize) -> Id {
    Id::of(("material-block", k))
}

pub(crate) fn material_param_id(k: usize, param: Param) -> Id {
    Id::of(("material-param", k, param.label()))
}

/// Which block a slider reports for, if any.
pub(crate) fn material_param_of(on: Id) -> Option<(usize, Param)> {
    (0..MATERIAL_BLOCKS).find_map(|k| {
        Param::of(MATERIAL_KINDS[k])
            .iter()
            .copied()
            .find(|p| material_param_id(k, *p) == on)
            .map(|p| (k, p))
    })
}

pub(crate) fn glass_knob_id(knob: Knob) -> Id {
    Id::of(("glass-knob", knob.label()))
}

/// A slider's caption: its name and the number it currently sits on.
fn caption(name: &str, value: f32) -> String {
    format!("{name}   {value:.2}")
}

/// Which glass knob a slider reports for, if any.
pub(crate) fn glass_knob_of(on: Id) -> Option<Knob> {
    Knob::ALL.iter().copied().find(|k| glass_knob_id(*k) == on)
}

impl MaterialState {
    pub(crate) fn glass(&self, knob: Knob) -> f32 {
        self.glass[Knob::ALL.iter().position(|k| *k == knob).unwrap_or(0)]
    }

    /// Move one glass knob: the slider position is stored and the override pinned.
    pub(crate) fn set_glass(&mut self, knob: Knob, t: f32) {
        let t = t.clamp(0.0, 1.0);
        self.glass[Knob::ALL.iter().position(|k| *k == knob).unwrap_or(0)] = t;
        tuning::set(knob, Some(tuning::denormalised(knob, t)));
    }
}

/// Which block a drag reports for, if any.
pub(crate) fn material_block_of(on: Id) -> Option<usize> {
    (0..MATERIAL_BLOCKS).find(|k| material_block_id(*k) == on)
}

/// Apply a drag step: `Moved` carries the whole travel since the press.
pub(crate) fn drag_block(state: &mut MaterialState, k: usize, started: bool, dx: f32, dy: f32) {
    if started {
        state.drag_from[k] = Some(state.pos[k]);
    }
    let from = state.drag_from[k].unwrap_or(state.pos[k]);
    state.pos[k] = (
        (from.0 + dx).clamp(STAGE_PAD, STAGE_W - BLOCK_W - STAGE_PAD),
        (from.1 + dy).clamp(STAGE_PAD, STAGE_H - BLOCK_H - STAGE_PAD),
    );
}

pub(crate) fn end_drag(state: &mut MaterialState, k: usize) {
    state.drag_from[k] = None;
}

fn wash(id: Id, x: f32, y: f32, w: f32, h: f32, top: Color, bottom: Color) -> Decl {
    Decl::container(id, NodeKind::Overlay)
        .offset(x, y)
        .size(SizeClass::fixed(w, h))
        .background(Color::TRANSPARENT, Corners::ZERO)
        .gradient(top, bottom, GradientAxis::Vertical)
}

/// One block on the stage: the material itself, its caption, and a drag handle.
fn block(state: &MaterialState, k: usize) -> Decl {
    let (material, sigma, body) = state.resolve(k);
    let kind = MATERIAL_KINDS[k];
    let name = match kind {
        MaterialKind::GlassPanel => "glass · panel",
        MaterialKind::GlassLens => "glass · lens",
        MaterialKind::Acrylic => "acrylic",
        MaterialKind::Plain => "plain",
    };
    Decl::container(material_block_id(k), NodeKind::Overlay)
        .offset(state.pos[k].0, state.pos[k].1)
        .size(SizeClass::fixed(BLOCK_W, BLOCK_H))
        .background(body, Corners::all(24.0))
        .backdrop_blur(sigma)
        .backdrop_material(material)
        .border(Color::rgba(1.0, 1.0, 1.0, 0.28), 1.0)
        .sense(Sense::HOVER | Sense::DRAG)
        .child(label(
            material_block_id(k).with("label"),
            name,
            14.0,
            Color::rgba(1.0, 1.0, 1.0, 0.92),
            BLOCK_W - 40.0,
        ))
}

/// The stage: a dark scene the materials can read against, plus the four dragged blocks.
fn stage(state: &MaterialState) -> Decl {
    let w = STAGE_W;
    let h = STAGE_H;
    let mut scene = Decl::container(Id::str("material-stage"), NodeKind::Box)
        .size(SizeClass::fixed(w, h))
        .background(Color::from_rgba8(12, 14, 20, 255), Corners::all(14.0))
        .child(wash(
            Id::str("material-wash"),
            0.0,
            0.0,
            w,
            h,
            Color::from_rgba8(24, 54, 110, 255),
            Color::from_rgba8(16, 22, 46, 255),
        ))
        .child(wash(
            Id::str("material-sun"),
            40.0,
            40.0,
            w - 80.0,
            h * 0.34,
            Color::from_rgba8(200, 96, 70, 255),
            Color::from_rgba8(120, 40, 90, 255),
        ))
        .child(wash(
            Id::str("material-teal"),
            70.0,
            h * 0.62,
            w - 140.0,
            h * 0.34,
            Color::from_rgba8(30, 140, 150, 255),
            Color::from_rgba8(20, 60, 110, 255),
        ));
    for k in 0..MATERIAL_BLOCKS {
        scene = scene.child(block(state, k));
    }
    scene
}

/// The settings card: one section per block, each holding only that block's parameters.
fn settings(
    state: &MaterialState,
    style: &dyn StyleKit,
    glass: &dyn StyleKit,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let pack = ui_widgets_md3::pack_of(ui_style::StyleScope::of(&root_env()).appearance);
    let fill = pack.ink("surfaceContainerHighest").unwrap_or(Color::WHITE);
    let ink = pack.ink("onSurfaceVariant").unwrap_or(Color::BLACK);
    // The sections are an element tree, not a declaration: only the element adapter
    // carries a slider's `change()`, so a declaration-built slider would paint and never
    let mut sections = Container::new(Id::str("material-settings/sections"), NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::hug(0.0, f32::INFINITY),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .gap(8.0);
    for k in 0..MATERIAL_BLOCKS {
        let title = match MATERIAL_KINDS[k] {
            MaterialKind::GlassPanel => "Glass · panel",
            MaterialKind::GlassLens => "Glass · lens",
            MaterialKind::Acrylic => "Acrylic",
            MaterialKind::Plain => "Plain",
        };
        sections = sections.child_decl(label(
            Id::of(("material-section", k)),
            title,
            13.0,
            ink,
            SETTINGS_W - 32.0,
        ));
        let mut scoped = cx.at_scope(style_env("md3"));
        for param in Param::of(MATERIAL_KINDS[k]) {
            let t = state.get(k, *param);
            sections = sections.child_decl(label(
                material_param_id(k, *param).with("label"),
                caption(param.label(), param.real(t)),
                12.0,
                ink,
                SETTINGS_W - 32.0,
            ));
            let slider = style.slider(
                &SliderSpec::new(material_param_id(k, *param), t, ""),
                &mut scoped,
            );
            sections = sections.child(scoped.element(slider));
        }
    }
    // The glass controls' own knobs, then a live switch so a change shows immediately.
    sections = sections.child_decl(label(
        Id::str("material-section/glass"),
        "Glass · controls",
        13.0,
        ink,
        SETTINGS_W - 32.0,
    ));
    let mut glass_scope = cx.at_scope(style_env("glass"));
    for knob in Knob::ALL {
        // The caption carries the value in the knob's own unit, so a tuned number can be
        // read off and reported without opening a debugger.
        sections = sections.child_decl(label(
            glass_knob_id(knob).with("label"),
            caption(knob.label(), tuning::denormalised(knob, state.glass(knob))),
            12.0,
            ink,
            SETTINGS_W - 32.0,
        ));
        let slider = glass.slider(
            &SliderSpec::new(glass_knob_id(knob), state.glass(knob), ""),
            &mut glass_scope,
        );
        sections = sections.child(glass_scope.element(slider));
    }
    let preview = glass.toggle(
        &ToggleSpec::new(Id::str("glass-tuning/switch"), true, ""),
        &mut glass_scope,
    );
    let preview = glass_scope.element(preview);
    // The sections outgrow the card, so they scroll: the card keeps its height and the last
    // section stays reachable instead of being clipped away.
    // The live switch sits above the scroll, so a knob moved far down the list still has its
    let scrolled = Scroll::vertical(Id::str("material-settings/scroll"), MATERIAL_PAGE_H - 160.0)
        .no_scrollbar()
        .element_in(cx)
        .child(Box::new(sections));
    Box::new(
        Container::new(Id::str("material-settings"), NodeKind::Column)
            .size(SizeClass::fixed(SETTINGS_W, MATERIAL_PAGE_H))
            .padding(Insets::all(16.0))
            .gap(8.0)
            .background(fill, Corners::all(12.0))
            .child_decl(label(
                Id::str("material-settings/title"),
                "Material settings",
                14.0,
                ink,
                SETTINGS_W - 32.0,
            ))
            .child(preview)
            .child(Box::new(scrolled)),
    )
}

pub(crate) fn materials_page(
    styles: &[(&dyn StyleKit, &'static str, Msg); 3],
    state: &MaterialState,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let md3 = styles
        .iter()
        .find(|(_, tag, _)| *tag == "md3")
        .map(|(kit, _, _)| *kit);
    let style = md3.unwrap_or(styles[0].0);
    let glass = styles
        .iter()
        .find(|(_, tag, _)| *tag == "glass")
        .map(|(kit, _, _)| *kit)
        .unwrap_or(styles[0].0);
    Box::new(
        Container::new(Id::of(("bodyrow", Page::Materials.tag())), NodeKind::Row)
            .size(SizeClass::fixed(CONTENT_W, MATERIAL_PAGE_H))
            .gap(ROW_GAP)
            .child(settings(state, style, glass, cx))
            .child(Box::new(from_decl(stage(state)))),
    )
}
