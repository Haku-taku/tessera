#!/usr/bin/env python3
"""Turn a Material design-system token export into a usable Rust spec function.

Two subcommands:

    inspect  Print the control's states / objects / properties and the full
             state -> object -> property table as JSON.
    gen      Emit a lean Rust module: per-object structs plus one function whose
             boolean parameters are the non-default states. It starts from the
             default values and overlays each active state in priority order.

Token names look like `md.comp.<control>.<states...>.<object>.<property>`; the object may
itself contain dots (`track.outline`, `focus.indicator`) and may be absent entirely
(`md.comp.divider.color`, where the control is its own object). A state is any segment that
is not part of the matched object, so the vocabulary does not have to be declared up front —
unknown states are *reported* instead of silently dropping the token.

Deprecated tokens (`deprecationMessage` on the token) are skipped.
"""
from __future__ import annotations

import argparse
import contextlib
import io
import json
import os
import re
import subprocess
import sys
import tempfile

# Object vocabularies per control; `""` means the control itself (no object segment).
# Objects are matched longest-first, so `track.outline` wins over `track`.
OBJECTS_BY_CONTROL = {
    "md.comp.switch": ["track.outline", "track", "handle", "icon", "state-layer", "focus.indicator"],
    "md.comp.slider": [
        "track.inner-corner", "track.outer-corner", "stop-indicator.container",
        "value-indicator.label.label-text", "label.label-text", "value-indicator.container",
        "track.shape", "handle.outline", "label.container", "stop-indicator",
        "state-layer", "container", "handle", "track", "icon", "label", "value-indicator", "stop",
    ],
    "md.comp.checkbox": [
        "focus.indicator.outline", "focus.indicator", "container.outline",
        "container", "outline", "icon", "state-layer",
    ],
    "md.comp.radio-button": ["icon", "state-layer", ""],
    # The button group's own tokens are the corner radius and the gap; the member's colours
    # come from `md.comp.button`, so no state-layer or icon vocabulary is needed here.
    "md.comp.button-group.connected": ["container", ""],
    "md.comp.button-group.standard": ["container", ""],
    # Segmented buttons keep the member's own parts: one shared outline, per-segment state.
    "md.comp.outlined-segmented-button": [
        "focus.indicator.outline", "focus.indicator", "label-text", "state-layer",
        "outline", "container", "icon",
    ],
    # Progress indicator: `circular`/`linear` are shape variants, not states, and the 4dp
    # thickness tokens are deprecated upstream — the numbers come only from the deprecations.
    "md.comp.progress-indicator": [
        "active-indicator", "track", "stop-indicator",
    ],
    # Navigation bar: `active` is a real state here (the export's base is `inactive`), so the
    # selected item reads a different colour, weight and state layer from its neighbours.
    "md.comp.navigation-bar": [
        "active-indicator", "focus.indicator.outline", "focus.indicator",
        "label-text", "state-layer", "container", "icon",
    ],
    # Navigation rail: the same destination as the bar, stacked in a fixed-width column; with
    # no label the indicator becomes a 56×56 circle instead of the 56×32 capsule.
    "md.comp.navigation-rail": [
        "active-indicator", "label-text", "state-layer", "container", "icon",
    ],
    # Navigation drawer: `standard`/`modal` are variant states on the container's colour and
    # elevation; the deprecated `container.color`/`scrim.*` must not be revived.
    "md.comp.navigation-drawer": [
        "active-indicator", "bottom.container", "focus.indicator.outline", "focus.indicator",
        "large-badge-label", "label-text", "headline", "state-layer", "container", "icon",
    ],
    # The date input's surface: a header band (headline over supporting text) and the
    # container. `header.headline` must beat a bare `headline` match, or `header` reads as
    # a state and the band loses its own object.
    "md.comp.date-input.modal": [
        "header.container", "header.headline", "header.supporting-text",
        "header", "container",
    ],
    # The docked picker's tree: the day cell, the header band, the month/year buttons, the
    # month and year menus, and the weekday row. `date`, `header`, `menu-button`, `menu`
    # and `weekdays` name parts of the picker, not appearances of one.
    "md.comp.date-picker.docked": [
        "date.container.outline", "date.container", "date.state-layer", "date.label-text",
        "menu-button.container", "menu-button.icon", "menu-button.label-text",
        "menu-button.state-layer",
        "menu.list-item.container", "menu.list-item.label-text", "menu.list-item.state-layer",
        "menu.list-item.leading-icon",
        "header.container", "header.headline", "header.supporting-text", "header",
        "weekdays.label-text",
        "container.surface-tint-layer", "container",
    ],
    # The bottom sheet's tree: the surface, the handle that says it drags, and the focus ring.
    # `docked` is where the export hangs every token, not an appearance of its own.
    "md.comp.sheet.bottom.docked": [
        "focus.indicator.outline", "focus.indicator", "drag-handle",
        "container.surface-tint-layer", "container",
    ],
    # The side sheet's tree: the panel, the line on its inner edge, the header and the
    # action row. `docked` is where the export hangs every token; `detached` is its base.
    "md.comp.sheet.side": [
        "focus.indicator.outline", "focus.indicator", "divider", "headline",
        "action.label-text", "action.state-layer",
        "container.surface-tint-layer", "container",
    ],
    # The modal picker's table carries only what its form adds: the taller container and the
    # two-line header. Its day grid is the docked tree repeated, so it is excluded here and
    # the control keeps reading the docked table for the body.
    "md.comp.date-picker.modal": [
        "header.container", "header.headline", "header.supporting-text", "header",
        "container.surface-tint-layer", "container",
    ],
    # The range form adds a band and a month subhead. `active-indicator` and `month` are parts
    # of the calendar, not appearances of one, so they stay in the vocabulary.
    "md.comp.date-picker.modal.range-selection": [
        "header.container", "header.headline", "header",
        "active-indicator.container", "active-indicator",
        "month.subhead", "month",
        "date.state-layer", "date.label-text", "date",
        "container.surface-tint-layer", "container",
    ],
    # The icon button: its box, the mark, the outlined variant's stroke, the layer and the ring.
    # The width tokens hang at the root (`leading-space` has no object of its own).
    "md.comp.icon-button": [
        "focus.indicator.outline", "focus.indicator",
        "container", "icon", "outline", "state-layer", "",
    ],
    # Toolbar geometry and its two member themes; the container's colour lives in the theme
    # table, not the geometry one (the docked `container.color` is deprecated).
    "md.comp.toolbar.docked": ["container"],
    "md.comp.toolbar.floating": ["container", "horizontal.container", "vertical.container"],
    "md.comp.toolbar.standard": [
        "button.container", "label-text", "state-layer", "container", "icon",
    ],
    "md.comp.toolbar.vibrant": [
        "button.container", "label-text", "state-layer", "container", "icon",
    ],
    # The floating toolbar's FAB group is a control of its own: the bar's own table
    # excludes that subtree, and this table carries its geometry and both themes. Its
    # sizes stay states of `container`/`icon`; new object names would leak into every
    # unregistered prefix, which borrows this whole vocabulary.
    "md.comp.toolbar.floating.fab": ["container", "icon", ""],
    # Snackbar: the body text is `supporting-text`; `label-text` exists only under `action`.
    "md.comp.snackbar": [
        "action.label-text", "action.state-layer", "supporting-text",
        "icon.state-layer", "container", "icon",
    ],
    # Plain tooltip: an inverse-surface panel and its label, with no states. The rich
    # variant carries a title and an action and is a control of its own; its surface-tint
    # layer is deprecated upstream, but naming it keeps `container` out of its path.
    "md.comp.plain-tooltip": ["container", "supporting-text"],
    "md.comp.rich-tooltip": [
        "container.surface-tint-layer", "action.state-layer", "action.label-text",
        "supporting-text", "subhead", "container",
    ],
    "md.comp.divider": [""],
    "md.comp.badge": ["label-text", ""],
    "md.comp.secondary-navigation-tab": [
        "active-indicator", "focus.indicator.outline", "focus.indicator", "label-text",
        "state-layer", "container", "divider", "icon",
    ],
    "md.comp.elevated-card": [
        "container.surface-tint-layer", "focus.indicator.outline", "focus.indicator",
        "container", "state-layer", "outline", "icon",
    ],
    "md.comp.filled-card": [
        "container.surface-tint-layer", "focus.indicator.outline", "focus.indicator",
        "container", "state-layer", "outline", "icon",
    ],
    "md.comp.outlined-card": [
        "container.surface-tint-layer", "focus.indicator.outline", "focus.indicator",
        "container", "state-layer", "outline", "icon",
    ],
    "md.comp.assist-chip": [
        "focus.indicator.outline", "focus.indicator", "container.surface-tint-layer",
        "container", "label-text", "state-layer", "outline", "icon",
    ],
    "md.comp.suggestion-chip": [
        "focus.indicator.outline", "focus.indicator", "container.surface-tint-layer",
        "container", "label-text", "state-layer", "outline", "leading-icon",
    ],
    "md.comp.filter-chip": [
        "focus.indicator.outline", "focus.indicator", "container.surface-tint-layer",
        "container", "label-text", "state-layer", "outline", "icon",
        "leading-icon", "trailing-icon",
    ],
    "md.comp.input-chip": [
        "focus.indicator.outline", "focus.indicator", "container.surface-tint-layer",
        "container", "label-text", "state-layer", "outline",
        "leading-icon", "trailing-icon", "avatar",
    ],
    "md.comp.fab-menu": [
        "close-button.container", "close-button.icon", "close-button.state-layer", "close-button",
        "menu-item.container", "menu-item.icon", "menu-item.label-text", "menu-item.state-layer",
        "menu-item",
        "list-item.container", "list-item.icon", "list-item.label-text", "list-item.state-layer",
        "list-item",
    ],
    # The split button's tree: two halves, the corners where they meet, and the arrow.
    # `trailing-button` is both a half and a section around the arrow's size.
    "md.comp.split-button": [
        "inner-corner", "outer-corner", "leading-button", "trailing-button",
        # The open menu's corner is the trailing half's alone: the shared `inner-corner`
        # must not inherit it, or the leading half would turn into a pill too.
        "trailing-button.inner-corner", "trailing-button.icon", "container",
    ],
    # The FAB's tree: the box, the mark, the layer and the ring. `branded` is excluded, so
    # its indicator badge stays out of the vocabulary too.
    "md.comp.fab": [
        "focus.indicator.outline", "focus.indicator", "container", "icon", "state-layer",
    ],
    "md.comp.list": [
        "focus.indicator.outline", "focus.indicator",
        "leading-icon.icon", "trailing-icon.icon",
        "label-text", "overline", "supporting-text", "trailing-supporting-text",
        "leading-icon", "trailing-icon", "leading-avatar", "leading-avatar-label",
        "leading-image", "leading-video",
        "state-layer", "container", "divider", "drop-zone", "segmented", "gap",
    ],
    "md.comp.dialog": [
        "headline", "supporting-text", "label-text", "state-layer", "container", "icon",
    ],
    # A menu: the item's own parts, the icon button some items carry, and the group that
    # holds sections. `standard`/`vibrant`/`horizontal` are sections of the export, not
    # states; see SECTION_SEGMENTS and EXCLUDED_SUBTREES.
    "md.comp.menus": [
        "menu-item.state-layer", "menu-item.label-text", "menu-item.supporting-text",
        "menu-item.trailing-supporting-text", "menu-item.leading-icon",
        "menu-item.trailing-icon", "menu-item.focus.indicator.outline",
        "menu-item.focus.indicator", "menu-item.container", "menu-item.inner-corner",
        "icon-button.state-layer", "icon-button.container", "icon-button.icon",
        "focus.indicator.outline", "focus.indicator",
        "section-label-text", "group", "container", "icon", "menu-item",
    ],
    "md.comp.outlined-text-field": [
        "focus.indicator.outline", "focus.indicator",
        "input-text.placeholder", "input-text.prefix", "input-text.suffix",
        "label-text.populated",
        "supporting-text", "leading-icon", "trailing-icon",
        "input-text", "label-text", "caret", "outline", "container",
    ],
    "md.comp.filled-text-field": [
        "focus.indicator.outline", "focus.indicator",
        "input-text.placeholder", "input-text.prefix", "input-text.suffix",
        "label-text.populated",
        "supporting-text", "leading-icon", "trailing-icon", "active-indicator",
        "state-layer", "input-text", "label-text", "caret", "container",
    ],
}


def objects_for(prefixes: list[str]) -> list[str]:
    objs: set[str] = set()
    for prefix in prefixes:
        known = OBJECTS_BY_CONTROL.get(prefix)
        if known is None:
            # The empty object means "no object path at all"; borrowing it would swallow every
            # path segment as a state, so it only applies to controls that declare it.
            known = [o for group in OBJECTS_BY_CONTROL.values() for o in group if o]
        objs.update(known)
    return sorted(objs, key=lambda o: (-len(o.split(".")), -len(o)))


# The default state: every other state becomes a parameter or a variant enum value.
DEFAULT_STATE = "unselected"
# An export with no unconditional token would otherwise take the alphabetically first state
# as its base; a control listed here says which state, if any, is the base.
DEFAULT_BY_CONTROL: dict[str, str] = {"md.comp.split-button": ""}


def load(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)["system"]


def values_of(token: dict, values: list[dict]) -> list[dict]:
    prefix = token["name"] + "/values/"
    return [v for v in values if v["name"].startswith(prefix)]


def default_value(vals: list[dict]) -> dict | None:
    if not vals:
        return None
    bare = [v for v in vals if not v.get("contextTags")]
    if bare:
        return bare[-1]
    return min(vals, key=lambda v: len(v.get("contextTags", [])))


def deprecated(token: dict) -> bool:
    return "deprecationMessage" in token


def argb(color: dict) -> int:
    def q(x: float) -> int:
        return max(0, min(255, round(x * 255)))

    return (
        (q(color.get("alpha", 1.0)) << 24)
        | (q(color.get("red", 0)) << 16)
        | (q(color.get("green", 0)) << 8)
        | q(color.get("blue", 0))
    )


class Resolver:
    def __init__(self, tokens: list[dict], values: list[dict]):
        self.by_name = {t["tokenName"]: t for t in tokens}
        self.values = values

    def resolve(self, value: dict, depth: int = 0) -> tuple[str, object]:
        if depth > 8:
            return ("ref", "?")
        if "tokenName" in value:
            name = value["tokenName"]
            if name.startswith(("md.sys.color.", "md.ref.palette.")):
                return ("colour", name)
            token = self.by_name.get(name)
            if token is not None:
                nxt = default_value(values_of(token, self.values))
                if nxt is not None:
                    return self.resolve(nxt, depth + 1)
            return ("ref", name)
        if "length" in value:
            # Some exported lengths carry only a unit and no number; there is nothing to emit.
            if "value" not in value["length"]:
                return ("missing", None)
            return ("length", float(value["length"]["value"]))
        if "opacity" in value:
            return ("opacity", float(value["opacity"]))
        if "color" in value:
            return ("colour_hex", argb(value["color"]))
        if "shape" in value:
            family = value["shape"].get("family", "")
            return ("shape", family.replace("SHAPE_FAMILY_", "").lower())
        if "elevation" in value:
            return ("elevation", float(value["elevation"].get("value", 0)))
        for numeric in ("fontSize", "lineHeight", "fontTracking", "fontWeight"):
            if numeric in value:
                # The export writes these as bare numbers, not `{value: n}`.
                raw = value[numeric]
                if isinstance(raw, dict):
                    raw = raw.get("value", 0)
                return ("length", float(raw))
        if "fontNames" in value:
            names = value["fontNames"].get("values") or ["?"]
            return ("font", names[0])
        if "type" in value:
            # The scale name lives in the size token's name, not its value (a bare number).
            # A component may point at its own `size` token first, so follow the chain of
            # names until a `md.sys.typescale.<name>.size` shows up.
            name = value["type"].get("fontSizeTokenName", "")
            for _ in range(4):
                parts = name.split(".")
                if len(parts) >= 5 and parts[:3] == ["md", "sys", "typescale"]:
                    return ("scale", parts[3])
                token = self.by_name.get(name)
                nxt = default_value(values_of(token, self.values)) if token else None
                if not nxt or "tokenName" not in nxt:
                    break
                name = nxt["tokenName"]
            return ("scale", "?")
        return ("ref", "?")


def split_token(
    name: str, prefix: str, objects: list[str], sections: frozenset[str]
) -> tuple[tuple[str, ...], str, str] | None:
    """Split `<states...>.<object>.<property>`; the export also writes `<object>.<states...>`
    and `<object-head>.<states...>.<object-tail>` (e.g. `close-button.focused.container`)."""
    if not name.startswith(prefix + "."):
        return None
    body = name[len(prefix) + 1 :].split(".")
    if not body or not body[0]:
        return None
    prop = body[-1]
    head = body[:-1]
    # Several objects can match one path (`segmented.container` vs `segmented` + `container`);
    # the longest object wins, then the match that leaves the object closest to the property.
    best: tuple[tuple[int, int], list[str], str] | None = None
    for obj in objects:
        parts = obj.split(".") if obj else []
        if not parts:
            # The empty object is the last resort, not a candidate: `md.comp.button.icon-label-space`
            # is the control's own property, but `md.comp.button.container.color` is not.
            continue
        if len(head) < len(parts):
            continue
        states: list[str] | None = None
        kind = 0
        if head[-len(parts) :] == parts:
            # Object right before the property.
            states, kind = head[: -len(parts)], 3
        elif head[: len(parts)] == parts:
            # Object right after the prefix.
            states, kind = head[len(parts) :], 1
        else:
            tail = parts[1:]
            if tail and head[0] == parts[0] and head[-len(tail) :] == tail:
                # States wedged between the object's head and tail: `close-button.focused.container`.
                states, kind = head[1 : -len(tail)], 2
            elif len(head) > len(parts):
                # Object segments in order but not contiguous, with states anywhere between
                # (`primary.close-button.hovered.container`). Matched greedily from the left.
                taken: list[int] = []
                at = 0
                for part in parts:
                    while at < len(head) and head[at] != part:
                        at += 1
                    if at == len(head):
                        break
                    taken.append(at)
                    at += 1
                if len(taken) == len(parts):
                    states = [s for i, s in enumerate(head) if i not in taken]
        if states is None:
            continue
        # A name that is both an object and a section (`trailing-button`) is the weaker
        # reading: `inner-corner selected trailing-button` names the corner, not the half.
        key = (len(parts), obj not in sections, kind)
        if best is None or key > best[0]:
            best = (key, states, obj)
    if best is None:
        # Nothing matched: the whole head is states and the control itself is the object.
        return tuple(sorted(set(head))), "", prop
    return tuple(sorted(set(best[1]))), best[2], prop


def rust_float(x: float) -> str:
    s = f"{x:g}"
    return s if ("." in s or "e" in s or "E" in s) else s + ".0"


def to_camel(kebab: str) -> str:
    parts = kebab.split("-")
    return parts[0] + "".join(p.capitalize() for p in parts[1:])


COLOUR_PREFIX = "md.sys.color."


def pack_role(token: str) -> str:
    # `md.sys.color.primary-container` is the very token our pack calls `primaryContainer`.
    if token.startswith(COLOUR_PREFIX):
        return to_camel(token[len(COLOUR_PREFIX) :])
    return token


def rust_type(kind: str) -> str:
    # The declared type can lie about the payload: `label-text.font` is declared FONT_SIZE but
    # resolves to a scale name. A string payload always wins.
    if kind in ("colour", "colour_hex", "shape", "font", "scale", "ref"):
        return "&'static str"
    return "f32"


def value_literal(kind: str, payload: object) -> str:
    if kind == "colour":
        return f'"{pack_role(payload)}"'
    if kind == "shape":
        return f'"{payload}"'
    if kind == "colour_hex":
        return f'"#{payload:08x}"'
    if isinstance(payload, str):
        return f'"{payload}"'
    return rust_float(payload)


def all_states_of(entries: list[dict]) -> set[str]:
    return {s for e in entries for s in e["states"]}


def used_default(entries: list[dict], prefix: str = "") -> str:
    if prefix in DEFAULT_BY_CONTROL:
        return DEFAULT_BY_CONTROL[prefix]
    used = all_states_of(entries)
    if DEFAULT_STATE in used:
        return DEFAULT_STATE
    if "inactive" in used:
        return "inactive"
    # A control whose base tokens carry no state at all: the empty set is the default.
    if any(not e["states"] for e in entries):
        return ""
    return sorted(used)[0] if used else ""


def build(system: dict, prefixes: list[str]) -> dict:
    tokens = system["tokens"]
    values = system["values"]
    resolver = Resolver(tokens, values)
    entries: list[dict] = []
    skipped: list[str] = []
    skipped_empty: list[str] = []
    skipped_unsupported: list[str] = []
    unknown: set[str] = set()
    for prefix in prefixes:
        if prefix not in OBJECTS_BY_CONTROL:
            # Borrowing hides a missing vocabulary behind plausible fields, and one control's
            # vocabulary can claim another's segments (`horizontal.container` in a menu).
            print(
                f"gen: {prefix} has no object vocabulary; every control's objects are borrowed",
                file=sys.stderr,
            )
        objects = objects_for([prefix])
        sections = SECTION_SEGMENTS.get(prefix, frozenset())
        for token in tokens:
            parsed = split_token(token["tokenName"], prefix, objects, sections)
            if parsed is None:
                continue
            if deprecated(token):
                skipped.append(token["tokenName"])
                continue
            value = default_value(values_of(token, values))
            if value is None:
                continue
            kind, payload = resolver.resolve(value)
            if kind == "missing":
                skipped_empty.append(token["tokenName"])
                continue
            states, obj, prop = parsed
            # The export spells the same interaction two ways across themes: the `surface`
            # family writes `hover`/`focus`, every other family `hovered`/`focused`.
            states = tuple(STATE_ALIASES.get(s, s) for s in states)
            section = SECTION_SEGMENTS.get(prefix, frozenset())
            states = tuple(s for s in states if s not in section)
            if EXCLUDED_SUBTREES.get(prefix, frozenset()) & set(states):
                skipped_unsupported.append(token["tokenName"])
                continue
            if IGNORED_SEGMENTS & set(states):
                skipped_unsupported.append(token["tokenName"])
                continue
            for state in states:
                if state not in KNOWN_STATES:
                    unknown.add(state)
            entries.append(
                {
                    "states": states,
                    "object": obj,
                    "property": prop,
                    "kind": kind,
                    "vtype": token["tokenValueType"],
                    "value": payload,
                    "source": prefix,
                }
            )
    entries.sort(key=lambda e: (e["states"], e["object"], e["property"]))
    all_states = sorted({s for e in entries for s in e["states"]})
    default = used_default(entries, prefix)
    return {
        "prefix": prefixes[0],
        "prefixes": prefixes,
        "control": prefixes[0].split(".")[-1],
        "token_count": len(tokens),
        "skipped_deprecated": skipped,
        "skipped_empty": skipped_empty,
        "skipped_unsupported": skipped_unsupported,
        "unknown_states": sorted(unknown),
        "states": {
            "default": default,
            "all": all_states,
            "params": [s for s in all_states if s != default],
        },
        "entries": entries,
    }


def cmd_inspect(system: dict, prefixes: list[str]) -> int:
    model = build(system, prefixes)
    objects: dict[str, dict[str, str]] = {}
    for e in model["entries"]:
        objects.setdefault(e["object"], {})[e["property"]] = rust_type(e["kind"])
    out = {
        "control": model["control"],
        "prefix": model["prefix"],
        "prefixes": model["prefixes"],
        "token_count": model["token_count"],
        "default_state": model["states"]["default"],
        "params": model["states"]["params"],
        "unknown_states": model["unknown_states"],
        "skipped_deprecated": model["skipped_deprecated"],
        "skipped_empty": model["skipped_empty"],
        "objects": objects,
        "entries": [
            {
                "states": list(e["states"]),
                "object": e["object"],
                "property": e["property"],
                "type": rust_type(e["kind"]),
                "value": e["value"],
            }
            for e in model["entries"]
        ],
    }
    json.dump(out, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")
    return 0


# Overlay priority: earliest wins first, later states override earlier ones.
PRIORITY = [
    # The primary axis and its siblings come first, so the interaction states can override.
    "selected", "checked", "active", "expanded", "current", "on", "indeterminate", "error",
    "xsmall", "small", "medium", "large", "xlarge",
    "with-icon", "with-label", "with-tick-marks", "with-overlap", "has-icon", "centered",
    "docked", "modal", "full-screen", "contained",
    # The export spells these two both ways (`hover` / `hovered`); both must keep their slot.
    "hover", "hovered", "focus", "focused", "pressed", "dragged", "disabled",
]
# Per-control overrides: a control whose axes collide with the shared order.
PRIORITY_BY_CONTROL: dict[str, list[str]] = {}
# Mutually exclusive state axes per control: each becomes one enum parameter, not bools.
# A control may have several axes (a button has both a style and a size).
VARIANT_GROUPS: dict[str, list[tuple[str, list[str]]]] = {
    "md.comp.slider": [("size", ["xsmall", "small", "medium", "large", "xlarge"])],
    "md.comp.app-bar": [("size", ["small", "medium", "large"])],
    # The FAB menu repeats the FAB's colour role: the close button takes it directly, the list
    # items take its `-container` form.
    "md.comp.assist-chip": [("variant", ["flat", "elevated"])],
    "md.comp.suggestion-chip": [("variant", ["flat", "elevated"])],
    "md.comp.filter-chip": [("variant", ["flat", "elevated"])],
    "md.comp.input-chip": [("variant", ["flat", "elevated"])],
    "md.comp.fab-menu": [
        ("color", ["primary", "secondary", "tertiary"]),
        ("container", ["primary-container", "secondary-container", "tertiary-container"]),
    ],
    "md.comp.button": [
        ("style", ["elevated", "filled", "outlined", "text", "tonal"]),
        ("size", ["xsmall", "small", "medium", "large", "xlarge"]),
        # The export's button is a *toggle* button: `unselected` is a real appearance, not the
        # stateless base, so a plain button needs `Base` to keep the plain tokens.
        ("toggle", ["unselected", "selected"]),
    ],
    # The connected group's only tokens are per-size corner and gap values, so size is an axis.
    "md.comp.button-group.connected": [
        ("size", ["xsmall", "small", "medium", "large", "xlarge"]),
    ],
    # A progress indicator is either a line or a ring, never both: two bools would admit
    # `circular && linear`, an axis cannot. `with-wave` is the expressive form, whose only
    # own values are the taller container and the larger ring.
    "md.comp.progress-indicator": [
        ("shape", ["linear", "circular"]),
        ("wave", ["with-wave"]),
    ],
    "md.comp.list": [
        ("lines", ["one-line", "two-line", "three-line"]),
        # Selection is an axis, not the primary bool: the export spells the unselected item
        # out as a real appearance (`unselected`), so both sides need a name.
        ("selection", ["unselected", "selected"]),
        ("layout", ["segmented"]),
        ("variant", ["expressive"]),
        ("video", ["small", "large"]),
    ],
    # The icon slot is a build-time choice: with an icon the headline centres and the icon
    # takes the secondary colour.
    "md.comp.dialog": [("icon_slot", ["with-icon"])],
    # A sheet is modal or standard, never both: the two forms differ only in elevation and,
    # in the app, in whether a scrim sits behind them.
    "md.comp.sheet.bottom.docked": [("form", ["standard", "modal"])],
    # The side sheet has the same two forms: docked into the page or floating over it.
    "md.comp.sheet.side": [("form", ["standard", "modal"])],
    # An icon button has three independent axes: the colour style, the selection of a toggle
    # (the export's base is the plain button, `unselected` a real appearance) and the size and
    # width that together name the box.
    "md.comp.icon-button": [
        ("style", ["standard", "filled", "tonal", "outlined"]),
        ("selection", ["unselected", "selected"]),
        ("size", ["xsmall", "small", "medium", "large", "xlarge"]),
        ("width", ["default", "narrow", "wide"]),
    ],
    # The split button's halves have one height per size and no other appearance axis.
    "md.comp.split-button": [
        ("size", ["xsmall", "small", "medium", "large", "xlarge"]),
    ],
    # A FAB names a size, a colour family and whether it is lowered; the export's base is
    # the 56 dp box, so `Base` carries the geometry and no colour. The `surface` family
    # re-declares that box, so colour is applied before size or a surface FAB would ignore
    # its size.
    "md.comp.fab": [
        ("color", [
            "primary-container", "secondary-container", "tertiary-container",
            "primary", "secondary", "tertiary", "surface",
        ]),
        ("size", ["small", "medium", "large"]),
        ("lowering", ["lowered"]),
    ],
    "md.comp.toolbar.floating.fab": [
        ("theme", ["standard", "vibrant"]),
        # The 80dp medium size; its geometry overlays the 56dp base.
        ("size", ["medium"]),
    ],
}

# Path segments that name a section of the design tool's tree, not a control state.
# They carry shape *names* and spring constants, which are not paintable properties.
IGNORED_SEGMENTS = {"shape", "corner-size", "motion", "spring"}

# Section segments carry no meaning of their own; the token under them still applies.
# `md.comp.list.list-item.hover.label-text.color` is the label's hover colour, not a
# "list-item" state.
SECTION_SEGMENTS: dict[str, frozenset[str]] = {
    "md.comp.list": frozenset({"list-item"}),
    # The wavy progress form's amplitude and wavelength belong to the active indicator of
    # every wavy bar; `wave` names that part, and `with-wave` carries the form itself.
    "md.comp.progress-indicator": frozenset({"wave"}),
    # A dialog's actions are always text buttons; `action` names the section, not an appearance.
    "md.comp.dialog": frozenset({"action"}),
    # The export splits the menu into `standard` and `vibrant` colour schemes. This build
    # paints the standard one, so its segment carries no meaning of its own.
    "md.comp.menus": frozenset({"standard"}),
    # Every side-sheet token hangs under `docked`; the form axis carries the appearance.
    "md.comp.sheet.side": frozenset({"docked", "action"}),
}

# Whole interaction models this package does not implement yet; keeping their tokens would
# only add parameters no widget reads.
EXCLUDED_SUBTREES: dict[str, frozenset[str]] = {
    "md.comp.list": frozenset({"reveal", "reorder", "expand"}),
    # The floating toolbar's FAB group is its own control, not a state of the bar.
    "md.comp.toolbar.floating": frozenset({"fab"}),
    # The modal picker's day grid repeats the docked one; its year list and range mode are
    # forms this build does not draw yet.
    "md.comp.date-picker.modal": frozenset({
        "date", "weekdays", "range-selection", "year-selection",
    }),
    # The engine's menu is vertical with the standard scheme; the horizontal layout and the
    # vibrant scheme are token subtrees this build does not paint.
    "md.comp.menus": frozenset({"horizontal", "vibrant"}),
    # The branded FAB is a multi-stop gradient the pack cannot resolve; its badge goes with it.
    "md.comp.fab": frozenset({"branded"}),
    # `minimized` is the box while the sheet is hidden; a hidden sheet is a removed node here.
    "md.comp.sheet.bottom.docked": frozenset({"minimized"}),
    # The detached side sheet is the same panel inset from every edge; this pack docks it.
    "md.comp.sheet.side": frozenset({"detached"}),
}

# Two spellings of one interaction, folded before the axes read them.
STATE_ALIASES = {"hover": "hovered", "focus": "focused"}

# Every state the export is known to use; an unlisted one is reported, not dropped.
KNOWN_STATES = set(PRIORITY) | {
    DEFAULT_STATE, "inactive", "enabled", "no-label", "standard",
    "icon", "with-single-line", "with-two-lines", "first-child", "last-child",
    "outside-month", "today", "in-range",
} | {
    s for groups in VARIANT_GROUPS.values() for _, states in groups for s in states
}

def variant_case(state: str) -> str:
    if state.startswith("x") and len(state) > 1:
        # `xsmall` reads as `XSmall`, not `Xsmall`.
        return "X" + state[1:].capitalize()
    # An axis name can carry an underscore (`icon_slot`), not just a hyphen.
    words = [w for w in state.replace("_", "-").split("-") if w]
    return "".join(w.capitalize() for w in words)


PARAM_RENAMES = {"hover": "hovered", "focus": "focused", "dragged": "dragging"}


def param_name(state: str) -> str:
    return PARAM_RENAMES.get(state, state).replace("-", "_")


def priority_for(prefix: str) -> list[str]:
    return PRIORITY_BY_CONTROL.get(prefix, PRIORITY)


def prio(state: str, order: list[str]) -> int:
    if state in order:
        return order.index(state)
    # An unlisted state behaves like an axis: it must be applied before the interaction
    # states so hover/focus/pressed/disabled still override it. A control's own order may
    # not list `hover` at all, so take whichever spelling of the first interaction it has.
    for interaction in ("hover", "hovered"):
        if interaction in order:
            return order.index(interaction) - 1
    return len(order)


def state_plan(entries: list[dict], prefix: str = "") -> dict:
    used = all_states_of(entries)
    groups = []
    for param, states in VARIANT_GROUPS.get(prefix, []):
        present = [s for s in states if s in used]
        if present:
            groups.append((param, present))
    variant_of = {state: param for param, states in groups for state in states}
    default = used_default(entries, prefix)
    single = {e["states"][0] for e in entries if len(e["states"]) == 1}
    primary = next(
        (
            s
            for s in ("selected", "checked", "active")
            if s != default and s in single and s not in variant_of
        ),
        None,
    )
    order = priority_for(prefix)
    flags = sorted(
        used - {default, *variant_of} - ({primary} if primary else set()),
        key=lambda s: (prio(s, order), s),
    )
    # `hover` and `hovered` are the same switch under two spellings; one parameter serves both.
    seen: set[str] = set()
    unique: list[str] = []
    for state in flags:
        param = param_name(state)
        if param not in seen:
            seen.add(param)
            unique.append(state)
    flags = unique
    return {
        "default": default,
        "primary": primary,
        "flags": flags,
        "groups": groups,
        "variant_of": variant_of,
        "order": order,
        # A variant axis declared later wins where two axes write the same field: the `surface`
        # FAB family re-declares the base box, so size has to be applied after colour.
        "axis_rank": {
            state: index
            for index, (_, states) in enumerate(groups)
            for state in states
        },
    }


def condition(states: tuple[str, ...], plan: dict) -> str:
    parts = []
    for state in states:
        if state in plan["variant_of"]:
            # `matches!` is const-friendly; `==` needs a non-const `PartialEq`.
            param = plan["variant_of"][state]
            parts.append(
                f"matches!({param}, {plan['types'][param]}::{variant_case(state)})"
            )
        elif state == plan["default"]:
            # The default only needs guarding when it is the opposite of a primary axis.
            if plan["primary"]:
                parts.append(f"!{param_name(plan['primary'])}")
        else:
            parts.append(param_name(state))
    return " && ".join(parts)


def key_order(key: tuple[str, ...], plan: dict) -> tuple[int, int, int]:
    # Base, then pure-axis values in their declared order, then any key carrying an
    # interaction switch: hover and press must override the axis they sit on.
    if not key:
        return (-2, 0, 0)
    order = plan["order"]
    axis_only = all(s in plan["variant_of"] for s in key)
    rank = max(plan["axis_rank"].get(s, -1) for s in key)
    if axis_only:
        return (0, len(key), rank)
    # The default's own guard sits with the primary axis, before every other switch.
    scores = [
        prio(plan["primary"], order) if s == plan["default"] else prio(s, order) for s in key
    ]
    # A key naming more states is more specific, so it is applied later.
    return (1, max(scores), len(key), rank)


def formatted(text: str) -> str:
    """`text` as rustfmt would leave it: formatting is not a difference in a table."""
    with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False, encoding="utf-8") as handle:
        handle.write(text)
        path = handle.name
    try:
        done = subprocess.run(
            ["rustfmt", "--edition", "2021", path],
            capture_output=True,
            text=True,
        )
        if done.returncode != 0:
            return text
        with open(path, encoding="utf-8") as handle:
            return handle.read()
    except FileNotFoundError:
        return text
    finally:
        os.unlink(path)


def code_of(text: str) -> str:
    """Everything but the comment lines, which report the export rather than the table."""
    return "\n".join(line for line in text.splitlines() if not line.startswith("//"))


def cmd_gen(
    system: dict,
    prefixes: list[str],
    out: str,
    name: str | None = None,
    check: bool = False,
) -> int:
    model = build(system, prefixes)
    # The header reports deprecated and unknown-state tokens; these two lists are just as
    # much lost values, and a control that silently defaults to 0.0/"" is a wrong table.
    if model["skipped_empty"]:
        names = ", ".join(model["skipped_empty"][:4])
        print(
            f"gen: dropped {len(model['skipped_empty'])} token(s) whose export value is blank: {names}",
            file=sys.stderr,
        )
    if model["skipped_unsupported"]:
        print(
            f"gen: dropped {len(model['skipped_unsupported'])} token(s) under an excluded segment",
            file=sys.stderr,
        )
    entries = model["entries"]
    plan = state_plan(entries, prefixes[0])
    # A table on disk names its own control, and a prefix can carry a section suffix that is
    # not part of it (`md.comp.date-picker.docked`); an in-place rewrite keeps that name.
    if name is None and out != "-":
        try:
            with open(out, encoding="utf-8") as handle:
                existing = re.search(r"pub const fn (\w+)_spec", handle.read())
        except FileNotFoundError:
            existing = None
        if existing:
            name = existing.group(1).replace("_", "-")
    # The last path segment is usually the control's name, but a prefixed subtree can share it
    # with a sibling (`button-group.connected.medium` reads as `medium`); `--name` fixes that.
    control = name or model["control"]
    camel = "".join(p.capitalize() for p in control.split("-"))
    # One enum per mutually exclusive axis, named after the control and the axis.
    plan["types"] = {param: camel + variant_case(param) for param, _ in plan["groups"]}

    objects: dict[str, dict[str, str]] = {}
    for e in entries:
        objects.setdefault(e["object"], {})[e["property"]] = rust_type(e["kind"])

    def struct_name(obj: str) -> str:
        if not obj:
            return camel + "Root"
        return camel + "".join(p.capitalize() for p in obj.replace(".", "-").split("-"))

    KEYWORDS = {"type", "impl", "match", "self", "crate", "const", "fn", "mod", "enum"}

    def field(name: str) -> str:
        if not name:
            return "root"
        clean = name.replace("-", "_").replace(".", "_")
        return f"{clean}_" if clean in KEYWORDS else clean

    def default_for(obj: str, prop: str) -> str:
        if objects[obj][prop] == "&'static str":
            return '""'
        if prop == "opacity" and obj != "state-layer":
            return "1.0"
        return "0.0"

    def assignments(key: tuple[str, ...], indent: str) -> list[str]:
        rows = [e for e in entries if e["states"] == key]
        return [
            f"{indent}out.{field(e['object'])}.{field(e['property'])} = "
            f"{value_literal(e['kind'], e['value'])};"
            for e in sorted(rows, key=lambda e: (e["object"], e["property"]))
        ]

    keys = sorted({e["states"] for e in entries}, key=lambda k: (key_order(k, plan), k))
    # A primary axis that became a variant value needs no boolean of its own.
    primary_flag = [plan["primary"]] if plan["primary"] and plan["primary"] not in plan["variant_of"] else []
    params = primary_flag + plan["flags"]

    L: list[str] = [
        "// Generated by tokens/gen_spec.py from the design-system export. Do not edit by hand.",
        "",
    ]
    if model["skipped_deprecated"]:
        L.append(f"// Skipped {len(model['skipped_deprecated'])} deprecated tokens (see the export).")
        L.append("")
    if model["unknown_states"]:
        L.append(f"// States outside the known vocabulary: {', '.join(model['unknown_states'])}")
        L.append("")
    L.append("/// One resolved value set for a control state.")
    L.append("#[derive(Clone, Copy, Debug, PartialEq)]")
    L.append(f"pub struct {camel}Spec {{")
    for obj in sorted(objects):
        L.append(f"    pub {field(obj)}: {struct_name(obj)},")
    L.append("}")
    L.append("")
    for obj in sorted(objects):
        L.append(f"/// `{obj or 'the control itself'}` values.")
        L.append("#[derive(Clone, Copy, Debug, PartialEq)]")
        L.append(f"pub struct {struct_name(obj)} {{")
        for prop in sorted(objects[obj]):
            L.append(f"    pub {field(prop)}: {objects[obj][prop]},")
        L.append("}")
        L.append("")

    for param, states in plan["groups"]:
        variant_type = plan["types"][param]
        L.append("/// A mutually exclusive variant axis, not a boolean.")
        L.append("#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]")
        L.append(f"pub enum {variant_type} {{")
        L.append("    /// No variant token applied.")
        L.append("    #[default]")
        L.append("    Base,")
        for state in states:
            L.append(f"    {variant_case(state)},")
        L.append("}")
        L.append("")
    # Interaction switches travel as a named struct: two `bool`s bound by position swap
    # silently, and a caller that restates six of them is one edit away from the wrong state.
    # One switch has no order to confuse, so it stays a plain parameter.
    flags_type = f"{camel}Flags"
    named = len(params) > 1
    if named:
        L.append("/// Which interaction states are active; the rest stay at the base.")
        L.append("#[derive(Clone, Copy, Debug)]")
        L.append(f"pub struct {flags_type} {{")
        for param in params:
            L.append(f"    pub {param_name(param)}: bool,")
        L.append("}")
        L.append("")
        L.append(f"impl {flags_type} {{")
        L.append("    /// Every switch off: the base state.")
        L.append(f"    pub const NONE: Self = Self {{")
        for param in params:
            L.append(f"        {param_name(param)}: false,")
        L.append("    };")
        L.append("}")
        L.append("")
    bits = [f"flags: {flags_type}"] if named else [f"{param_name(p)}: bool" for p in params]
    for param, _ in plan["groups"]:
        bits.append(f"{param}: {plan['types'][param]}")
    signature = ", ".join(bits)
    doc_flags = ", ".join(f"`{param_name(p)}`" for p in params) or "none"
    for param, _ in plan["groups"]:
        doc_flags += f", `{param}`"
    L.append("/// Resolve the control's values for one state combination.")
    base = f"`{plan['default']}` is the base." if plan["default"] else "No state is the base."
    L.append(f"/// Flags by priority: {doc_flags}; {base}")
    L.append(f"pub const fn {control.replace('-', '_')}_spec({signature}) -> {camel}Spec {{")
    if named:
        names = ", ".join(param_name(p) for p in params)
        L.append(f"    let {flags_type} {{ {names} }} = flags;")
    L.append(f"    let mut out = {camel}Spec {{")
    for obj in sorted(objects):
        L.append(f"        {field(obj)}: {struct_name(obj)} {{")
        for prop in sorted(objects[obj]):
            L.append(f"            {field(prop)}: {default_for(obj, prop)},")
        L.append("        },")
    L.append("    };")
    for key in keys:
        rows = assignments(key, "        ")
        if not rows:
            continue
        # A state with no condition to test is the base itself: the default state when no
        # primary axis exists to negate. An `if` around it would emit `if  {`.
        cond = condition(key, plan) if key else ""
        if not cond:
            L.extend(assignments(key, "    "))
        else:
            L.append(f"    if {cond} {{")
            L.extend(rows)
            L.append("    }")
    L.append("    out")
    L.append("}")
    L.append("")


    text = "\n".join(L)
    if check:
        if out == "-":
            print("check: --check needs --out <file>", file=sys.stderr)
            return 2
        try:
            with open(out, encoding="utf-8") as handle:
                current = handle.read()
        except FileNotFoundError:
            print(f"check: {out} does not exist", file=sys.stderr)
            return 1
        if code_of(formatted(current)) != code_of(formatted(text)):
            # A table that was hand-edited, or generated before the export or vocabulary
            # changed, reads as current until someone regenerates it.
            print(f"check: {out} is stale", file=sys.stderr)
            return 1
        print(f"check: {out} is current")
        return 0
    if out == "-":
        sys.stdout.write(text)
    else:
        # Every checked-in table is rustfmt-ed; writing an unformatted one would make the
        # next `--check` pass only by accident and bury the real change in reflow noise.
        with open(out, "w", encoding="utf-8") as f:
            f.write(formatted(text))
    return 0


def cmd_audit(manifest: str) -> int:
    """Regenerate every listed table and name the ones that no longer match."""
    with open(manifest, encoding="utf-8") as handle:
        tables = json.load(handle)
    stale: list[str] = []
    for spec_file, entry in sorted(tables.items()):
        try:
            with open(spec_file, encoding="utf-8") as handle:
                current = handle.read()
        except FileNotFoundError:
            print(f"stale {spec_file}: listed in {manifest} but missing")
            stale.append(spec_file)
            continue
        # The file names its own control, so the manifest only has to record where the
        # values came from.
        match = re.search(r"pub const fn (\w+)_spec", current)
        name = match.group(1).replace("_", "-") if match else None
        with tempfile.NamedTemporaryFile(suffix=".rs", delete=False) as handle:
            path = handle.name
        try:
            with contextlib.redirect_stderr(io.StringIO()):
                cmd_gen(load(entry["export"]), entry["prefixes"], path, name)
            fresh = open(path, encoding="utf-8").read()
        finally:
            os.unlink(path)
        if code_of(formatted(current)) != code_of(fresh):
            print(f"stale {spec_file}: from {entry['export']} {entry['prefixes']}")
            stale.append(spec_file)
    if stale:
        print(f"audit: {len(stale)} of {len(tables)} tables are stale", file=sys.stderr)
        return 1
    print(f"audit: all {len(tables)} tables are current")
    return 0


def _fixture(tokens: list[tuple[str, dict]]) -> dict:
    """A minimal export: one token per name and one default value per token."""
    made: list[dict] = []
    values: list[dict] = []
    for index, (name, value) in enumerate(tokens):
        resource = f"fixtures/{index}"
        made.append({"name": resource, "tokenName": name, "tokenValueType": "COLOR"})
        values.append({"name": f"{resource}/values/0", **value})
    return {"tokens": made, "values": values, "tokenSets": []}


def _table(tokens: list[tuple[str, dict]], prefixes: list[str]) -> str:
    system = _fixture(tokens)
    with tempfile.NamedTemporaryFile(suffix=".rs", delete=False) as handle:
        path = handle.name
    try:
        cmd_gen(system, prefixes, path)
        with open(path, encoding="utf-8") as handle:
            return handle.read()
    finally:
        os.unlink(path)


def cmd_selftest() -> int:
    """Parse small exports whose shape the real ones pin, without shipping them."""
    failures: list[str] = []

    def check(ok: bool, what: str) -> None:
        if not ok:
            failures.append(what)

    text = _table(
        [
            ("md.comp.menus.menu-item.selected.container.color", {"tokenName": "md.sys.color.primary"}),
            ("md.comp.menus.menu-item.height", {"length": {"value": 48}}),
        ],
        ["md.comp.menus"],
    )
    check(
        "pub struct MenusMenuItemContainer" in text,
        "an object between the states keeps its own struct",
    )
    check("selected: bool" in text, "and the wedged state becomes a parameter")

    text = _table(
        [
            ("md.comp.menus.menu-item.selected.label-text.color", {"tokenName": "md.sys.color.on-surface"}),
        ],
        ["md.comp.menus"],
    )
    check(
        "pub struct MenusMenuItemLabelText" in text,
        "an object split around a state still matches",
    )

    text = _table(
        [
            ("md.comp.list.label-text.color", {"tokenName": "md.sys.color.on-surface"}),
            ("md.comp.list.list-item.hover.label-text.color", {"tokenName": "md.sys.color.on-surface"}),
        ],
        ["md.comp.list"],
    )
    check("hovered: bool" in text, "a section name is stripped and its state kept")
    check("list_item" not in text, "the section leaves no field of its own")

    model = build(
        _fixture(
            [
                ("md.comp.menus.container.color", {"tokenName": "md.sys.color.surface"}),
                ("md.comp.menus.vibrant.menu-item.label-text.color", {"tokenName": "md.sys.color.primary"}),
            ]
        ),
        ["md.comp.menus"],
    )
    check(len(model["entries"]) == 1, "an excluded subtree is not a state of the control")
    check(len(model["skipped_unsupported"]) == 1, "and is counted as excluded")

    text = _table(
        [("md.comp.radio-button.unselected.icon.color", {"tokenName": "md.sys.color.primary"})],
        ["md.comp.radio-button"],
    )
    check("if  {" not in text, "a base state with no axis to negate emits no empty condition")
    check('    out.icon.color = "primary";' in text, "it applies as the base instead")

    text = _table(
        [
            ("md.comp.radio-button.unselected.icon.color", {"tokenName": "md.sys.color.on-surface-variant"}),
            ("md.comp.radio-button.selected.icon.color", {"tokenName": "md.sys.color.primary"}),
        ],
        ["md.comp.radio-button"],
    )
    # An unconditional default would overwrite the primary it is the opposite of.
    check("if !selected {" in text, "a default opposite a primary axis is guarded by it")

    text = _table(
        [
            ("md.comp.slider.container.color", {"tokenName": "md.sys.color.surface"}),
            ("md.comp.slider.large.handle.color", {"tokenName": "md.sys.color.primary"}),
        ],
        ["md.comp.slider"],
    )
    check("size: SliderSize" in text, "a variant axis becomes one enum parameter")
    check("SliderSize::Large" in text, "and each of its states an enum value")

    text = _table(
        [
            ("md.comp.radio-button.unselected.icon.color", {"tokenName": "md.sys.color.on-surface-variant"}),
            ("md.comp.radio-button.selected.icon.color", {"tokenName": "md.sys.color.primary"}),
            ("md.comp.radio-button.hover.icon.color", {"tokenName": "md.sys.color.primary"}),
        ],
        ["md.comp.radio-button"],
    )
    check(
        "pub const fn radio_button_spec(flags: RadioButtonFlags)" in text,
        "two switches travel as one named struct, not positional booleans",
    )
    check("pub const NONE: Self" in text, "and the struct offers a const all-off value")

    for what in failures:
        print(f"selftest: {what}", file=sys.stderr)
    if failures:
        return 1
    print("selftest: ok")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    for name in ("inspect", "gen"):
        p = sub.add_parser(name)
        p.add_argument("json")
        p.add_argument("--prefix", action="append", default=None)
        if name == "gen":
            p.add_argument("--out", default="-")
            p.add_argument("--name", default=None)
            p.add_argument(
                "--check",
                action="store_true",
                help="compare --out with the generated table instead of writing it",
            )
    sub.add_parser("selftest")
    audit = sub.add_parser("audit")
    audit.add_argument("--manifest", default="tokens/tables.json")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return cmd_selftest()
    if args.cmd == "audit":
        return cmd_audit(args.manifest)
    system = load(args.json)
    prefixes = args.prefix or [system["tokenSets"][0]["tokenSetName"]]
    if args.cmd == "inspect":
        return cmd_inspect(system, prefixes)
    return cmd_gen(system, prefixes, args.out, args.name, args.check)


if __name__ == "__main__":
    raise SystemExit(main())
