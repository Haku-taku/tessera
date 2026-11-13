"""Generate fluent_data.rs from docs/reference/fluent-tokens.json (one-shot, data only)."""
import json, re, pathlib, sys

ROOT = pathlib.Path(__file__).resolve().parents[4]
d = json.load(open(ROOT / 'docs/reference/fluent-tokens.json'))
light, dark, hc = d['themes']['webLight'], d['themes']['webDark'], d['themes']['teamsHighContrast']
assert set(light) == set(dark) == set(hc), 'the three themes must share one name set'

HEX6 = re.compile(r'#([0-9a-fA-F]{6})')
HEX3 = re.compile(r'#([0-9a-fA-F]{3})')
RGBA_ALLOW_SPACE = re.compile(
    r'rgba?\(\s*([0-9.]+)\s*,\s*([0-9.]+)\s*,\s*([0-9.]+)\s*(?:,\s*([0-9.]+)\s*)?\)')


def rgba_of(v):
    if not isinstance(v, str):
        return None
    if v == 'transparent':
        return 0x00000000
    m = HEX6.fullmatch(v)
    if m:
        return int('ff' + m.group(1), 16)
    m = HEX3.fullmatch(v)
    if m:
        h = m.group(1)
        return int('ff' + h[0] * 2 + h[1] * 2 + h[2] * 2, 16)
    m = re.fullmatch(r'#([0-9a-fA-F]{8})', v)
    if m:
        # Fluent writes alpha LAST in #rrggbbaa; our ARGB wants it first.
        h = m.group(1)
        return int(h[6:8] + h[0:6], 16)
    return None


def only_alpha_rgba(v):
    """Parse rgba()/rgb()/transparent into ARGB, tolerating spaces and decimals."""
    if not isinstance(v, str):
        return None
    text = v.strip()
    if text == 'transparent':
        return 0x00000000
    if not text.startswith(('rgba(', 'rgb(')):
        return None
    nums = text[text.index('(') + 1:text.rindex(')')].split(',')
    try:
        r, g, b = (round(float(nums[i])) for i in range(3))
        a = float(nums[3]) if len(nums) > 3 else 1.0
    except (ValueError, IndexError):
        return None
    return (round(a * 255) << 24) | (r << 16) | (g << 8) | b


value_colors, value_alpha, value_numbers, value_lengths = [], [], [], []
for name in sorted(light):
    vals = [light[name], dark[name], hc[name]]
    plain = [rgba_of(v) for v in vals]
    if all(p is not None for p in plain):
        if any(p >> 24 != 0xff for p in plain):
            value_alpha.append((name, plain))
        else:
            value_colors.append((name, plain))
        continue
    # Mixed per theme (Fluent high contrast swaps rgba overlays for solid hex), so parse both ways.
    mixed = [p if p is not None else only_alpha_rgba(v)
             for p, v in zip(plain, vals)]
    if all(p is not None for p in mixed):
        if any(p >> 24 != 0xff for p in mixed):
            value_alpha.append((name, mixed))
        else:
            value_colors.append((name, mixed))
        continue
    if all(isinstance(v, int) for v in vals):
        value_numbers.append((name, vals))
        continue
    if all(isinstance(v, str) and re.fullmatch(r'-?[0-9.]+(px|ms)?', v) for v in vals):
        nums = [float(re.sub(r'(px|ms)$', '', v)) for v in vals]
        if name.startswith('color') and all(v in ('0', '0px') for v in vals):
            value_alpha.append((name, [0x00000000] * 3))
        else:
            value_lengths.append((name, nums))
        continue

non_values = sorted(set(light) - {n for n, _ in value_colors} - {n for n, _ in value_alpha}
                    - {n for n, _ in value_numbers} - {n for n, _ in value_lengths})

# Every colour alias except the state/shadow ones must equal a global palette entry.
palettes = sorted(d['globalColors'])
palette_hexes = set()
for pal in d['globalColors'].values():
    if isinstance(pal, dict):
        for v in pal.values():
            if isinstance(v, str):
                h = rgba_of(v)
                if h is not None:
                    palette_hexes.add(h)

out = []
w = out.append
w('// Generated from docs/reference/fluent-tokens.json. Do not edit by hand.')
w('//')
w('// Theme columns are [light, dark, high-contrast], aligned with `Themes` in the system module.')
w('')
w('pub const THEME_COUNT: usize = 3;')
w('')
def emit_table(name, doc_en, rows):
    w('/// ' + doc_en)
    w('pub const {}: &[(&str, [u32; THEME_COUNT])] = &['.format(name))
    for n, vals in rows:
        hexes = ', '.join('0x{0:08x}'.format(int(v)) for v in vals)
        w('    ("{}", [{}]),'.format(n, hexes))
    w('];')
    w('')

emit_table('COLORS', 'Opaque colour tokens: (name, [light, dark, high-contrast]).',
           value_colors)
emit_table('ALPHA_COLORS', 'Colour tokens that carry alpha (overlays, stencils): same shape as COLORS.',
           value_alpha)
w('/// Colour tokens whose value is not a flat colour (shadows, gradients).')
w('pub const COLOR_TOKENS_WITHOUT_VALUE: &[&str] = &[')
for name in non_values:
    if name.startswith('color') or name.startswith('shadow'):
        w('    "{}",'.format(name))
w('];')
w('')
w('pub const NUMBERS: &[(&str, [i32; THEME_COUNT])] = &[')
for name, vals in value_numbers:
    w('    ("{}", [{}]),'.format(name, ', '.join(map(str, vals))))
w('];')
w('')
w('pub const LENGTHS: &[(&str, [f32; THEME_COUNT])] = &[')
for name, vals in value_lengths:
    w('    ("{}", [{}]),'.format(name, ', '.join('{}f32'.format(v) for v in vals)))
w('];')
w('')
w('/// Non-numeric token values (font stacks, cubic-beziers, shadows) kept verbatim.')
w('pub const TEXTS: &[(&str, &str, &str, &str)] = &[')
for name in non_values:
    if name.startswith(('color', 'shadow')):
        continue
    w('    ("{}", "{}", "{}", "{}"),'.format(
        name, light[name].replace('\\', '\\\\').replace('"', '\\"'),
        dark[name].replace('\\', '\\\\').replace('"', '\\"'),
        hc[name].replace('\\', '\\\\').replace('"', '\\"')))
w('];')
w('')
w('/// The default web brand ramp (tone 10..160) the alias layer derives from.')
w('pub const BRAND_RAMP: &[(u16, u32)] = &[')
for tone in sorted(d['brandRamp'], key=int):
    w('    ({}, 0xff{:06x}),'.format(tone, int(d['brandRamp'][tone][1:], 16)))
w('];')

path = pathlib.Path(__file__).with_name('fluent_data.rs')
path.write_text('\n'.join(out) + '\n')
print('colors', len(value_colors), 'alpha', len(value_alpha), 'numbers', len(value_numbers), 'lengths', len(value_lengths),
      'texts', sum(1 for n in non_values if not n.startswith(('color', 'shadow'))),
      'non-values', len(non_values))
print('palette hexes', len(palette_hexes))
