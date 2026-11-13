#!/usr/bin/env python3
"""Generate `winui_data.rs` from the pinned WinUI 3 XAML theme dictionaries.

Usage: gen_winui.py <xaml-dir> > ../src/winui/data.rs

The corpus is `microsoft/microsoft-ui-xaml` at the SHA below; only
`*_themeresources.xaml` files are read, since they are the authority for the
control brushes, thicknesses and gradients. Output is grouped by resource name,
each with one value per theme in [Light, Dark, HighContrast] order.
"""

import os
import re
import sys

SHA = "868fef9ac230e79ab54a90a79d2048c939299c70"
THEMES = ["Light", "Default", "HighContrast"]  # XAML -> output order below
OUT_ORDER = ["Light", "Default", "HighContrast"]
NAMED = {"transparent": 0x00000000, "white": 0xFFFFFFFF, "black": 0xFF000000}

ITEM = re.compile(
    r"<(?P<tag>Color|SolidColorBrush|StaticResource|Thickness|LinearGradientBrush|x:Boolean|x:String|x:Double)\b(?P<attrs>[^>]*?)(?:/>|>(?P<body>.*?)</(?P=tag)>)",
    re.S,
)
ATTR = re.compile(r'([A-Za-z:]+)="([^"]*)"')
REF = re.compile(r"\{(?:StaticResource|ThemeResource)\s+([A-Za-z0-9_]+)\}")


def attr(attrs, name):
    for k, v in ATTR.findall(attrs):
        if k == name:
            return v
    return None


def parse_color(text):
    t = text.strip()
    if t.startswith("#"):
        h = t[1:]
        if len(h) == 6:
            return int(h, 16) | 0xFF000000
        if len(h) == 8:
            return int(h, 16)
        if len(h) == 3:
            r, g, b = (int(c * 2, 16) for c in h)
            return 0xFF000000 | (r << 16) | (g << 8) | b
        raise ValueError(f"bad hex {text}")
    if t.lower() in NAMED:
        return NAMED[t.lower()]
    raise ValueError(f"unknown colour {text!r}")


def theme_blocks(text):
    """Yield (theme|None, body) for each theme dictionary, plus the shared tail."""
    body = text
    blocks = []
    start = text.find("<ResourceDictionary.ThemeDictionaries>")
    if start >= 0:
        end = text.find("</ResourceDictionary.ThemeDictionaries>", start)
        body = text[:start] + text[end + len("</ResourceDictionary.ThemeDictionaries>"):]
        inner = text[start:end]
        for m in re.finditer(
            r'<ResourceDictionary x:Key="(Default|Light|HighContrast)">(.*?)</ResourceDictionary>',
            inner,
            re.S,
        ):
            blocks.append((m.group(1), m.group(2)))
    blocks.append((None, body))
    return blocks


def items(block):
    for m in ITEM.finditer(block):
        tag = m.group("tag")
        attrs = m.group("attrs")
        key = attr(attrs, "x:Key")
        if not key:
            continue
        yield tag, key, attrs, m.group("body") or ""


def collect(path, out, conflicts):
    text = open(path, encoding="utf-8-sig").read()
    for theme, block in theme_blocks(text):
        for tag, key, attrs, body in items(block):
            defs = []
            if tag == "Color":
                defs = [("paint", ("#%08X" % parse_color(body), 1.0))]
            elif tag == "SolidColorBrush":
                c = attr(attrs, "Color")
                if c is None:
                    continue
                op = float(attr(attrs, "Opacity") or 1.0)
                ref = REF.search(c)
                if ref:
                    defs = [("paint", (ref.group(1), op))]
                else:
                    defs = [("paint", ("#%08X" % parse_color(c), op))]
            elif tag == "StaticResource":
                target = attr(attrs, "ResourceKey")
                if target:
                    defs = [("paint", (target, 1.0))]
            elif tag == "Thickness":
                vals = [float(v) for v in body.strip().split(",")]
                if len(vals) == 1:
                    vals *= 4
                elif len(vals) == 2:  # XAML: horizontal, vertical
                    vals = [vals[0], vals[1], vals[0], vals[1]]
                defs = [("thickness", tuple(vals))]  # may differ per theme; see output
            elif tag == "LinearGradientBrush":
                stops = []
                for sm in re.finditer(
                    r'<GradientStop\s+Offset="([0-9.]+)"\s+Color="([^"]+)"', body
                ):
                    ref = REF.search(sm.group(2))
                    stops.append((float(sm.group(1)), ref.group(1) if ref else sm.group(2)))
                st = attr(attrs, "StartPoint") or "0,0"
                en = attr(attrs, "EndPoint") or "0,1"
                rel = attr(attrs, "MappingMode") == "RelativeToBoundingBox"
                # Dark mode mirrors the elevation brushes vertically, so the highlight sits
                # on the bottom edge; the transform is the only place that is recorded.
                flip = bool(re.search(r'<ScaleTransform[^>]*ScaleY="-1"', body))
                defs = [("gradient", (tuple(float(v) for v in st.split(",")),
                                      tuple(float(v) for v in en.split(",")),
                                      rel, flip, tuple(stops)))]
            for kind, value in defs:
                entry = out.setdefault(key, {})
                prev = entry.get(theme)
                if prev is not None and prev != (kind, value):
                    conflicts.append((key, theme, prev, (kind, value)))
                entry[theme] = (kind, value)


def flt(x):
    """A Rust f32 literal that always carries a decimal point."""
    s = "%g" % x
    return s if ("." in s or "e" in s.lower()) else s + ".0"


def resolve(name, theme, out, depth=0):
    """Follow paint aliases to a literal for equivalence checks only."""
    if depth > 8:
        return None
    per = out.get(name)
    if not per:
        return None
    v = per.get(theme) or per.get("Default") or per.get(None)
    if not v:
        return None
    kind, value = v
    if kind != "paint":
        return ("opaque", value)
    target, op = value
    if target.startswith("#"):
        return ("paint", target, op)
    inner = resolve(target, theme, out, depth + 1)
    if inner and inner[0] == "paint":
        return ("paint", inner[1], op * inner[2])
    return None


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    d = sys.argv[1]
    # `Common_themeresources_any.xaml` holds the theme palettes but does not end in
    # `_themeresources.xaml`; match on the stem instead.
    files = sorted(
        f for f in os.listdir(d)
        if "themeresources" in f.lower() and f.endswith(".xaml") and "perf2026" not in f
    )
    out, conflicts = {}, []
    for f in files:
        collect(os.path.join(d, f), out, conflicts)

    rows = []
    unresolved = []
    for name in sorted(out):
        per = out[name]
        kinds = {k for k, _ in per.values()}
        if kinds == {"gradient", "paint"}:
            # A gradient in two themes and a flat colour in the third: a one-stop gradient
            # is the flat colour, so the entry keeps one shape.
            geom = next(v[:3] for k, v in per.values() if k == "gradient")
            for theme, (kind, value) in list(per.items()):
                if kind == "paint":
                    target, op = value
                    if op != 1.0 and target.startswith("#"):
                        argb = int(target[1:], 16)
                        a = round(((argb >> 24) & 0xFF) * op)
                        target = "#%08X" % ((argb & 0xFFFFFF) | (a << 24))
                    per[theme] = ("gradient", (*geom, False, ((0.0, target),)))
            kinds = {"gradient"}
        if len(kinds) != 1:
            unresolved.append((name, "mixed kinds " + repr(kinds)))
            continue
        kind = kinds.pop()
        values = []
        for theme in OUT_ORDER:
            # XAML falls back to the `Default` dictionary for any theme, so a key that
            # only the dark block defines still resolves in light and high contrast.
            if theme in per:
                values.append(per[theme][1])
            elif "Default" in per:
                values.append(per["Default"][1])
            elif None in per:
                values.append(per[None][1])
            else:
                values.append(None)
        if any(v is None for v in values):
            # A key the XAML defines for only some themes: use the value the others agree on.
            present = [v for v in values if v is not None]
            def canon(v):
                # Fully transparent is one colour whatever its RGB bytes say.
                if v and v[0] == "paint" and (int(v[1][1:3], 16) == 0):
                    return ("paint", "#00000000", 1.0)
                return v

            resolved = {t: canon(resolve(name, t, out)) for t in OUT_ORDER}
            same_paint = all(
                resolved[t] == resolved[OUT_ORDER[0]] for t in OUT_ORDER
                if resolved[t] is not None
            ) and resolved[OUT_ORDER[0]] is not None
            if same_paint:
                values = [v if v is not None else present[0] for v in values]
            elif present and all(v == present[0] for v in present):
                values = [v if v is not None else present[0] for v in values]
            else:
                unresolved.append((name, "missing theme"))
                continue
        rows.append((kind, name, values))

    w = sys.stdout.write
    w("// Generated from microsoft/microsoft-ui-xaml@%s by data/gen_winui.py.\n" % SHA)
    w("//\n")
    w("// Columns are [Light, Dark, HighContrast]; values are the theme dictionaries'\n")
    w("// own, with `Default` standing for Dark.\n\n")
    w("pub const THEME_COUNT: usize = 3;\n\n")
    w("pub enum Def {\n")
    w("    /// A colour per theme: either `#AARRGGBB` or another resource's name.\n")
    w("    Paint([&'static str; THEME_COUNT], [f32; THEME_COUNT]),\n")
    w("    /// A thickness per theme; a few WinUI borders are 0 in one theme and 1 in another.\n")
    w("    Thickness([[f32; 4]; THEME_COUNT]),\n")
    w("    Gradient {\n")
    w("        start: (f32, f32),\n")
    w("        end: (f32, f32),\n")
    w("        relative: bool,\n")
    w("        /// Mirrored across the box's mid-line, as dark mode's elevation brushes are.\n")
    w("        flip: [bool; THEME_COUNT],\n")
    w("        stops: [&'static [(f32, &'static str)]; THEME_COUNT],\n")
    w("    },\n")
    w("}\n\n")
    w("pub struct Entry {\n    pub name: &'static str,\n    pub def: Def,\n}\n\n")
    w("pub const RESOURCES: &[Entry] = &[\n")
    for kind, name, values in sorted(rows, key=lambda r: r[1]):
        if kind == "paint":
            targets = [t for t, _ in values]
            # High contrast uses `#FF0000` / `#FF00FF` as "the system replaces this" canaries;
            # carrying them as colours would paint a WinUI theme in pure red.
            for i in (2,):
                if targets[i].upper() in ("#FFFF0000", "#FFFF00FF"):
                    targets[i] = "@system"
            ops = [o for _, o in values]
            w('    Entry { name: "%s", def: Def::Paint([%s], [%s]) },\n' % (
                name,
                ", ".join('"%s"' % t for t in targets),
                ", ".join(("%g" % o) if o != 1.0 else "1.0" for o in ops),
            ))
        elif kind == "thickness":
            w('    Entry { name: "%s", def: Def::Thickness([%s]) },\n' % (
                name,
                ", ".join("[%s]" % ", ".join(flt(x) for x in v) for v in values),
            ))
        elif kind == "gradient":
            (st, en, rel, _flip, stops0) = values[0]
            for st2, en2, rel2, _, _ in values:
                if (st2, en2, rel2) != (st, en, rel):
                    unresolved.append((name, "gradient geometry differs by theme"))
            slices = []
            for _, _, _, _, stops in values:
                lit = []
                for off, col in stops:
                    if col.startswith("#") or col.lower() in NAMED:
                        c = parse_color(col)
                        lit.append('(%s, "#%08X")' % (flt(off), c))
                    else:
                        lit.append('(%s, "%s")' % (flt(off), col))
                slices.append("&[%s]" % ", ".join(lit))
            w('    Entry { name: "%s", def: Def::Gradient { start: (%s, %s), end: (%s, %s), relative: %s, flip: [%s], stops: [%s] } },\n' % (
                name, flt(st[0]), flt(st[1]), flt(en[0]), flt(en[1]),
                "true" if rel else "false", ", ".join("true" if v[3] else "false" for v in values),
                ", ".join(slices)))
    w("];\n")

    if unresolved:
        sys.stderr.write("unresolved:\n")
        for name, why in unresolved:
            sys.stderr.write("  %s: %s\n" % (name, why))
    if conflicts:
        sys.stderr.write("conflicts (%d):\n" % len(conflicts))
        for key, theme, prev, new in conflicts[:40]:
            sys.stderr.write("  %s [%s]: %r vs %r\n" % (key, theme, prev, new))


if __name__ == "__main__":
    main()
