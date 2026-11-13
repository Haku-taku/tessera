## Gallery chrome: nav, shell, and the per-page sample copy.

gallery-section-controls = Controls
gallery-section-layouts = Layouts
gallery-section-materials = Materials
gallery-section-design-systems = Design systems

gallery-style-md3 = MD3
gallery-style-fluent = Fluent
gallery-style-glass = Glass

gallery-not-implemented = not implemented yet
gallery-appearance-light = Light
gallery-appearance-dark = Dark
gallery-sidebar-readout = semantic: on={ $on } value={ $value }%

## Page titles (nav rows and the content header).

gallery-page-switch-title = Controls · Switch
gallery-page-checkbox-title = Controls · Checkbox
gallery-page-radio-title = Controls · Radio
gallery-page-slider-title = Controls · Slider
gallery-page-tabs-title = Controls · Tabs
gallery-page-tooltip-title = Controls · Tooltips
gallery-page-info-bar-title = Controls · Info bars
gallery-page-buttons-title = Controls · Buttons
gallery-page-icon-button-title = Controls · Icon button
gallery-page-fab-menu-title = Controls · FAB menu
gallery-page-menu-flyout-title = Controls · Menu flyout
gallery-page-combo-title = Controls · Combo box
gallery-page-chips-title = Controls · Chips
gallery-page-cards-title = Controls · Cards
gallery-page-lists-title = Controls · Lists
gallery-page-icons-title = Controls · Icons
gallery-page-dialogs-title = Controls · Dialogs
gallery-page-action-sheet-title = Controls · Action sheet
gallery-page-toast-title = Controls · Toast
gallery-page-modal-sheet-title = Controls · Modal sheet
gallery-page-stepper-title = Controls · Stepper
gallery-page-date-picker-title = Controls · Date picker
gallery-page-time-picker-title = Controls · Time picker
gallery-page-app-bar-title = Controls · App bar
gallery-page-carousel-title = Controls · Carousel
gallery-page-text-field-title = Controls · Text fields
gallery-page-search-title = Controls · Search
gallery-page-selection-title = Controls · Text selection
gallery-page-generic-title = Controls · Pack widgets
gallery-page-layouts-title = Layouts · Scaffold
gallery-page-grid-title = Layouts · Grid
gallery-page-inline-title = Layouts · Inline
gallery-page-markdown-title = Layouts · Markdown
gallery-markdown-edit-title = Layouts · Markdown · Editing
gallery-markdown-code-numbers = Line numbers
gallery-markdown-code-wrap = Wrap lines
gallery-markdown-code-height = Cap height
gallery-page-math-title = Layouts · Math
gallery-page-positioning-title = Layouts · Positioning
gallery-page-progress-title = Controls · Progress indicator
gallery-page-segmented-title = Controls · Segmented buttons
gallery-page-nav-bar-title = Controls · Navigation bar
gallery-page-nav-rail-title = Controls · Navigation rail
gallery-page-nav-drawer-title = Controls · Navigation drawer
gallery-page-nav-view-title = Controls · Navigation view
gallery-page-toolbar-title = Controls · Toolbar
gallery-page-snackbar-title = Controls · Snackbar
gallery-page-materials-title = Materials · Panels
gallery-page-md3-system-title = Design systems · MD3
gallery-page-fluent-system-title = Design systems · Fluent 2
gallery-page-hig-system-title = Design systems · HIG

## Switch.

gallery-switch-caption = { $kind ->
  [icon] with icon
 *[plain] no icon
}{ $disabled ->
  [yes] , disabled
 *[no] { "" }
}

## Checkbox.

gallery-checkbox-caption = { $state ->
  [on] checked
  [mixed] mixed
 *[off] unchecked
}{ $error ->
  [yes] ", error"
 *[no] { "" }
}{ $disabled ->
  [yes] ", disabled"
 *[no] { "" }
}
gallery-checkbox-all-off = all off
gallery-checkbox-all-mixed = all mixed
gallery-checkbox-all-on = all on
gallery-checkbox-note = no size variant in the export — error and disabled are the build-time knobs
gallery-checkbox-label-off = Unchecked
gallery-checkbox-label-on = Checked
gallery-checkbox-label-mixed = Indeterminate
gallery-checkbox-label-tri = Select all

## Radio.

gallery-radio-option = option { $letter }{ $selected ->
  [yes] {" "}(selected)
 *[no] { "" }
}
gallery-radio-disabled = { $selected ->
  [yes] disabled, selected
 *[no] disabled
}
gallery-radio-note = the export has one geometry; selected / disabled are the only knobs

## Slider.

gallery-slider-size-caption = { $size } — { $track }dp track, { $handle }dp handle
gallery-slider-disabled-caption = disabled — onSurface at 12% / 38%
gallery-slider-stepped-caption = stepped — { $stops } stops, tick dots
gallery-slider-range-caption = range — two handles, { $start }%..{ $end }%
gallery-slider-range-step-caption = range + step — both handles snap
gallery-slider-readout = value = { $value }% (shared by every row)
gallery-slider-value = value = { $value }%

## Tabs.

gallery-tab-overview = Overview
gallery-tab-specs = Specs
gallery-tab-reviews = Reviews
gallery-tab-home = Home
gallery-tab-search = Search
gallery-tab-library = Library
gallery-tab-one = One
gallery-tab-two = Two
gallery-tab-three = Three
gallery-tab-four = Four
gallery-tab-five = Five
gallery-tabs-caption-text = text-only — 3 tabs, equal shares of the bar
gallery-tabs-caption-icons = with icons — 72dp content, 2dp underline band
gallery-tabs-caption-many = five tabs — the share follows the count
gallery-tabs-caption-disabled = disabled — onSurface at 38%, no underline
gallery-tabs-readout = selected: { $sel } (one per bar, click to change)
gallery-tabs-fluent = WinUI tabs — 32 dp headers, the selected one joined to the page, close and add buttons

## Tooltips.

gallery-tooltip-caption = WinUI tooltips — acrylic over a blurred backdrop, 12 pt text, 320 px cap, 167 ms fade
gallery-tooltip-caption-md3 = Material 3 tooltips — an inverse-surface panel, 12 sp label, 200 dp cap, 150 ms in and 75 ms out
gallery-tooltip-caption-rich = Material 3 rich tooltips — a 320 dp surface with a subhead, supporting text and text-button actions; the app owns when it shows
gallery-tooltip-rich-hint = click Info — the app declares the panel only while it is open, and its action closes it
gallery-tooltip-rich-open = Info
gallery-tooltip-rich-subhead = Rich tooltip
gallery-tooltip-rich-text = A rich tooltip explains a feature in place: a title names it, a short paragraph says what it does, and an action acknowledges it.
gallery-tooltip-rich-action = Got it
gallery-tooltip-delay = hover a button — it opens after the 400 ms system hover time, then fades in
gallery-tooltip-btn-save = Save
gallery-tooltip-btn-delete = Delete
gallery-tooltip-btn-long = Long text
gallery-tooltip-save = Save the file
gallery-tooltip-delete = Delete the selected rows
gallery-tooltip-long = Hovering this button shows a text long enough to reach the 320 px cap, so the panel wraps instead of growing.
gallery-tooltip-long-md3 = Hovering this button shows a label longer than the 200 dp cap, so the panel wraps instead of growing.

## Buttons.

gallery-btn-filled = Filled
gallery-btn-tonal = Tonal
gallery-btn-elevated = Elevated
gallery-btn-outlined = Outlined
gallery-btn-text = Text
gallery-btn-xs = XS
gallery-btn-small = Small
gallery-btn-medium = Medium
gallery-btn-large = Large
gallery-btn-xl = XL
gallery-btn-toggle-a = Toggle A
gallery-btn-toggle-b = Toggle B
gallery-btn-undo = Undo
gallery-btn-redo = Redo
gallery-btn-copy = Copy
gallery-buttons-caption-styles = styles — filled, tonal, elevated (small)
gallery-buttons-caption-styles2 = styles — outlined, text: no container, the outline is the affordance
gallery-buttons-caption-sizes = sizes — 32 / 40 / 56dp (xsmall, small, medium)
gallery-buttons-caption-sizes2 = sizes — 96 / 136dp, the label grows with the box
gallery-buttons-caption-icons = with icons — leading pad, icon, icon-label gap, label
gallery-buttons-caption-toggles = toggle buttons — `selected` belongs to the app, a click flips it
gallery-buttons-caption-group = connected group — one pill, the inner edges stay square
gallery-buttons-caption-disabled = disabled — content at 38%, container at 10%
gallery-buttons-caption-disabled2 = disabled — no container, the outline stays at 12%
gallery-buttons-readout = toggles: { $sel } (click to flip)
gallery-btn-default = Default
gallery-btn-accent = Accent
gallery-btn-subtle = Subtle
gallery-btn-text-link = Text link
gallery-buttons-caption-fluent-styles = WinUI styles — default, accent, subtle, text link: one 32dp row
gallery-buttons-caption-fluent-border = elevation border — a 3dp gradient, mirrored in light theme
gallery-buttons-caption-fluent-disabled = disabled — the disabled brush family, nothing else fades
gallery-buttons-caption-glass-icons = glass — 56dp capsules that inflate 17dp on the long side when pressed
gallery-buttons-caption-glass-labels = labelled — 44dp tall, the width comes from the text
gallery-buttons-caption-glass-disabled = disabled — half opacity, and the press reports nothing
gallery-buttons-glass-readout = glass presses { $count }

gallery-split-button-label = Open
gallery-split-button-caption = split button — the trailing half owns the menu, the leading half the action
gallery-split-button-sizes-caption = sizes — 32 / 40 / 56 / 96 / 136dp, both halves share the height
gallery-split-button-readout = menu open: { $open } · leading presses: { $count }

## FAB menu.

gallery-fab-create = Create
gallery-fab-share = Share
gallery-fab-edit = Edit
gallery-fab-note-primary = primary — items take primaryContainer
gallery-fab-note-secondary = secondary — items take secondaryContainer
gallery-fab-note-tertiary = tertiary — items take tertiaryContainer
gallery-fab-note-closed = closed — only the anchor shows
gallery-fab-note-disabled = disabled — the anchor takes no input
gallery-fab-tone-primary = primary
gallery-fab-tone-secondary = secondary
gallery-fab-tone-tertiary = tertiary
gallery-fab-readout-picked = picked: { $tone } · { $item } (the menu closed itself)
gallery-fab-readout-none = picked: nothing yet (click an item, or the anchor to close)

gallery-fab-samples-caption = sizes 40 · 56 · 80 · 96 dp, five colour styles, one lowered
gallery-fab-readout = FAB activations: { $count }

## Menu flyout.

gallery-menu-caption = MenuFlyoutPresenter — 8dp corners, 1dp stroke, acrylic; items 4dp apart, 11,8,11,9 inside, 16dp icon column
gallery-menu-show = Show menu
gallery-md3-menu-caption = MD3 menu — 4dp corners, 2dp inset; rows 44dp, a section label, a selected row and a disabled one
gallery-md3-menu-section = File
gallery-md3-menu-supporting = Opens a saved document
gallery-menu-glass-caption = glass menu — 200dp panel, 32dp radius, 44dp rows 2dp apart
gallery-menu-new = New
gallery-menu-open = Open
gallery-menu-save = Save
gallery-menu-share = Share
gallery-menu-print = Print
gallery-menu-accel-new = Ctrl+N
gallery-menu-accel-open = Ctrl+O
gallery-menu-accel-save = Ctrl+S
gallery-menu-accel-share = Ctrl+Shift+S
gallery-menu-accel-print = Ctrl+P
gallery-menu-readout-picked = ran: { $command } (the menu closed itself)
gallery-menu-readout-none = ran: nothing yet (click an item, or the anchor to close)
gallery-popover-caption = glass popover — the menu's 280dp sheet, holding the app's own blocks
gallery-popover-show = Show popover
gallery-popover-title = Profile
gallery-popover-body = Any blocks sit inside the glass; what they do is the app's call.
gallery-popover-done = Done
gallery-popover-readout = done pressed { $count } times (click outside or press Escape to close)

## Combo box.

gallery-combo-caption = ComboBox — 64×32 minimum, 12,5,0,7 inside, 1dp border, 4dp corners; the panel is acrylic at 8dp and follows the field's width
gallery-combo-header = Fruit
gallery-combo-placeholder = Pick a fruit
gallery-combo-item-0 = Apple
gallery-combo-item-1 = Banana
gallery-combo-item-2 = Cherry
gallery-combo-item-3 = Damson
gallery-combo-item-4 = Elderberry
gallery-combo-readout-picked = picked: { $item }
gallery-combo-readout-none = picked: nothing yet (open the field, then click a row)

## Navigation view.

gallery-nav-view-documents = Documents
gallery-nav-view-search = Search
gallery-nav-view-saved = Saved
gallery-nav-view-print = Print
gallery-nav-view-body = The pane is 320 dp wide; its toggle folds it to 48 dp and the labels step aside, leaving the icons and the 3×16 selection indicator.
gallery-nav-view-mode-expanded = Expanded
gallery-nav-view-mode-compact = Compact
gallery-nav-view-mode-minimal = Minimal

## Chips.

gallery-chip-assist = Assist
gallery-chip-suggestion = Suggestion
gallery-chip-filter = Filter
gallery-chip-input = Input
gallery-chip-selected = Selected
gallery-chip-unselected = Unselected
gallery-chips-caption-kinds = kinds — assist / suggestion / filter / input, all flat
gallery-chips-caption-icons = with a leading mark — the pad shrinks from 16 to 8
gallery-chips-caption-elevated = elevated — a fill and a shadow instead of a ring
gallery-chips-caption-selected = filter selection — the container replaces the ring (click to toggle)
gallery-chips-caption-disabled = disabled — content 38%, ring and container 12%
gallery-chips-readout = filter selection: { $sel }
gallery-chips-glass-caption-marks = glass — a 16 dp mark, a 6 dp gap, a 12/8 pad, a 14 pt w500 label
gallery-chips-glass-caption-selected = selection — a 10 % base wash inside the capsule (click to toggle)
gallery-chips-glass-caption-deletable = deletable — the × reports on its own node
gallery-chips-glass-caption-disabled = disabled — content at 50 %
gallery-chips-glass-deletable = Removable
gallery-chips-glass-readout = glass selection: { $sel } · removed: { $gone }

## Cards.

gallery-card-elevated = Elevated card
gallery-card-filled = Filled card
gallery-card-outlined = Outlined card
gallery-card-static = Static card
gallery-card-activated = activated { $count ->
  [one] { $count } time
 *[other] { $count } times
}
gallery-card-static-body = no action, so no state layer
gallery-card-disabled-body = disabled
gallery-cards-caption-variants = elevated / filled / outlined — click to activate
gallery-cards-caption-static = static — it cannot be hovered or clicked, so it paints no layer
gallery-cards-caption-disabled = disabled — 38% container, 12% ring, no action
gallery-cards-readout = activations: { $hits }
gallery-card-glass-title = Glass card
gallery-card-glass-body = 16 dp inset and a 12 dp continuous corner over the package's default body
gallery-cards-glass-caption-card = glass card — 16 dp inset, 12 dp corner
gallery-cards-glass-caption-solid = solid card — white in light, #1C1C1E in dark, 26 dp corner
gallery-cards-glass-caption-container = plain container — no inset, 16 dp corner
gallery-cards-glass-note = the body is LiquidGlassSettings()'s default: thickness 30, blur 3, chromatic 0.5.

## Lists.

gallery-list-one-line = One line
gallery-list-two-lines = Two lines
gallery-list-three-lines = Three lines
gallery-list-supporting = Supporting text
gallery-list-overline = Overline
gallery-list-icon = Icon
gallery-list-avatar = Avatar
gallery-list-row = Row { $index }
gallery-list-segment = Segment { $index }
gallery-list-disabled-row = Disabled row
gallery-lists-caption-lines = 56 / 72 / 88 — one, two and three lines
gallery-lists-caption-leading = leading slots — icon 24dp, avatar 40dp (chosen when built)
gallery-lists-caption-selection = selection — secondary-container, owned by the app (click to toggle)
gallery-lists-caption-segmented = segmented — a 2dp gap and a rounded container
gallery-lists-caption-ornaments = trailing badges — 16dp pill, 6dp dot; divider 1dp, inset 16
gallery-lists-caption-disabled = disabled — content 38%, no state layer
gallery-list-unread = Inbox
gallery-list-drafts = Drafts
gallery-lists-glass-caption-group = rows in a card — 16/12 inset, separators from the text edge
gallery-lists-glass-caption-standalone = standalone — its own 12 dp grouped cell
gallery-lists-glass-caption-still = still — a row with no action paints no wash
gallery-lists-glass-readout = rows activated { $count } times — press one, or Tab to it
gallery-list-glass-wifi = Wi-Fi
gallery-list-glass-wifi-sub = Home network
gallery-list-glass-airplane = Airplane mode
gallery-list-glass-airplane-sub = Off
gallery-list-glass-dnd = Do not disturb
gallery-list-glass-dnd-sub = Until 8:00
gallery-list-glass-standalone = Location
gallery-list-glass-standalone-sub = While using the app
gallery-list-glass-still = Version
gallery-list-glass-still-sub = 26.2

## Icons.

gallery-icons-column = Material Symbols · { $set }
gallery-icons-caption-toggle = click the heart to animate FILL

## Dialogs.

gallery-dialog-caption-no-icon = no icon — every line starts at the inset, two actions
gallery-dialog-caption-with-icon = with icon — the lines centre, and three actions still end-align
gallery-dialog-caption-no-title = no title — the body takes the title's inset, one action
gallery-dialog-title-discard = Discard draft?
gallery-dialog-body-discard = This draft will be gone for good.
gallery-dialog-title-reset = Reset settings?
gallery-dialog-body-reset = Every local change is removed, including unsaved ones.
gallery-dialog-body-undone = Nothing here can be undone.
gallery-dialog-title-publish = Publish changes?
gallery-dialog-body-publish = Everything behind the scrim is blocked until you choose.
gallery-dialog-action-cancel = Cancel
gallery-dialog-action-discard = Discard
gallery-dialog-action-later = Later
gallery-dialog-action-save = Save
gallery-dialog-action-reset = Reset
gallery-dialog-action-ok = OK
gallery-dialog-show-modal = Show modal
gallery-dialog-caption-modal = modal — the scrim covers this window: click outside the surface to dismiss
gallery-dialog-show-in-panel = Show in panel
gallery-dialog-panel-note = a panel that can host a modal
gallery-dialog-host-note = a panel that can host a modal — click the button, and the scrim covers only this panel
gallery-dialog-column-host = MD3 — modal host
gallery-dialog-column-fluent = Fluent — content dialog
gallery-dialog-column-glass = LiquidGlass — alert
gallery-dialog-glass-caption-two = two actions — the destructive one keeps the system red
gallery-dialog-glass-caption-three = three actions — the strip stacks, iOS style
gallery-dialog-glass-caption-one = one action — it takes the whole width
gallery-dialog-fluent-caption-default = default primary — the accent marks what Enter does
gallery-dialog-fluent-caption-none = no default — three actions split the command area
gallery-dialog-fluent-caption-close = close only — the lone action keeps the right half
gallery-dialog-fluent-show = Show content dialog
gallery-dialog-action-delete = Delete
gallery-dialog-action-keep = Keep
gallery-dialog-action-publish = Publish
gallery-dialogs-readout = last action: { $action } · dismissals: { $count }
gallery-action-sheet-show = Show action sheet
gallery-action-sheet-caption = the sheet rises from the bottom edge — 8 dp inset, two cards
gallery-action-sheet-readout = last action: { $action } · dismissals: { $count }
gallery-action-sheet-title = Share photo
gallery-action-sheet-message = Choose what to do with this photo.
gallery-action-sheet-open = Open
gallery-action-sheet-share = Share
gallery-action-sheet-delete = Delete
gallery-action-sheet-cancel = Cancel

## Toast.
gallery-toast-show = Show toast
gallery-toast-show-error = Show error at the top
gallery-toast-caption = a 48 dp pill 16 dp from the edge — it floats, it does not block
gallery-toast-readout = actions: { $picked } · dismissals: { $count }
gallery-toast-saved = Photo saved to your library
gallery-toast-undo = Undo
gallery-toast-failed = Could not reach the server

## Modal sheet.
gallery-modal-sheet-show = Show modal sheet
gallery-modal-sheet-caption = half at 45 % of the window, full at 90 dp from the top — drag the handle
gallery-modal-sheet-readout = detent: { $detent } · dismissals: { $count }
gallery-modal-sheet-title = Photo details
gallery-modal-sheet-body = Drag the handle up for the full detent, down to dismiss.
gallery-modal-sheet-half = half
gallery-modal-sheet-full = full
gallery-modal-sheet-none = —
gallery-modal-sheet-done = Done

gallery-bottom-sheet-open = Show the MD3 sheet
gallery-bottom-sheet-caption = modal · 640 dp max, drag past half to dismiss
gallery-bottom-sheet-readout = dismissals: { $count }
gallery-bottom-sheet-title = MD3 bottom sheet
gallery-bottom-sheet-body = Drag the handle down past half the height, or press outside, to close.
gallery-bottom-sheet-done = Done

gallery-side-sheet-open = Show the MD3 side sheet
gallery-side-sheet-caption = modal · 256 dp, docked to the trailing edge
gallery-side-sheet-readout = dismissals: { $count }
gallery-side-sheet-title = Side sheet
gallery-side-sheet-body = A panel docked to the trailing edge, with its line on the inner side.
gallery-side-sheet-save = Save

## Stepper.
gallery-stepper-caption = 94×34 capsule; hold a side to repeat after 0.5 s, then every 0.1 s
gallery-stepper-caption-wrap = wraps past either end
gallery-stepper-caption-disabled = disabled
gallery-stepper-readout = value: { $value } · wrap: { $wrap } · steps: { $count }

## Date picker.

gallery-date-readout = today { $today } · picked: { $picked } · shown: { $month } { $year } · view: { $view }
gallery-date-view-days = days
gallery-date-view-months = months
gallery-date-view-years = years
gallery-date-caption-mon = docked · Monday first
gallery-date-caption-sun = docked · Sunday first, 10–24 October selectable
gallery-date-modal-caption = modal · 360×524 over a scrim
gallery-date-modal-open = Open the modal picker
gallery-date-range-caption = modal · two dates over one scrolling month list
gallery-date-range-open = Open the range picker
gallery-date-range-readout = range: { $start } – { $end }
gallery-date-input-title = MD3 · text input
gallery-date-input-readout = buffer "{ $text }" · last action: { $action }
gallery-date-input-none = none yet
gallery-date-input-calendar = switch to calendar
gallery-date-input-cancel = cancel
gallery-date-input-confirm = confirm
gallery-date-input-supporting = Select date
gallery-date-input-headline = No date selected
gallery-date-input-label = Date
gallery-date-input-placeholder = MM/DD/YYYY
gallery-date-input-hint = Type the date, then confirm
gallery-date-input-cancel-label = Cancel
gallery-date-input-confirm-label = OK
gallery-date-input-pending = The Fluent and glass packs have no date picker yet.

## Time picker.

gallery-time-caption-live = time input · live
gallery-time-caption-disabled = time input · disabled

## App bar.

gallery-appbar-title-inbox = Inbox
gallery-appbar-title-medium = Medium
gallery-appbar-title-large = Large
gallery-appbar-title-flexible = Flexible
gallery-appbar-title-search = Search
gallery-appbar-sub-medium = medium flexible · 136dp with a subtitle
gallery-appbar-sub-large = large flexible · display-small collapses to title-large
gallery-appbar-caption-small = small · 64dp · leading icon + two actions
gallery-appbar-caption-medium = medium · 112dp · 32dp avatar leading, title below the band
gallery-appbar-caption-large = large · 152dp · headline-medium title at the bottom
gallery-appbar-caption-medium-flex = medium flexible · 112dp, 136dp with a subtitle · collapses to 64dp
gallery-appbar-caption-large-flex = large flexible · 120dp, 152dp with a subtitle · collapses to 64dp
gallery-appbar-caption-search = search · 56dp pill · surface-container with an on-surface-variant label
gallery-appbar-toggle-scrolled = toggle scrolled
gallery-appbar-toggle-collapsed = toggle collapsed
gallery-appbar-readout = last slot: { $slot } · scrolled: { $scrolled } · collapsed: { $collapsed }

## Carousel.

gallery-carousel-caption-multi = multi-browse · 16dp gutter · 156 / 104 / 52 at 360dp
gallery-carousel-caption-hero = hero · 280dp large with a 40dp item peeking in
gallery-carousel-caption-hero-centered = center-aligned hero · the large item sits between two small ones
gallery-carousel-caption-uncontained = uncontained · every item 280dp, sliding under the gutter
gallery-carousel-caption-full = full screen · one 120dp item at a time, vertical
gallery-carousel-caption-loop = looping · eight items that never reach an end
gallery-carousel-prev = prev
gallery-carousel-next = next
gallery-carousel-readout = last item: { $item }
gallery-carousel-offset = offset { $offset } / { $max }dp
gallery-carousel-item = sample { $sample } item { $index }

## Text fields and search.

gallery-tf-filled = Filled
gallery-tf-outlined = Outlined
gallery-tf-placeholder = Type here
gallery-tf-menu-clear = Clear field
gallery-tf-readout = app buffer: { $buffers }
gallery-tf-chord-caption = shortcut · focus, then press a chord
gallery-tf-shortcut-placeholder = Press a chord
gallery-tf-label = Label
gallery-tf-name = Name
gallery-tf-email = Email
gallery-tf-search = Search
gallery-tf-disabled = Disabled
gallery-tf-supporting = Supporting text
gallery-tf-error-supporting = Enter a valid email
gallery-tf-caption-fluent = header · placeholder · type here
gallery-tf-caption-fluent-text = text · the clear button appears
gallery-tf-caption-fluent-disabled = disabled
gallery-tf-caption-filled-label = filled · label and supporting text
gallery-tf-caption-filled-floats = filled · text floats the label
gallery-tf-caption-outlined-floats = outlined · text floats the label
gallery-tf-caption-outlined-icon = outlined · leading icon
gallery-tf-caption-outlined-error = outlined · error
gallery-tf-caption-outlined-disabled = outlined · disabled
gallery-tf-caption-glass = glass · leading icon · type here
gallery-tf-caption-glass-search = glass · search variant, 44 high
gallery-tf-caption-glass-text = glass · disabled, with text
gallery-tf-caption-glass-secure = glass · secure: marks, no copy
gallery-tf-secure-placeholder = Password
gallery-tf-clear-button = clear
gallery-search-hint = Search
gallery-search-hint-contained = Contained
gallery-search-caption-docked = docked · leading icon · one action
gallery-search-caption-contained = contained
gallery-search-caption-long = a query wider than the bar
gallery-search-supporting = Search settings, apps and files
gallery-search-caption-supporting = supporting text and a trailing avatar
gallery-search-caption-glass = glass · clear × and cancel button
gallery-search-caption-glass-quiet = glass · no cancel button
gallery-search-view-hint = Search this device
gallery-search-caption-view = search view · click the bar to open, the back arrow to close
gallery-search-readout = app query: { $query } · last action: { $action }

## Text selection.

gallery-selection-prose = Drag across this paragraph to select it. The highlight is the engine's geometry; how it is drawn belongs to the style. Double-click a word to take just that word, and watch the three columns differ.

## Generic pack-driven widgets.

gallery-generic-button = pack button
gallery-generic-card-title = Card
gallery-generic-column-card = Card + Button
gallery-generic-column-list = List
gallery-generic-column-panel = Panel
gallery-generic-column-paged = Paged strip
gallery-generic-page = Page { $index }
gallery-generic-row = row { $index }
gallery-generic-panel-title = Panel
gallery-generic-panel-hint = pack-driven
gallery-generic-column-collapse = Collapsing row
gallery-generic-banner-text = Dismissible banner
gallery-generic-banner-hide = Hide banner
gallery-generic-banner-show = Show banner
gallery-generic-column-grow = Growing card
gallery-generic-note = A line that comes and goes.
gallery-generic-note-add = Add a line
gallery-generic-note-remove = Remove the line

## Layout grids.

gallery-grid-column-md3 = MD3 · fixed + fr
gallery-grid-column-fluent = Fluent · auto columns
gallery-grid-column-glass = Glass · spans + alignment
gallery-grid-column-dense = Dense · repeat(auto-fill) + minmax
gallery-grid-tile-row2 = row 2
gallery-grid-tile-span = spans two columns
gallery-grid-tile-pinned = pinned
gallery-grid-tile-two-span = two
gallery-grid-tile-long-label = a longer label
gallery-grid-tile-second = second
gallery-grid-tile-row = row
gallery-grid-tile-here = here
gallery-grid-tile-one = one
gallery-grid-tile-mid = mid
gallery-grid-tile-two-rows = two rows
gallery-grid-tile-two-columns = row 1 · two columns
gallery-grid-tile-centred = centred

## Inline.

gallery-inline-column-baseline = Baseline · two icons
gallery-inline-column-mixed = Top · middle · bottom
gallery-inline-column-narrow = Narrow · atom on its own line
gallery-inline-column-row = Cross-item baseline
gallery-inline-column-decorations = Decorations · single, double, dotted, wavy
gallery-inline-column-justify = Justify · both edges flush
gallery-inline-column-holder = Inline box · a widget in a sentence
gallery-inline-sync-1 = Syncing{" "}
gallery-inline-sync-2 = {" "}with the server and{" "}
gallery-inline-sync-3 = {" "}three more files
gallery-inline-tall-1 = Tall{" "}
gallery-inline-tall-2 = {" "}top,{" "}
gallery-inline-tall-3 = {" "}middle,{" "}
gallery-inline-tall-4 = {" "}bottom.
gallery-inline-narrow-1 = before{" "}
gallery-inline-narrow-2 = {" "}after

## Positioning.

gallery-positioning-column-relative = Relative · keeps its slot
gallery-positioning-column-absolute = Absolute · padding box + auto margins
gallery-positioning-column-fixed = Fixed · root box, and z order
gallery-positioning-tile-shifted = shifted
gallery-positioning-tile-pin = pin
gallery-positioning-tile-auto = auto
gallery-positioning-tile-in-flow = in flow
gallery-positioning-tile-fixed = fixed

## Scaffold regions.

gallery-scaffold-nav = nav
gallery-scaffold-header = header
gallery-scaffold-main = main
gallery-scaffold-context = context
gallery-scaffold-inline = inline
gallery-scaffold-overlay = overlay

## Materials.

gallery-materials-plain = Plain
gallery-materials-acrylic = Acrylic
gallery-materials-glass = Glass

## Design-system pages.

gallery-system-colours = { $system } · colours
gallery-system-dimensions = { $system } · dimensions
gallery-system-provenance = provenance
gallery-system-common-roles = common roles
gallery-system-native-extras = native extras

## Progress, navigation, toolbar and snackbar pages.

gallery-icon-button-styles = four styles · standard, filled, tonal, outlined
gallery-icon-button-sizes = five sizes · 32, 40, 56, 96 and 136 dp
gallery-icon-button-widths = three widths at medium · narrow 48, default 56, wide 72
gallery-icon-button-toggles = click to check · the checked corner is 12 dp
gallery-icon-button-toggles-title = MD3 · toggles
gallery-icon-button-readout = checked: { $on }
gallery-icon-button-none = none yet
gallery-icon-button-disabled = disabled · the token's own 38% mark and 10% fill
gallery-progress-caption = { $percent }% of the track
gallery-progress-note = the ring's track stops 4 dp short of the arc either side; that gap is the export's track_active_indicator_space, the token the bar uses too.
gallery-progress-error = 60% of the track, in the error colour
gallery-progress-paused = 60% of the track, paused
gallery-progress-indeterminate = indeterminate — two bars on a two-second clock
gallery-progress-md3-indeterminate = indeterminate — two bars on the 1.8 s Android sweep
gallery-progress-wavy = 60 % of the track as a wave — the expressive form, 10 dp tall
gallery-progress-wavy-indeterminate = indeterminate — two waves sweeping on the 1.8 s clock
gallery-progress-md3-ring-indeterminate = indeterminate — a 40 dp ring, arc growing and travelling
gallery-progress-fluent-note = 3 dp slot, 1 dp track, rounded indicator; the sweep reads the frame clock itself.
gallery-progress-fluent-rings = rings at 0, 25, 60 and 100 %, then spinning and inactive — the track brush is transparent in WinUI 3
gallery-progress-glass-rings = rings at 0, 25, 60 and 100 % at 20 dp, then a 40 dp one at 35 % and a sweeping one
gallery-progress-glass-bars = bars at 0, 25, 60 and 100 %, then a sweeping one — 4 dp tall, never narrower than 200 dp
gallery-progress-glass-note = the sweep runs on its own one-second clock; the halo is a wider translucent stroke, since the engine blurs no arc.

gallery-segmented-single = single-select — a click replaces the choice
gallery-segmented-multi = multi-select — a click toggles the segment
gallery-segmented-disabled = disabled — 38% content, 12% outline
gallery-segmented-day = Day
gallery-segmented-week = Week
gallery-segmented-month = Month
gallery-segmented-readout = single { $single }, multi { $multi }
gallery-segmented-note = 40dp tall; each segment reports its own id, so the app owns the selection.
gallery-segmented-glass = labels — 13pt, w500 at 60% then w600 when selected
gallery-segmented-glass-icons = icons — 20pt glyphs in equal cells
gallery-segmented-glass-readout = glass selected { $index }

gallery-nav-caption = labels: { $kind ->
  [all] always
  [selected] selected only
  [expressive] inside the pill
 *[none] never
}
gallery-nav-home = Home
gallery-nav-search = Search
gallery-nav-profile = Profile
gallery-nav-bar-note = 80dp bar; the capsule scales in on x over 500ms.
gallery-nav-rail-note = 80dp column; without labels the capsule becomes a 56×56 circle.
gallery-nav-rail-expand = expand / collapse
gallery-nav-rail-expressive-note = expressive: the 96dp column and the 220dp panel are one control — 300ms standard takes the width across, and the pill opens with it.

gallery-drawer-headline = Mail
gallery-drawer-caption = standard — a headline group over three destinations
gallery-drawer-inbox = Inbox
gallery-drawer-sent = Sent
gallery-drawer-drafts = Drafts
gallery-drawer-note = 360dp surface, 336×56 capsules inset 12dp; standard variant, no scrim.
gallery-drawer-open = Open drawer
gallery-drawer-modal-caption = modal — the page dims behind a 32% scrim, and a press outside asks it to close

gallery-toolbar-caption = { $kind ->
  [docked] docked
 *[floating] floating
} · { $theme ->
  [standard] standard
 *[vibrant] vibrant
}
gallery-toolbar-note = 48dp icon-button slots on a 64dp bar.

gallery-snackbar-message = Your message was archived.
gallery-snackbar-two-lines = This message needs a second line to say what actually happened.
gallery-snackbar-action = Undo
gallery-snackbar-one-line = one line with an action
gallery-snackbar-two-line-caption = two lines, the 68dp branch
gallery-snackbar-floating = floating, text inset by 16dp
gallery-snackbar-hidden = hidden — paints nothing
gallery-snackbar-hits = action clicked { $count } times
gallery-snackbar-note = The app owns the lifetime; the bar fades in and out.
gallery-snackbar-overlay-show = show floating bar
gallery-snackbar-overlay-caption = overlay — floats over the page bottom, out of the flow

## Info bar.

gallery-infobar-caption = WinUI info bars — a 48 dp strip tinted by severity, a filled status disc, a 38 dp close
gallery-infobar-info-title = Update available.
gallery-infobar-info-message = Restart the application to apply the latest update.
gallery-infobar-success-title = Backup complete.
gallery-infobar-success-message = All documents were uploaded successfully.
gallery-infobar-warning-title = Your subscription is expiring in 3 days.
gallery-infobar-warning-message = Renew your subscription to keep all functionality.
gallery-infobar-error-title = No Internet
gallery-infobar-error-message = Reconnect to continue working.
gallery-infobar-reopen = Show the closed bars again
gallery-infobar-narrow-caption = narrow — the panel stacks title and message once they no longer share a line
gallery-infobar-narrow-title = Backup complete
gallery-infobar-narrow-message = All documents were uploaded successfully, and this bar is narrow enough that the message can no longer sit beside its title.

## The Markdown page's sample document: content, not chrome, but it renders through
## the same catalog so the page shows a CJK document in a CJK locale.

gallery-markdown-sample =
    # Markdown, placed

    One paragraph holds several runs: *emphasis*, **strong**, `inline code`, ~~struck through~~ and a [link](https://example.com), so it wraps as one line and each mark travels with its words.

    - A bullet holds blocks, not a string
    - Nesting works:

      1. ordered items start from their own first number
      2. and keep their paragraphs


    > A quote is a container like any other.

    - [x] A task item draws its box instead of its marker
    - [ ] and keeps the bytes that make it editable

    ```rust
    let runs = to_text_block(&document, block, base, look);
    let first = runs.iter().position(|run| run.visibility.painted());
    let longest = runs.iter().map(|run| run.advance).fold(0.0, f32::max);
    let digits = line_count.to_string().len();
    let gutter = advance(&"0".repeat(digits), style) + CODE_NUMBER_GAP;
    // A comment that runs past the card's far edge, so the wrap has something to break
    let inner = width - gutter;
    let rows = wrap(runs, inner, gutter, gutter);
    let numbers = rows.iter().enumerate().map(|(index, row)| index + 1);
    let painted = paint(numbers, rows, gutter);
    ```

    ---

    Inline math such as $a^2 + b^2 = c^2$ is a mark; a display formula is a block:

    $$
    \int_0^1 x^2 \, dx
    $$

## The math page: what size and ink each design system gave the formula.

gallery-math-display-caption = display — { $size }, { $ink }
gallery-math-inline-caption = inline — { $size }, { $ink }
gallery-math-refused-caption = refused source — hover the boxed run
gallery-math-structured-caption = structured — click a slot and type into it
