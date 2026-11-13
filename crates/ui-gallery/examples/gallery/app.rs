// The gallery's app state and its App implementation.

use crate::pages::*;
use crate::shell::*;
use ui_core::timed;
use ui_widgets_glass::{GlassSheetDetent, detent_id_of};

pub(crate) struct Gallery {
    pub(crate) page: Page,
    pub(crate) on: bool,
    pub(crate) value: f32,
    /// Whether the whole gallery renders in the light or the dark appearance. The toolbar's
    /// toggle flips it; nothing else in the demo changes.
    pub(crate) dark: bool,
    /// Each MD3 variant's own on-value, seeded to the requested defaults: icon switches start
    /// off, plain ones on. A click flips just that variant.
    pub(crate) variant_on: [bool; 4],
    /// Each MD3 checkbox's live value; `error` and `disabled` live on the instance.
    pub(crate) checked: [ui_widgets_md3::CheckState; CHECKBOXES],
    /// The Fluent column's three boxes; one value each, the app owns them.
    pub(crate) fchecked: [ui_widgets_fluent::CheckState; 3],
    /// The `IsThreeState` box: a click walks it through all three values.
    pub(crate) ftri: ui_widgets_fluent::CheckState,
    /// Which option of the demo radio group is selected; the group lives in the app.
    pub(crate) radio_sel: usize,
    /// The Fluent column's selection; the control reports, the app decides.
    pub(crate) fradio_sel: usize,
    /// Each tab bar's selected index; a bar never holds its own selection.
    pub(crate) tab_sel: [usize; TAB_BARS],
    /// The Fluent strip's open tabs, as slots of `TAB_MANY`, and which one is picked.
    pub(crate) fluent_tabs: Vec<usize>,
    pub(crate) fluent_tab_sel: usize,
    /// Tooltip demo: the target under the pointer, the hover episode's clock and how long
    /// it has run, and which panel is mounted (open or fading out).
    pub(crate) tip_hover: Option<usize>,
    pub(crate) tip_gen: u32,
    pub(crate) tip_delay: f32,
    pub(crate) tip_shown: Option<usize>,
    pub(crate) tip_close_at: Option<f64>,
    /// Each Fluent info bar's `IsOpen`; a close button reports, the app collapses the bar.
    pub(crate) info_open: [bool; INFO_BARS],
    /// Each toggle button's selection; a button never holds its own.
    pub(crate) btn_sel: [bool; BTN_TOGGLES],
    /// The split button's menu, owned here because the control only reports the half pressed.
    pub(crate) split_open: bool,
    pub(crate) split_presses: u32,
    /// The MD3 menu's picked row; the control reports it and holds no selection.
    pub(crate) md3_menu_picked: Option<usize>,
    /// Each FAB menu's own open state; the app owns it, a click on an item or the anchor moves it.
    pub(crate) fab_open: [bool; FAB_MENUS],
    /// Each filter chip's own selection; the app owns it.
    pub(crate) chip_sel: [bool; CHIP_FILTERS],
    /// The glass chips' selections, and whether the removable one has been deleted.
    pub(crate) chip_glass: [bool; CHIP_GLASS_FILTERS],
    pub(crate) chip_glass_gone: bool,
    /// The segmented page's single and multi selections; the app owns both.
    pub(crate) seg_single: usize,
    pub(crate) seg_multi: [bool; SEGMENTS],
    /// The glass segmented control's own selection, kept apart from MD3's.
    pub(crate) seg_glass: usize,
    /// How many glass buttons have been activated.
    pub(crate) glass_press: u32,
    /// The destination the bar and rail samples share, and the drawer's own choice.
    pub(crate) nav_sel: usize,
    pub(crate) drawer_sel: usize,
    /// The modal drawer sample is open; a scrim press closes it.
    pub(crate) drawer_open: bool,
    /// The rich tooltip sample is open; its action closes it.
    pub(crate) rich_tip_open: bool,
    /// The toolbar member toggled on, and how often a snackbar action was clicked.
    pub(crate) toolbar_sel: Option<usize>,
    /// How many times the floating bars' FABs were pressed.
    pub(crate) toolbar_fab_hits: u32,
    pub(crate) snack_hits: u32,
    /// Whether the floating snackbar sample is showing; the app owns it.
    pub(crate) snack_overlay_visible: bool,
    /// How many times each card variant was activated; a card only reports the event.
    pub(crate) card_hits: [u32; CARD_CLICKS],
    /// Each selectable list row's own selection; the app owns it.
    pub(crate) list_sel: [bool; LIST_SELECT],
    /// How many glass list rows have been activated.
    pub(crate) glass_list_hits: u32,
    /// The generic page's list measures its own rows, and a row that wraps keeps the height
    /// layout gave it after it scrolls away; the table has to outlive the frame.
    pub(crate) list_heights: std::cell::RefCell<ui_widgets::RowHeights>,
    /// Whether the generic page's collapsing banner is declared; the app owns the toggle.
    pub(crate) banner_open: bool,
    /// Whether the hugging card carries its extra line; the app owns this toggle too.
    pub(crate) note_open: bool,
    /// Whether the big sample icon is filled; its `FILL` axis animates toward it.
    pub(crate) icon_filled: bool,
    /// The app bar page's two app-owned facts, and the slot it last reported.
    pub(crate) app_bar_scrolled: bool,
    pub(crate) app_bar_collapsed: bool,
    pub(crate) app_bar_fired: Option<&'static str>,
    /// Whether the rail page's Expressive sample is in its expanded half.
    pub(crate) rail_expanded: bool,
    /// The carousel item the page last reported; each strip scrolls on its own.
    pub(crate) carousel_fired: Option<(usize, usize)>,
    /// Each picker's chosen day, the month it shows and which body it shows.
    pub(crate) dp_value: [Option<ui_widgets_md3::Date>; DATE_PICKERS],
    pub(crate) dp_shown: [ui_widgets_md3::Date; DATE_PICKERS],
    pub(crate) dp_view: [ui_widgets_md3::DatePickerView; DATE_PICKERS],
    /// The modal sample: declared only while open, and holding its own picked date.
    pub(crate) dp_modal: DateModalState,
    /// The range sample: its two dates and the first month of its list.
    pub(crate) dp_range: DateRangeState,
    /// The icon button sample's four toggles, one per style.
    pub(crate) icon_button_on: [bool; 4],
    /// The text-mode input's buffer, and the action it last reported.
    pub(crate) di_text: String,
    pub(crate) di_action: Option<ui_widgets_md3::DateInputAction>,
    /// Each time input's two buffers and its period; the engine edits, the app keeps them.
    pub(crate) tp_hour: [String; TIME_PICKERS],
    pub(crate) tp_minute: [String; TIME_PICKERS],
    pub(crate) tp_period: [ui_widgets_md3::Period; TIME_PICKERS],
    /// The text field page's two live buffers; the engine edits them, the app keeps them.
    pub(crate) tf_value: [String; TF_LIVE],
    /// The Fluent column's one live buffer.
    pub(crate) tf_fluent: String,
    /// The glass column's live buffer.
    pub(crate) tf_glass: String,
    /// The secret field's real buffer; only its marks reach the screen.
    pub(crate) tf_secure: String,
    /// The structured formulas' sources, written back by their editors.
    pub(crate) math_edit_src: [String; MATH_COLUMNS.len()],
    /// The in-place editor's document. The control reports its whole buffer and the app
    /// keeps the bytes; the parsed tree beside it is rebuilt only when they change.
    pub(crate) markdown_src: String,
    pub(crate) markdown_doc: Option<(String, std::sync::Arc<ui_richtext::Document>)>,
    /// The projections cut with that parse, which is the per-frame cost of a long document.
    pub(crate) markdown_prepared: Option<std::sync::Arc<ui_richtext::Prepared>>,
    /// The reading card's sample, parsed under the string it came from.
    pub(crate) markdown_sample: Option<(String, std::sync::Arc<ui_richtext::Document>)>,
    pub(crate) markdown_sample_prepared: Option<std::sync::Arc<ui_richtext::Prepared>>,
    /// The code card's own knobs, from the page's switches. Both cards share them so the
    /// reading and editing states of one card never disagree about its options.
    pub(crate) markdown_code: ui_widgets::CodeOptions,
    /// The search page's live query, and the last action the bar reported.
    pub(crate) search_value: String,
    pub(crate) search_acted: Option<String>,
    /// Whether the search view on that page is open, and the query it holds.
    pub(crate) search_open: bool,
    pub(crate) search_view_value: String,
    /// The glass search bar's query, and the part it last reported.
    pub(crate) glass_search: String,
    pub(crate) glass_search_acted: Option<String>,
    /// The chord the shortcut field captured; the app owns it, the field only reports.
    pub(crate) shortcut: Option<ui_core::KeyChord>,
    /// The dialog action the app was last told about, by its title key; a dialog only reports.
    pub(crate) dialog_picked: Option<Msg>,
    /// How many times a click on a modal's scrim cancelled it.
    pub(crate) dialog_dismissed: u32,
    /// Whether the modal is showing. The app owns it, as it owns every dialog's lifetime.
    pub(crate) dialog_open: bool,
    /// What the showing modal covers: the window, or the panel that hosts it.
    pub(crate) dialog_target: ModalTarget,
    /// Whether the glass action sheet is up, what it last reported, and how often it was dismissed.
    pub(crate) sheet_open: bool,
    pub(crate) sheet_picked: Option<Msg>,
    pub(crate) sheet_dismissed: u32,
    /// Which toast is up, how often its action was pressed, and how often it was swiped away.
    pub(crate) toast_shown: Option<ToastShown>,
    pub(crate) toast_picked: u32,
    pub(crate) toast_dismissed: u32,
    /// Whether the glass modal sheet is up, the detent it rests at, and how often it closed.
    pub(crate) modal_open: bool,
    pub(crate) modal_detent: Option<Msg>,
    /// The MD3 sheet's own trigger state, and how often it closed itself.
    pub(crate) md3_sheet_open: bool,
    pub(crate) md3_sheet_dismissed: u32,
    /// The MD3 side sheet's trigger, and how often its scrim closed it.
    pub(crate) md3_side_sheet_open: bool,
    pub(crate) md3_side_sheet_dismissed: u32,
    pub(crate) modal_dismissed: u32,
    /// The two glass steppers' values, and how many times they reported a step.
    pub(crate) stepper_value: f64,
    pub(crate) stepper_wrap: f64,
    pub(crate) stepper_presses: u32,
    /// Whether the Fluent content dialog is open; the engine holds the closing one itself.
    pub(crate) fdlg_open: bool,
    /// The last item picked on the FAB menu page, as (menu, item).
    pub(crate) fab_picked: Option<(usize, usize)>,
    /// How many times a plain FAB on the FAB page was activated.
    pub(crate) fab_presses: u32,
    /// The Fluent menu's `IsOpen`, and the command it last reported.
    pub(crate) menu_open: bool,
    pub(crate) menu_picked: Option<usize>,
    /// The glass menu's own open flag and the command it last ran.
    pub(crate) gmenu_open: bool,
    pub(crate) gmenu_picked: Option<usize>,
    /// The glass popover's open flag and how often its own "Done" was pressed.
    pub(crate) gpop_open: bool,
    pub(crate) gpop_done: u32,
    /// The Fluent combo box's `IsOpen` and the row it last selected.
    pub(crate) combo_open: bool,
    pub(crate) combo_sel: Option<usize>,
    /// The Fluent navigation view's `IsPaneOpen` and its selected destination.
    pub(crate) nav_open: bool,
    pub(crate) nav_view_sel: usize,
    /// `PaneDisplayMode`: whether an open pane widens in place or covers the content.
    pub(crate) nav_mode: ui_widgets_fluent::PaneDisplayMode,
    pub(crate) md3: Md3Kit,
    /// The context menu's look, resolved through the kit each frame so a running theme
    /// transition blends it like every other surface.
    pub(crate) menu_style: ui_widgets::MenuStyle,
    pub(crate) fluent: FluentKit,
    pub(crate) glass: HigKit,
    pub(crate) dragging: Option<usize>,
    /// The materials page: each block's knobs and where the finger left it.
    pub(crate) materials: MaterialState,
    /// The stepped slider's value and the range slider's pair; both are the app's.
    pub(crate) step_value: f32,
    pub(crate) range: (f32, f32),
    pub(crate) frames: u32,
    /// Appearance being blended from during a theme switch; `None` when settled.
    pub(crate) theme_from: Option<bool>,
    /// Force the transition to restart from 0 on the next frame.
    pub(crate) theme_snap: bool,
    /// Host light/dark watcher (XDG portal on Wayland).
    pub(crate) theme_watch: Option<ui_platform_wayland::PortalThemeWatcher>,
}

/// The Windows hover time before a tooltip opens: `SPI_GETMOUSEHOVERTIME`, 400 ms by default.
pub(crate) const TIP_HOVER_DELAY: f32 = 0.4;

/// `GALLERY_MARKDOWN_CODE=numbers,nowrap,cap:8` seeds the code card's knobs, so a
/// screenshot of a combination needs no clicks. A knob that does not parse is the default:
/// a screenshot run must not fail on a typo.
fn code_options_from_env() -> ui_widgets::CodeOptions {
    let mut code = ui_widgets::CodeOptions::default();
    let raw = std::env::var("GALLERY_MARKDOWN_CODE").unwrap_or_default();
    for part in raw.split(',') {
        match part.trim() {
            "numbers" => code.line_numbers = true,
            "nowrap" => code.wrap = false,
            cap => {
                let lines = cap.strip_prefix("cap:").and_then(|n| n.parse().ok());
                if let Some(lines) = lines {
                    code.max_height = Some(ui_widgets::CodeHeight::Lines(lines));
                }
            }
        }
    }
    code
}

/// Seconds this hover episode has run; the id is fresh per episode, so it starts at zero.
fn tip_age(cx: &mut Cx<'_, '_>, episode: u32) -> f32 {
    let timer = Id::of(("tip-timer", episode));
    cx.at(timer).age(timer)
}

impl Gallery {
    /// Parse the Markdown cards once per source change, and cut their frames there too.
    ///
    /// The control redraws the whole document every frame; a parse and a projection are
    /// both linear in its length, so doing either in the page body would repeat the work
    /// sixty times a second. The reading card's sample changes only with the locale, so it
    /// is keyed by the text it was parsed from, exactly like the editor's source.
    fn parse_markdown(&mut self, cx: &Cx<'_, '_>) {
        if self
            .markdown_doc
            .as_ref()
            .is_none_or(|(source, _)| source != &self.markdown_src)
        {
            let (document, prepared) = match self.markdown_doc.as_ref() {
                Some((_, previous)) => {
                    let reparsed = previous.reparse(&self.markdown_src);
                    let prepared = match self.markdown_prepared.as_deref() {
                        Some(old) => {
                            ui_richtext::Prepared::update(old, &reparsed.document, &reparsed.splice)
                        }
                        None => ui_richtext::Prepared::of(&reparsed.document),
                    };
                    (reparsed.document, prepared)
                }
                None => {
                    let document = ui_richtext::parse(&self.markdown_src);
                    let prepared = ui_richtext::Prepared::of(&document);
                    (document, prepared)
                }
            };
            self.markdown_prepared = Some(std::sync::Arc::new(prepared));
            self.markdown_doc = Some((self.markdown_src.clone(), std::sync::Arc::new(document)));
        }
        let sample = tr(cx, Msg::GALLERY_MARKDOWN_SAMPLE);
        if self
            .markdown_sample
            .as_ref()
            .is_none_or(|(text, _)| text != &sample)
        {
            let document = ui_richtext::parse(&sample);
            self.markdown_sample_prepared =
                Some(std::sync::Arc::new(ui_richtext::Prepared::of(&document)));
            self.markdown_sample = Some((sample, std::sync::Arc::new(document)));
        }
    }

    /// Mount a closed panel on hover, open it after the delay, and keep it until its fade ends.
    fn tick_tips(&mut self, cx: &mut Cx<'_, '_>) {
        let hover = (0..TIP_COLUMNS * TIPS).find(|slot| {
            cx.core()
                .hovered_within(tip_target_id(slot / TIPS, slot % TIPS))
        });
        if hover != self.tip_hover {
            self.tip_hover = hover;
            self.tip_gen = self.tip_gen.wrapping_add(1);
            self.tip_delay = 0.0;
        }
        if let Some(shown) = self.tip_shown {
            if self.tip_hover == Some(shown) {
                if self.tip_delay < TIP_HOVER_DELAY {
                    self.tip_delay = tip_age(cx, self.tip_gen);
                }
                return;
            }
            let now = cx.now();
            let since = *self.tip_close_at.get_or_insert(now);
            if now >= since + ui_widgets_fluent::TOOLTIP_FADE.duration as f64 {
                self.tip_shown = None;
                self.tip_close_at = None;
            }
            return;
        }
        let Some(k) = self.tip_hover else { return };
        // Mount closed at once so the fade has a zero to start from; reading the clock here
        // is what keeps frames coming through the hover delay.
        self.tip_shown = Some(k);
        self.tip_delay = tip_age(cx, self.tip_gen);
    }

    /// Whether `slot`'s panel is mounted, and whether it is open or fading out.
    pub(crate) fn tip_state(&self, slot: usize) -> Option<bool> {
        (self.tip_shown == Some(slot))
            .then(|| self.tip_hover == Some(slot) && self.tip_delay >= TIP_HOVER_DELAY)
    }

    /// Whether `id` is a nav row, and if so, go there.
    fn navigation(&mut self, id: Id) -> bool {
        if let Some(page) = PAGES
            .iter()
            .copied()
            .find(|p| p.nav_id() == id || p.nav_id().with("label") == id)
        {
            self.page = page;
            return true;
        }
        false
    }

    /// Whether `id` is one of the three switches.
    fn toggle_hit(&self, id: Id) -> bool {
        // The three style columns' toggles plus the four MD3 variant switches on the Switch page.
        (0..4).any(|k| md3_variant_id(k) == id)
            || PAGES
                .iter()
                .any(|page| STYLES.iter().any(|(tag, _)| switch_id(*page, tag) == id))
    }
    pub(crate) fn new() -> Self {
        use ui_platform::ThemeWatcher as _;
        // Publish the catalog scope before the first frame builds any page.
        init_locale();
        let theme_watch = ui_platform_wayland::PortalThemeWatcher::new();
        // `GALLERY_DARK=0|1` seeds the appearance, so a light-theme screenshot needs no click.
        let dark = match std::env::var("GALLERY_DARK").ok().as_deref() {
            Some("0") => false,
            Some("1") => true,
            _ => theme_watch.preference().is_dark(false),
        };
        Self {
            // `GALLERY_PAGE=slider` opens straight on a page, for screenshots of one section.
            page: std::env::var("GALLERY_PAGE")
                .ok()
                .and_then(|want| PAGES.into_iter().find(|p| p.tag() == want))
                .unwrap_or(Page::Switch),
            on: false,
            value: 0.35,
            dark,
            variant_on: [false, true, false, true],
            checked: [
                ui_widgets_md3::CheckState::Off,
                ui_widgets_md3::CheckState::Mixed,
                ui_widgets_md3::CheckState::Off,
                ui_widgets_md3::CheckState::On,
                ui_widgets_md3::CheckState::Off,
                ui_widgets_md3::CheckState::On,
            ],
            fchecked: [
                ui_widgets_fluent::CheckState::Unchecked,
                ui_widgets_fluent::CheckState::Checked,
                ui_widgets_fluent::CheckState::Indeterminate,
            ],
            ftri: ui_widgets_fluent::CheckState::Indeterminate,
            radio_sel: 0,
            fradio_sel: 0,
            tab_sel: [0, 1, 2, 0],
            fluent_tabs: vec![0, 1, 2, 3],
            fluent_tab_sel: 1,
            tip_hover: None,
            tip_gen: 0,
            tip_delay: 0.0,
            tip_shown: None,
            tip_close_at: None,
            info_open: [true; INFO_BARS],
            btn_sel: [true, false],
            split_open: false,
            split_presses: 0,
            md3_menu_picked: None,
            fab_open: [true, true, true, false, true],
            fab_picked: None,
            fab_presses: 0,
            // Open on arrival: the panel is the page's subject, not something to hunt for.
            menu_open: true,
            menu_picked: None,
            // Closed on arrival: two overlays open at once leave only the last with the keys.
            gmenu_open: false,
            gmenu_picked: None,
            // Closed on arrival: unlike the menu, the popover's blocks are the point, so the
            // reader opens it and sees them appear.
            gpop_open: false,
            gpop_done: 0,
            // The open panel is this page's subject too.
            combo_open: true,
            combo_sel: Some(1),
            // Open on arrival: the pane and its labels are the page's subject.
            nav_open: true,
            nav_view_sel: 0,
            nav_mode: ui_widgets_fluent::PaneDisplayMode::Expanded,
            chip_sel: [true, false],
            chip_glass: [true, false],
            chip_glass_gone: false,
            seg_single: 0,
            seg_multi: [true, false, false],
            seg_glass: 0,
            glass_press: 0,
            nav_sel: 0,
            drawer_sel: 0,
            drawer_open: false,
            rich_tip_open: false,
            toolbar_sel: Some(0),
            toolbar_fab_hits: 0,
            snack_hits: 0,
            snack_overlay_visible: false,
            card_hits: [0; CARD_CLICKS],
            list_sel: [true, false],
            glass_list_hits: 0,
            list_heights: std::cell::RefCell::new(ui_widgets::RowHeights::new(22.0)),
            banner_open: true,
            note_open: true,
            icon_filled: true,
            app_bar_scrolled: false,
            app_bar_collapsed: false,
            app_bar_fired: None,
            rail_expanded: false,
            carousel_fired: None,
            dp_value: [None, ui_widgets_md3::Date::new(2026, 10, 20)],
            dp_shown: [date_picker_today(), date_picker_today()],
            dp_view: [ui_widgets_md3::DatePickerView::Days; DATE_PICKERS],
            dp_modal: DateModalState {
                open: false,
                value: ui_widgets_md3::Date::new(2026, 10, 24),
                shown: date_picker_today(),
                view: ui_widgets_md3::DatePickerView::Days,
            },
            icon_button_on: [false, true, false, false],
            dp_range: DateRangeState {
                open: false,
                // A sample range, so the band is there the first time it opens.
                start: ui_widgets_md3::Date::new(2026, 10, 4),
                end: ui_widgets_md3::Date::new(2026, 10, 9),
                shown: date_picker_today(),
            },
            di_text: String::new(),
            di_action: None,
            tp_hour: ["09".to_string(), "07".to_string()],
            tp_minute: ["30".to_string(), "05".to_string()],
            tp_period: [ui_widgets_md3::Period::Am, ui_widgets_md3::Period::Pm],
            tf_value: Default::default(),
            tf_fluent: String::new(),
            tf_glass: String::new(),
            tf_secure: String::new(),
            math_edit_src: std::array::from_fn(|_| STRUCTURED_SEED.to_owned()),
            markdown_src: String::new(),
            markdown_doc: None,
            markdown_prepared: None,
            markdown_sample: None,
            markdown_sample_prepared: None,
            markdown_code: code_options_from_env(),
            search_value: String::new(),
            search_acted: None,
            search_open: false,
            search_view_value: String::new(),
            glass_search: String::new(),
            glass_search_acted: None,
            shortcut: None,
            dialog_picked: None,
            dialog_dismissed: 0,
            dialog_open: false,
            dialog_target: ModalTarget::Global,
            sheet_open: false,
            sheet_picked: None,
            sheet_dismissed: 0,
            toast_shown: None,
            toast_picked: 0,
            toast_dismissed: 0,
            modal_open: false,
            modal_detent: None,
            md3_sheet_open: false,
            md3_sheet_dismissed: 0,
            md3_side_sheet_open: false,
            md3_side_sheet_dismissed: 0,
            modal_dismissed: 0,
            stepper_value: 3.0,
            stepper_wrap: 4.0,
            stepper_presses: 0,
            fdlg_open: false,
            md3: Md3Kit::new(),
            menu_style: ui_widgets_md3::menu_style(
                ui_widgets_md3::pack_of(ui_design::Appearance::Light),
                0.0,
            ),
            fluent: FluentKit::new(),
            glass: HigKit::new(),
            dragging: None,
            materials: MaterialState::default(),
            step_value: 0.25,
            range: (0.2, 0.7),
            frames: 0,
            theme_from: None,
            theme_snap: false,
            theme_watch: Some(theme_watch),
        }
    }

    /// Pull any host colour-scheme change into the gallery's own dark flag.
    fn sync_system_theme(&mut self) {
        use ui_platform::ThemeWatcher;
        if let Some(watch) = self.theme_watch.as_mut()
            && watch.poll().is_some()
        {
            let want = watch.preference().is_dark(false);
            if want != self.dark {
                self.theme_from = Some(self.dark);
                self.theme_snap = true;
                self.dark = want;
            }
        }
    }

    /// Advance the transition progress and publish it for `style_env`.
    fn advance_theme(&mut self, info: &mut ui_core::cx::Cx<'_>) -> f32 {
        match self.theme_from {
            None => 1.0,
            Some(_) => {
                let id = Id::str("gallery/theme/t");
                if self.theme_snap {
                    let _ = info.animate(
                        id,
                        0.0,
                        ui_core::AnimSpec::new(0.0, ui_core::Easing::Linear),
                    );
                    self.theme_snap = false;
                }
                let t = info.animate(
                    id,
                    1.0,
                    ui_core::AnimSpec::new(0.2, ui_core::Easing::EaseOut),
                );
                if t >= 0.999 {
                    self.theme_from = None;
                }
                t
            }
        }
    }
}

impl App for Gallery {
    /// The menu look `view` resolved this frame; the engine copies it before painting.
    fn menu_style(&self) -> Option<ui_widgets::MenuStyle> {
        Some(self.menu_style.clone())
    }

    fn view(&mut self, info: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
        self.frames += 1;
        // Sync the host appearance, then advance the theme tween before any `style_env` read.
        self.sync_system_theme();
        let theme_t = self.advance_theme(info);
        THEME_FROM.with(|c| c.set(self.theme_from));
        THEME_T.with(|c| c.set(theme_t));
        // The scope is published through thread-locals, so the engine cannot see it move.
        if self.theme_from.is_some() {
            info.invalidate_decls();
        }
        // The theme env is the outermost layer: everything below inherits its appearance.
        set_root_env(theme_env(self.dark));
        // The app no longer owns a store: it builds straight through the context it was
        // handed, which carries the engine's store, clock and routing.
        let mut cx = Cx::new(info, root_env());
        // The editor starts from the same sample the read-only card draws, read from the
        // catalogue on first use because the text needs the app's locale.
        if self.page == Page::Markdown {
            if self.markdown_src.is_empty() {
                // REPRO (temporary): `GALLERY_MARKDOWN_DOC=path` opens a file in the editor.
                self.markdown_src = std::env::var("GALLERY_MARKDOWN_DOC")
                    .ok()
                    .and_then(|path| std::fs::read_to_string(path).ok())
                    .unwrap_or_else(|| tr(&cx, Msg::GALLERY_MARKDOWN_SAMPLE));
            }
            self.parse_markdown(&cx);
        }
        if self.page == Page::ToolTip {
            self.tick_tips(&mut cx);
        }
        let title = tr(&cx, self.page.title());
        let nav = sidebar(self.page, self.value, &mut cx);
        let mut content = Container::new(Id::str("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .gap(12.0)
            .child(toolbar(&title, &mut cx));
        // Only the visible page is built: constructing a page resolves its styles and
        // widgets eagerly, which costs more than every other phase of the frame together.
        let body = timed!("page_body", page_body(self.page, self, &mut cx));
        // The page's own viewport: a page taller than the window scrolls under the app bar
        // instead of running past it, which also lets focus bring a control into view.
        let page = Container::new(self.page.body_id(), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::hug(0.0, 0.0),
                height: AxisSize::hug(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .child(body);
        let wrapper = Container::new(Id::str("page/view"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .scrollable()
            .sense(ui_core::Sense::SCROLL_OWNED)
            .child(Box::new(page));
        content = content.child(Box::new(wrapper));
        // A modal floats over the window, so the root is a box: the shell is its flow child
        // and the modal is an overlay sibling. Nesting it inside the page column would dim
        // that column only and cover the samples it is meant to float above.
        // The drawer sits flush against the window's left edge; the content area keeps its
        // own inset. Both backgrounds come from the MD3 scheme, so the whole window reads
        // as one surface instead of a hand-picked dark grey.
        // The kit's resolution blends through a running theme transition, so the window
        // surface fades instead of snapping between schemes.
        let pack = self.md3.pack_for(&cx);
        // The menu is not part of the tree, so it takes the same pack the shell resolved. It
        // stays opaque: frost is an app's choice, through `Md3Options::menu_blur`.
        self.menu_style = ui_widgets_md3::menu_style(&pack, 0.0);
        let surface = pack
            .roles
            .get(ui_style::RoleName("surface"))
            .unwrap_or(Color::from_rgba8(16, 18, 24, 255));
        let shell = Container::new(Id::str("shell"), NodeKind::Row)
            .gap(16.0)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::Yes,
                grow_y: Grow::Yes,
            })
            .padding(Insets {
                left: 0.0,
                right: 16.0,
                top: 16.0,
                bottom: 16.0,
            })
            .child(nav)
            .child(Box::new(content));
        let mut root = Container::new(Id::str("root"), NodeKind::Box)
            .background(surface, Corners::ZERO)
            // The photo sits under the shell; the surface above stays as the letterbox and
            // the fallback when the file is missing.
            .background_image(crate::wallpaper::WALLPAPER, ImageFit::Cover)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fill(0.0, 0.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .child(Box::new(shell));
        if self.dialog_open
            && self.page == Page::Dialogs
            && self.dialog_target == ModalTarget::Global
        {
            root = root.child(dialogs_modal(ModalTarget::Global, &mut cx));
        }
        // The sheet is a modal too, so it lives at the root beside the shell, never inside a column.
        if self.sheet_open && self.page == Page::ActionSheet {
            root = root.child(sheet_modal(&mut cx));
        }
        if self.modal_open && self.page == Page::ModalSheet {
            root = root.child(modal_sheet_modal(&mut cx));
        }
        if self.md3_sheet_open && self.page == Page::ModalSheet {
            root = root.child(md3_sheet_modal(&mut cx));
        }
        if self.md3_side_sheet_open && self.page == Page::ModalSheet {
            root = root.child(modal_sheet::md3_side_sheet_modal(&mut cx));
        }
        // The toast is not modal: it sits above the shell but lets every press through.
        if let Some(shown) = self.toast_shown
            && self.page == Page::Toast
        {
            root = root.child(toast_layer(shown, &mut cx));
        }
        // The dialog is declared only while it is open: its entrance and its way out are
        // the control's own declarations, so the app needs no warm-up frame and no clock.
        if self.page == Page::Dialogs && self.fdlg_open {
            root = root.child(dialogs_fluent::fluent_dialog_modal(&mut cx));
        }
        // The picker is declared only while it is open: its entrance is the control's own
        // declaration, so the app needs no warm-up frame and no clock.
        if self.page == Page::DatePicker && self.dp_modal.open {
            root = root.child(date_modal_picker(&self.dp_modal, &mut cx));
        }
        if self.page == Page::DatePicker && self.dp_range.open {
            root = root.child(date_range_picker(&self.dp_range, &mut cx));
        }
        Box::new(root)
    }

    fn changed(&mut self, on: Id, mut change: ui_widgets::Change) -> bool {
        use ui_widgets::Change as C;
        // The in-place editor is one input over the whole source, so an edit arrives as the
        // whole new document; the app replaces its own copy and reparses next frame.
        if on == markdown_edit_root()
            && let C::Text(next) = &change
        {
            self.markdown_src = next.clone();
            return true;
        }
        // Each control reports its own meaning: a row says "activated", a switch says
        // which state it settled on, a slider reports the value under the finger.
        // The nav drawer's offset is engine-owned now: the wheel lands directly in the
        // scroller, and the app no longer accumulates or clamps it.
        // A dialog names the action that was hit, or says its scrim was pressed. Both close
        // it here: the control reports, the app decides what closing means.
        if dialog_is_ours(on) {
            match change {
                C::Selected(Some(picked)) => {
                    self.dialog_picked = dialog_label_of(picked);
                    self.dialog_open = false;
                    return true;
                }
                C::Cancelled => {
                    self.dialog_dismissed += 1;
                    self.dialog_open = false;
                    return true;
                }
                _ => {}
            }
        }
        if on == dialog_open_id() && matches!(change, C::Activated) {
            self.dialog_target = ModalTarget::Global;
            self.dialog_open = true;
            return true;
        }
        // The action sheet reports the action that was hit, or says it was dismissed — the
        // cancel card and a scrim press both arrive as `Cancelled`.
        if self.sheet_open && on == sheet_id() {
            match change {
                C::Selected(Some(picked)) => {
                    self.sheet_picked = sheet_label_of(picked);
                    self.sheet_open = false;
                    return true;
                }
                C::Cancelled => {
                    self.sheet_dismissed += 1;
                    self.sheet_open = false;
                    return true;
                }
                _ => {}
            }
        }
        if on == sheet_open_id() && matches!(change, C::Activated) {
            self.sheet_open = true;
            return true;
        }
        // The modal sheet reports the content it pressed, the detent a drag settled on, or
        // its own dismissal — a scrim press, Escape or a drag below the lowest detent.
        if self.modal_open && on == modal_sheet_id() {
            match change {
                C::Selected(Some(picked)) if picked == modal_sheet_done_id() => {
                    self.modal_open = false;
                    return true;
                }
                C::Selected(Some(picked))
                    if picked == detent_id_of(modal_sheet_id(), GlassSheetDetent::Half) =>
                {
                    self.modal_detent = Some(Msg::GALLERY_MODAL_SHEET_HALF);
                    return true;
                }
                C::Selected(Some(picked))
                    if picked == detent_id_of(modal_sheet_id(), GlassSheetDetent::Full) =>
                {
                    self.modal_detent = Some(Msg::GALLERY_MODAL_SHEET_FULL);
                    return true;
                }
                C::Dismissed => {
                    self.modal_dismissed += 1;
                    self.modal_open = false;
                    return true;
                }
                _ => {}
            }
        }
        // The modal drawer names the destination it picked, or says its scrim was pressed.
        if self.drawer_open && on == modal_drawer_id() {
            match change {
                C::Selected(Some(picked)) => {
                    if let Some(k) = modal_drawer_pick(picked) {
                        self.drawer_sel = k;
                    }
                    self.drawer_open = false;
                    return true;
                }
                C::Dismissed => {
                    self.drawer_open = false;
                    return true;
                }
                _ => {}
            }
        }
        if on == drawer_open_id() && matches!(change, C::Activated) {
            self.drawer_open = true;
            return true;
        }
        // The rich tooltip reports the action it offered; the app decides when it closes.
        if on == rich_tip_trigger_id() && matches!(change, C::Activated) {
            self.rich_tip_open = !self.rich_tip_open;
            return true;
        }
        if self.rich_tip_open
            && on == rich_tip_panel_id()
            && let C::Selected(Some(picked)) = change
            && picked == rich_tip_action_id()
        {
            self.rich_tip_open = false;
            return true;
        }
        // A stepper reports the value it stepped to; where it stops is the app's call.
        if on == stepper_id() || on == stepper_wrap_id() {
            if let C::Value(v) = change {
                if on == stepper_id() {
                    self.stepper_value = v;
                } else {
                    self.stepper_wrap = v;
                }
                self.stepper_presses += 1;
                return true;
            }
            return false;
        }
        if on == modal_sheet_open_id() && matches!(change, C::Activated) {
            self.modal_open = true;
            self.modal_detent = Some(Msg::GALLERY_MODAL_SHEET_HALF);
            return true;
        }
        if on == md3_sheet_open_id() && matches!(change, C::Activated) {
            self.md3_sheet_open = true;
            return true;
        }
        // The MD3 sheet names the button it holds, or says it was dragged away or pressed
        // outside; how either closes it is the app's decision.
        if self.md3_sheet_open && on == md3_sheet_id() {
            match change {
                C::Selected(Some(picked)) if picked == md3_sheet_done_id() => {
                    self.md3_sheet_open = false;
                    return true;
                }
                C::Cancelled => {
                    self.md3_sheet_dismissed += 1;
                    self.md3_sheet_open = false;
                    return true;
                }
                _ => {}
            }
        }
        if on == md3_side_sheet_open_id() && matches!(change, C::Activated) {
            self.md3_side_sheet_open = true;
            return true;
        }
        if self.md3_side_sheet_open && on == md3_side_sheet_id() {
            match change {
                C::Selected(Some(picked)) if picked == md3_side_sheet_save_id() => {
                    self.md3_side_sheet_open = false;
                    return true;
                }
                C::Cancelled => {
                    self.md3_side_sheet_dismissed += 1;
                    self.md3_side_sheet_open = false;
                    return true;
                }
                _ => {}
            }
        }
        // A toast reports the action it offered, or says it was swiped away. The app owns
        // how long it lives; the control never closes itself.
        if on == toast_show_id() && matches!(change, C::Activated) {
            self.toast_shown = Some(ToastShown::Success);
            return true;
        }
        if on == toast_error_id() && matches!(change, C::Activated) {
            self.toast_shown = Some(ToastShown::ErrorTop);
            return true;
        }
        if self.toast_shown.is_some() && on == toast_id() {
            match change {
                C::Selected(Some(picked)) if picked == toast_action_id() => {
                    self.toast_picked += 1;
                    self.toast_shown = None;
                    return true;
                }
                C::Cancelled => {
                    self.toast_dismissed += 1;
                    self.toast_shown = None;
                    return true;
                }
                _ => {}
            }
        }
        // The Fluent dialog reports the action that was hit; a preview reports too, and only
        // the modal closes. The control never decides its own lifetime.
        if dialogs_fluent::fluent_dialog_is_ours(on)
            && let C::Selected(Some(picked)) = change
        {
            self.dialog_picked = dialogs_fluent::fluent_dialog_label_of(picked);
            if on == dialogs_fluent::fluent_dialog_id() {
                self.fdlg_open = false;
            }
            return true;
        }
        // The glass alerts are previews: they report their action and the app records it.
        if dialogs::glass_dialog_is_ours(on)
            && let C::Selected(Some(picked)) = change
        {
            self.dialog_picked = dialogs::glass_dialog_label_of(picked);
            return true;
        }
        if on == dialogs_fluent::fluent_dialog_open_id() && matches!(change, C::Activated) {
            self.fdlg_open = true;
            return true;
        }
        // The Fluent strip: a header names itself when picked, its close button sends a
        // payload, and the add button is a plain activation.
        if let Some(k) = self
            .fluent_tabs
            .iter()
            .position(|slot| fluent_tab_id(*slot) == on)
        {
            if matches!(&change, C::Selected(Some(_))) {
                self.fluent_tab_sel = self.fluent_tabs[k];
                return true;
            }
            // A dragged header takes the slot it was released into; the app owns the order.
            if let C::Data(payload) = &change
                && let Some(ui_widgets_fluent::TabViewChange::Dropped { from, to }) =
                    payload.downcast_ref::<ui_widgets_fluent::TabViewChange>()
                && *from < self.fluent_tabs.len()
                && *to < self.fluent_tabs.len()
            {
                let moved = self.fluent_tabs.remove(*from);
                self.fluent_tabs.insert(*to, moved);
                return true;
            }
            if matches!(
                &change,
                C::Data(payload)
                    if payload.downcast_ref::<ui_widgets_fluent::TabViewChange>()
                        == Some(&ui_widgets_fluent::TabViewChange::Closed)
            ) {
                let closed = self.fluent_tabs.remove(k);
                if self.fluent_tab_sel == closed {
                    self.fluent_tab_sel = self
                        .fluent_tabs
                        .get(k)
                        .or_else(|| self.fluent_tabs.last())
                        .copied()
                        .unwrap_or(0);
                }
                return true;
            }
        }
        if on == fluent_tab_add_id() && matches!(change, C::Activated) {
            if let Some(slot) = (0..TAB_MANY.len()).find(|s| !self.fluent_tabs.contains(s)) {
                self.fluent_tabs.push(slot);
                self.fluent_tab_sel = slot;
            }
            return true;
        }
        if on == dialog_open_panel_id() && matches!(change, C::Activated) {
            self.dialog_target = ModalTarget::Proximate;
            self.dialog_open = true;
            return true;
        }
        // An info bar reports a close; collapsing it is the app's decision, and the reset
        // button is the way back.
        if let Some(k) = (0..INFO_BARS).position(|k| info_bar_id(k) == on)
            && matches!(change, C::Activated)
        {
            self.info_open[k] = false;
            return true;
        }
        if on == info_bar_reset_id() && matches!(change, C::Activated) {
            self.info_open = [true; INFO_BARS];
            return true;
        }
        // A date picker reports one fact per click: a day chosen, a month moved, a view
        // switched. What each means for the page is the app's call.
        if let Some(k) = (0..DATE_PICKERS).find(|k| date_picker_id(*k) == on) {
            // A mismatched payload comes back unchanged, so the branches below still see it.
            change = match change.data::<ui_widgets_md3::DatePickerChange>() {
                Ok(fact) => {
                    match fact {
                        ui_widgets_md3::DatePickerChange::Picked(date) => {
                            self.dp_value[k] = Some(date)
                        }
                        ui_widgets_md3::DatePickerChange::Shown { date, view } => {
                            self.dp_shown[k] = date;
                            self.dp_view[k] = view;
                        }
                        ui_widgets_md3::DatePickerChange::View(view) => self.dp_view[k] = view,
                        // The docked form has no scrim, so it cannot report one.
                        ui_widgets_md3::DatePickerChange::Dismissed => {}
                    }
                    return true;
                }
                Err(other) => other,
            };
        }
        // The modal sample: the app owns whether it is open, so a pick and a scrim press both
        // close it, and a fresh press on the button opens it on the same month.
        if on == date_modal_button_id() && matches!(change, C::Activated) {
            self.dp_modal.open = true;
            return true;
        }
        if on == date_modal_id() {
            change = match change.data::<ui_widgets_md3::DatePickerChange>() {
                Ok(fact) => {
                    match fact {
                        ui_widgets_md3::DatePickerChange::Picked(date) => {
                            self.dp_modal.value = Some(date);
                            self.dp_modal.open = false;
                        }
                        ui_widgets_md3::DatePickerChange::Shown { date, view } => {
                            self.dp_modal.shown = date;
                            self.dp_modal.view = view;
                        }
                        ui_widgets_md3::DatePickerChange::View(view) => {
                            self.dp_modal.view = view;
                        }
                        ui_widgets_md3::DatePickerChange::Dismissed => self.dp_modal.open = false,
                    }
                    return true;
                }
                Err(other) => other,
            };
        }
        // The range sample: a day press carries the selection Compose's rule produced, and
        // the app keeps it; a scrim press closes the picker.
        // An icon button reports its activation; the four toggle samples own the state.
        if let Some(k) = (0..4).find(|k| icon_button_toggle_id(*k) == on) {
            if matches!(change, C::Activated) {
                self.icon_button_on[k] = !self.icon_button_on[k];
            }
            return true;
        }
        if on == date_range_button_id() && matches!(change, C::Activated) {
            self.dp_range.open = true;
            return true;
        }
        if on == date_range_id() {
            change = match change.data::<ui_widgets_md3::DateRangeChange>() {
                Ok(fact) => {
                    match fact {
                        ui_widgets_md3::DateRangeChange::Selected { start, end } => {
                            self.dp_range.start = Some(start);
                            self.dp_range.end = end;
                        }
                        ui_widgets_md3::DateRangeChange::Dismissed => self.dp_range.open = false,
                    }
                    return true;
                }
                Err(other) => other,
            };
        }
        // The text-mode date input reports one action per click; its buffer is an engine text
        // input on the field's own id, so edits arrive as `Change::Text` beside it.
        if on == date_input_id() {
            change = match change.data::<ui_widgets_md3::DateInputChange>() {
                Ok(fact) => {
                    self.di_action = Some(fact.0);
                    return true;
                }
                Err(other) => other,
            };
        }
        if on == ui_widgets_md3::date_input_field_id(date_input_id())
            && let C::Text(next) = &change
        {
            self.di_text = next.clone();
            return true;
        }
        // A time input's fields are the engine's text inputs: edits arrive as `Change::Text`
        // on the field's own id, and the widget reports only what the engine cannot.
        if let Some(k) = (0..TIME_PICKERS).find(|k| {
            let picker = time_picker_id(*k);
            on == picker || on == picker.with("hour") || on == picker.with("minute")
        }) {
            if let C::Text(next) = &change {
                if on == time_picker_id(k).with("hour") {
                    self.tp_hour[k] = next.clone();
                } else {
                    self.tp_minute[k] = next.clone();
                }
                return true;
            }
            change = match change.data::<ui_widgets_md3::TimePickerChange>() {
                Ok(fact) => {
                    match fact {
                        ui_widgets_md3::TimePickerChange::Hour(text) => {
                            self.tp_hour[k] = text;
                        }
                        ui_widgets_md3::TimePickerChange::Minute(text) => {
                            self.tp_minute[k] = text;
                        }
                        ui_widgets_md3::TimePickerChange::Period(period) => {
                            self.tp_period[k] = period;
                        }
                    }
                    return true;
                }
                Err(other) => other,
            };
        }
        // An app bar reports the slot that was pressed; what that means is the app's call.
        if app_bar_is_ours(on)
            && let C::Selected(Some(slot)) = change
            && let Some(name) = app_bar_slot_name(slot)
        {
            self.app_bar_fired = Some(name);
            return true;
        }
        if on == app_bar_scroll_id() && matches!(change, C::Activated) {
            self.app_bar_scrolled = !self.app_bar_scrolled;
            return true;
        }
        if on == app_bar_collapse_id() && matches!(change, C::Activated) {
            self.app_bar_collapsed = !self.app_bar_collapsed;
            return true;
        }
        if on == rail_expand_id() && matches!(change, C::Activated) {
            self.rail_expanded = !self.rail_expanded;
            return true;
        }
        // A text field reports the whole buffer after an edit; the app keeps it and
        // re-declares it next frame.
        if let C::Text(next) = &change {
            if let Some(k) = (0..TF_LIVE).find(|k| text_field_id(*k) == on) {
                self.tf_value[k] = next.clone();
                return true;
            }
            if on == fluent_text_field_id() {
                self.tf_fluent = next.clone();
                return true;
            }
            if on == glass_text_field_id() {
                self.tf_glass = next.clone();
                return true;
            }
            if on == glass_secure_field_id() {
                self.tf_secure = next.clone();
                return true;
            }
            if let Some(k) = MATH_COLUMNS.iter().position(|tag| math_edit_id(tag) == on) {
                self.math_edit_src[k] = next.clone();
                return true;
            }
        }
        // The WinUI text box names its clear button by that button's own id.
        if on == fluent_text_field_id()
            && matches!(change, C::Selected(Some(id)) if id == on.with("clear"))
        {
            self.tf_fluent.clear();
            return true;
        }
        if on == text_field_clear_id() && matches!(change, C::Activated) {
            self.tf_value = Default::default();
            return true;
        }
        // A search bar edits its query through the engine and reports its own actions.
        if on == search_id() {
            if let C::Text(next) = &change {
                self.search_value = next.clone();
                return true;
            }
            change = match change.data::<ui_widgets_md3::SearchBarChange>() {
                Ok(ui_widgets_md3::SearchBarChange::Action(id)) => {
                    self.search_acted = Some(if id == search_action_id() {
                        "mic".to_string()
                    } else {
                        format!("{id:?}")
                    });
                    return true;
                }
                Err(other) => other,
            };
        }
        // The search view opens on its own request and closes on its back action; the app
        // owns both ends, so neither happens without this branch.
        if on == search_view_id() {
            if let C::Text(next) = &change {
                self.search_view_value = next.clone();
                return true;
            }
            match change.data::<ui_widgets_md3::SearchViewChange>() {
                Ok(ui_widgets_md3::SearchViewChange::Activated) => {
                    self.search_open = true;
                    return true;
                }
                Ok(ui_widgets_md3::SearchViewChange::Action(_)) => {
                    self.search_open = false;
                    return true;
                }
                Err(other) => change = other,
            }
        }
        // The glass bar edits through its field and names the button that was pressed.
        if on == glass_search_id().with("field")
            && let C::Text(next) = &change
        {
            self.glass_search = next.clone();
            return true;
        }
        if on == glass_search_id()
            && let C::Selected(Some(id)) = &change
        {
            if *id == glass_search_id().with("field").with("trailing") {
                self.glass_search.clear();
                self.glass_search_acted = Some("clear".to_string());
                return true;
            }
            if *id == glass_search_id().with("cancel") {
                self.glass_search.clear();
                self.glass_search_acted = Some("cancel".to_string());
                return true;
            }
        }
        // A capturing node reports the chord it saw; the app decides what to bind.
        if let C::KeyChord(chord) = &change
            && on == shortcut_field_id()
        {
            self.shortcut = Some(*chord);
            return true;
        }
        // A context-menu row the app declared reports its own id.
        if let C::MenuItem(item) = &change
            && *item == Id::str("tf/menu/clear")
        {
            self.tf_value[0].clear();
            return true;
        }
        // The materials page owns each block's knobs and each block's place on the stage.
        if let Some(knob) = glass_knob_of(on) {
            if let C::Value(v) = change {
                self.materials.set_glass(knob, v as f32);
                return true;
            }
            return false;
        }
        if let Some((k, param)) = material_param_of(on) {
            if let C::Value(v) = change {
                self.materials.set(k, param, v as f32);
                return true;
            }
            return false;
        }
        if let Some(k) = material_block_of(on) {
            match change {
                C::DragStarted => {
                    drag_block(&mut self.materials, k, true, 0.0, 0.0);
                    return true;
                }
                C::Moved { dx, dy } => {
                    drag_block(&mut self.materials, k, false, dx as f32, dy as f32);
                    return true;
                }
                C::Dropped(_) | C::Cancelled => {
                    end_drag(&mut self.materials, k);
                    return true;
                }
                _ => {}
            }
        }
        // An MD3 menu row carries its own id; the app owns the selection it reports.
        if let C::Selected(Some(row)) = change
            && let Some(j) = (0..MD3_MENU_ITEMS).find(|j| md3_menu_item_id(*j) == row)
        {
            self.md3_menu_picked = Some(j);
            return true;
        }
        // A split button carries the half that was pressed in the change, not the root.
        if let C::Selected(Some(half)) = change {
            if half == btn_split_id().with("leading") {
                self.split_presses += 1;
                return true;
            }
            if half == btn_split_id().with("trailing") {
                self.split_open = !self.split_open;
                return true;
            }
        }
        // A carousel reports the item that was pressed; its strip scrolls itself.
        if let C::Selected(Some(item)) = change
            && let Some((k, j)) = carousel_item_of(item)
        {
            self.carousel_fired = Some((k, j));
            return true;
        }
        match change {
            C::Activated if self.navigation(on) => return true,
            // A button says only that it was activated; a toggle's state is the app's.
            C::Activated => {
                // The snackbar reports from its root; only its action carries a click sense.
                if snack_pick(on).is_some() {
                    self.snack_hits += 1;
                    return true;
                }
                // The mode buttons pick how much of the pane stays on screen; the pane
                // itself starts in the state that mode is about.
                if let Some(k) = (0..NAV_MODES.len()).find(|k| nav_mode_id(*k) == on) {
                    self.nav_mode = NAV_MODES[k];
                    self.nav_open = self.nav_mode == ui_widgets_fluent::PaneDisplayMode::Expanded;
                    return true;
                }
                if on == snack_overlay_show_id() {
                    self.snack_overlay_visible = !self.snack_overlay_visible;
                    return true;
                }
                // The banner leaves by collapsing its own slot, so the app only owns the flag.
                if on == banner_toggle_id() {
                    self.banner_open = !self.banner_open;
                    return true;
                }
                if on == note_toggle_id() {
                    self.note_open = !self.note_open;
                    return true;
                }
                // The group buttons write the whole column, which is how a mixed box is
                // reached at all: clicking one only ever settles it to checked.
                for (tag, _) in CHECKBOX_GROUP {
                    if on == checkbox_group_id(tag) {
                        let value = match tag {
                            "off" => ui_widgets_md3::CheckState::Off,
                            "mixed" => ui_widgets_md3::CheckState::Mixed,
                            _ => ui_widgets_md3::CheckState::On,
                        };
                        self.checked = [value; CHECKBOXES];
                        return true;
                    }
                }
                if let Some(k) = (0..BTN_TOGGLES).find(|k| btn_toggle_id(*k) == on) {
                    self.btn_sel[k] = !self.btn_sel[k];
                    return true;
                }
                // A glass filter chip toggles itself; the app stores the new state.
                if let Some(k) = (0..CHIP_GLASS_FILTERS).find(|k| chip_glass_id(*k) == on) {
                    self.chip_glass[k] = !self.chip_glass[k];
                    return true;
                }
                // A glass button reports only that it was pressed; the count is the app's.
                let glass = (0..GLASS_ICON_BTNS)
                    .find(|k| glass_btn_id(*k) == on)
                    .or_else(|| (0..GLASS_LABEL_BTNS).find(|k| glass_label_btn_id(*k) == on));
                if glass.is_some() {
                    self.glass_press += 1;
                    return true;
                }
                // A FAB menu's anchor opens and closes it. The control reports from its root,
                // so the root is what to match; the close button's id never arrives here.
                if let Some(k) = (0..FAB_MENUS).find(|k| fab_menu_id(*k) == on) {
                    self.fab_open[k] = !self.fab_open[k];
                    return true;
                }
                // A plain FAB reports one activation; the page counts them.
                if (0..FAB_SAMPLES).any(|k| fab_sample_id(k) == on)
                    && matches!(change, C::Activated)
                {
                    self.fab_presses += 1;
                    return true;
                }
                // The Fluent menu's anchor opens and closes it, as `MenuFlyout.ShowAt` does.
                if on == menu_anchor_id() {
                    self.menu_open = !self.menu_open;
                    return true;
                }
                if on == glass_menu_anchor_id() {
                    self.gmenu_open = !self.gmenu_open;
                    return true;
                }
                if on == glass_popover_anchor_id() {
                    self.gpop_open = !self.gpop_open;
                    return true;
                }
                // A combo box's field says only that it was pressed; opening is the app's call.
                if on == combo_id() {
                    self.combo_open = !self.combo_open;
                    return true;
                }
                // A card says only that it was activated; the count is the app's.
                if let Some(k) = (0..CARD_CLICKS).find(|k| card_id(CARD_VARIANTS[*k]) == on) {
                    self.card_hits[k] += 1;
                    return true;
                }
                // A list row says only that it was activated; whether that selects it is the app's.
                if let Some(k) = (0..LIST_SELECT).find(|k| list_id("selection", k + 1) == on) {
                    self.list_sel[k] = !self.list_sel[k];
                    return true;
                }
                // A glass row says only that it was activated; the count is the app's.
                if ["wifi", "airplane", "dnd", "standalone"]
                    .iter()
                    .any(|tag| glass_tile_id(tag) == on)
                {
                    self.glass_list_hits += 1;
                    return true;
                }
                // The animated icon says only that it was activated; the target is the app's.
                if on == icon_toggle_id() {
                    self.icon_filled = !self.icon_filled;
                    return true;
                }
            }
            // A variant toggle carries its own on-value; the shared page toggle carries `on`.
            // A radio only ever says "this one"; the group stores the choice.
            C::Selected(Some(picked)) => {
                // A popover reports the content id it saw pressed; the app owns what follows.
                if on == glass_popover_id() && picked == glass_popover_done_id() {
                    self.gpop_done += 1;
                    self.gpop_open = false;
                    return true;
                }
                // A glass chip's delete glyph names itself; the app drops the chip.
                if picked == chip_glass_removable_id().with("delete") {
                    self.chip_glass_gone = true;
                    return true;
                }
                // The app bar's trailing action flips the whole gallery's appearance.
                if picked == Id::str("toolbar/appearance") {
                    self.dark = !self.dark;
                    self.theme_from = Some(!self.dark);
                    self.theme_snap = true;
                    return true;
                }
                // A nav drawer names the destination it was pressed on; a bare container
                // would not report which row the press landed in.
                if self.navigation(picked) {
                    return true;
                }
                // A segment reports only itself; single-select replaces, multi-select flips.
                if let Some(k) = (0..SEGMENTS).find(|k| seg_single_id(*k) == picked) {
                    self.seg_single = k;
                    return true;
                }
                if let Some(k) = (0..SEGMENTS).find(|k| seg_multi_id(*k) == picked) {
                    self.seg_multi[k] = !self.seg_multi[k];
                    return true;
                }
                if let Some(k) = (0..SEGMENTS).find(|k| seg_glass_id(*k) == picked) {
                    self.seg_glass = k;
                    return true;
                }
                if let Some(k) = (0..SEGMENTS).find(|k| seg_glass_icon_id(*k) == picked) {
                    self.seg_glass = k;
                    return true;
                }
                if let Some(k) = nav_pick(picked) {
                    self.nav_sel = k;
                    return true;
                }
                if let Some(k) = drawer_pick(picked) {
                    self.drawer_sel = k;
                    return true;
                }
                if toolbar_fab_hit(picked) {
                    self.toolbar_fab_hits += 1;
                    return true;
                }
                if let Some(k) = toolbar_pick(picked) {
                    self.toolbar_sel = Some(k);
                    return true;
                }
                if let Some(k) = (0..RADIOS).find(|k| radio_id(*k) == picked) {
                    self.radio_sel = k;
                    return true;
                }
                if let Some(k) = (0..RADIOS).find(|k| fluent_radio_id(*k) == picked) {
                    self.fradio_sel = k;
                    return true;
                }
                // The Fluent menu names the command it ran; a menu always closes on a pick.
                if let Some(j) = (0..MENU_COMMANDS).find(|j| menu_item_id(*j) == picked) {
                    self.menu_picked = Some(j);
                    self.menu_open = false;
                    return true;
                }
                if let Some(j) = (0..MENU_COMMANDS).find(|j| glass_menu_item_id(*j) == picked) {
                    self.gmenu_picked = Some(j);
                    self.gmenu_open = false;
                    return true;
                }
                // A combo box names the row it took; selecting it closes the panel.
                if let Some(j) = (0..COMBO_ITEMS).find(|j| combo_item_id(*j) == picked) {
                    self.combo_sel = Some(j);
                    self.combo_open = false;
                    return true;
                }
                // A navigation view names the destination it took; the app owns the page.
                if let Some(k) = (0..NAV_VIEW_ITEMS).find(|k| nav_view_item_id(*k) == picked) {
                    self.nav_view_sel = k;
                    // An overlay pane has served its purpose once it navigated somewhere.
                    if self.nav_mode != ui_widgets_fluent::PaneDisplayMode::Expanded {
                        self.nav_open = false;
                    }
                    return true;
                }
                // A FAB menu names the item it picked and closes itself.
                for k in 0..FAB_MENUS {
                    if let Some(j) = (0..FAB_ITEMS).find(|j| fab_item_id(k, *j) == picked) {
                        self.fab_picked = Some((k, j));
                        self.fab_open[k] = false;
                        return true;
                    }
                }
                // A tab bar names the tab it picked; the app keeps the choice.
                for bar in 0..TAB_BARS {
                    if let Some(k) = (0..TAB_COUNT[bar]).find(|k| tab_item_id(bar, *k) == picked) {
                        self.tab_sel[bar] = k;
                        return true;
                    }
                }
            }
            C::Toggled { on: settled } => {
                // A filter chip toggles itself; the app stores the new state.
                if let Some(k) = (0..CHIP_FILTERS).find(|k| chip_filter_id(*k) == on) {
                    self.chip_sel[k] = settled == Some(true);
                    return true;
                }
                // The navigation view's toggle reports the state the pane would settle into.
                if on == nav_view_id() {
                    self.nav_open = settled == Some(true);
                    return true;
                }
                if let Some(k) = (0..3).find(|k| fluent_checkbox_id(*k) == on) {
                    self.fchecked[k] = match settled {
                        Some(true) => ui_widgets_fluent::CheckState::Checked,
                        Some(false) => ui_widgets_fluent::CheckState::Unchecked,
                        None => ui_widgets_fluent::CheckState::Indeterminate,
                    };
                    return true;
                }
                if on == fluent_checkbox_id(4) {
                    self.ftri = match settled {
                        Some(true) => ui_widgets_fluent::CheckState::Checked,
                        Some(false) => ui_widgets_fluent::CheckState::Unchecked,
                        None => ui_widgets_fluent::CheckState::Indeterminate,
                    };
                    return true;
                }
                if let Some(k) = (0..CHECKBOXES).find(|k| checkbox_id(*k) == on) {
                    // The control names the value it should take, indeterminate included.
                    self.checked[k] = match settled {
                        Some(true) => ui_widgets_md3::CheckState::On,
                        Some(false) => ui_widgets_md3::CheckState::Off,
                        None => ui_widgets_md3::CheckState::Mixed,
                    };
                    return true;
                }
                if let Some(k) = (0..4).find(|k| md3_variant_id(*k) == on) {
                    self.variant_on[k] = settled == Some(true);
                    return true;
                }
                // The Markdown page's three code-card knobs.
                if on == code_option_id("numbers") {
                    self.markdown_code.line_numbers = settled == Some(true);
                    return true;
                }
                if on == code_option_id("wrap") {
                    self.markdown_code.wrap = settled == Some(true);
                    return true;
                }
                if on == code_option_id("height") {
                    self.markdown_code.max_height = (settled == Some(true))
                        .then_some(ui_widgets::CodeHeight::Lines(CODE_CAP_LINES));
                    return true;
                }
                if self.toggle_hit(on) {
                    self.on = settled == Some(true);
                    return true;
                }
            }
            // A press outside the panel or Escape reached it; the app owns what closing means.
            C::Dismissed => {
                if on == menu_flyout_id() {
                    self.menu_open = false;
                    return true;
                }
                if on == glass_menu_id() {
                    self.gmenu_open = false;
                    return true;
                }
                if on == glass_popover_id() {
                    self.gpop_open = false;
                    return true;
                }
                if on == combo_id() {
                    self.combo_open = false;
                    return true;
                }
                // An overlay pane closes on a press outside it, or on Escape.
                if on == nav_view_id() {
                    self.nav_open = false;
                    return true;
                }
            }
            _ => {}
        }
        // A range slider reports both values in one compound change; the single slider
        // reports the number it settled on.
        if on == slider_step_id() {
            if let C::Value(v) = change {
                self.step_value = (v as f32).clamp(0.0, 1.0);
                return true;
            }
            return false;
        }
        if on == slider_range_id() || on == slider_range_step_id() {
            return match change.data::<ui_widgets_md3::Md3Range>() {
                Ok(r) => {
                    self.range = (r.start, r.end);
                    true
                }
                Err(_) => false,
            };
        }
        let Some(index) = slider_column_of(on) else {
            return false;
        };
        match change {
            // The control mapped the press point to a value itself; nothing to compute here.
            C::Value(v) => {
                self.value = (v as f32).clamp(0.0, 1.0);
                self.dragging = Some(index);
            }
            C::DragStarted => self.dragging = Some(index),
            C::Selected(_) | C::Cancelled => self.dragging = None,
            _ => return false,
        }
        true
    }
}
