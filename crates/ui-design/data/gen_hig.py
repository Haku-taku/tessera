"""Generate hig_data.rs from docs/reference/hig-tokens.json (data only)."""
import json, pathlib, re

ROOT = pathlib.Path(__file__).resolve().parents[4]
d = json.loads((ROOT / 'docs/reference/hig-tokens.json').read_text())
sc = d['color']['systemColors']
gr = d['color']['grayRamp']

# Column order matches `hig::theme_column`: [light, dark, light-IC, dark-IC].
COLS = ['light', 'dark', 'light_increased_contrast', 'dark_increased_contrast']


def hexargb(v):
    v = v.lstrip('#')
    if len(v) == 6:
        return int('ff' + v, 16)
    if len(v) == 8:
        return int(v, 16)
    raise SystemExit('unexpected hex ' + v)


out = []
w = out.append
w('// Generated from docs/reference/hig-tokens.json. Do not edit by hand.')
w('')
w('pub const CONTEXT_COUNT: usize = 4;')
w('')
w('/// System colours: (name, [light, dark, light-increased-contrast, dark-increased-contrast]).')
w('pub const SYSTEM_COLORS: &[(&str, [u32; CONTEXT_COUNT])] = &[')
for name in sorted(sc):
    vals = [hexargb(sc[name][c]) for c in COLS]
    w('    ("{}", [{}]),'.format(name, ', '.join('0x{:08x}'.format(v) for v in vals)))
w('];')
w('')
w('/// The iOS grey ramp beyond `systemGray`.')
w('pub const GRAY_RAMP: &[(&str, [u32; CONTEXT_COUNT])] = &[')
for name in sorted(gr):
    vals = [hexargb(gr[name][c]) for c in COLS]
    w('    ("{}", [{}]),'.format(name, ', '.join('0x{:08x}'.format(v) for v in vals)))
w('];')
w('')
w('/// Disabled-state colours; the values are published, not derived.')
w('pub const DISABLED_COLORS: &[(&str, [u32; CONTEXT_COUNT])] = &[')
for name, spec in d['color']['disabledColors'].items():
    vals = [hexargb(spec[c]) for c in COLS]
    w('    ("{}", [{}]),'.format(name, ', '.join('0x{:08x}'.format(v) for v in vals)))
w('];')
w('')
w('/// System fill colours, translucent by design.')
w('pub const FILL_COLORS: &[(&str, [u32; CONTEXT_COUNT])] = &[')
for name, spec in d['color']['systemFills'].items():
    vals = [hexargb(spec[c]) for c in COLS]
    w('    ("{}", [{}]),'.format(name, ', '.join('0x{:08x}'.format(v) for v in vals)))
w('];')
w('')
w('/// Hit-target minimums in points, per platform: (platform, default, minimum).')
w('pub const HIT_TARGETS: &[(&str, f32, f32)] = &[')
for plat, spec in sorted(d['layout']['hitTarget'].items()):
    # Only the per-platform entries have default/minimum; the rest are notes.
    if not isinstance(spec, dict) or 'default' not in spec:
        continue
    default = float(spec['default'].split('x')[0])
    minimum = float(spec['minimum'].split('x')[0])
    w('    ("{}", {}f32, {}f32),'.format(plat, default, minimum))
w('];')
w('')
w('/// Content materials, thinnest to thickest.')
w('pub const MATERIALS: &[&str] = &[')
for m in d['materials']['systemMaterials']:
    w('    "{}",'.format(m.lstrip('.')))
w('];')

w('')
w('/// Standard content materials as our derivation: (name, blur sigma in points, tint argb,')
w('/// fallback argb), each tint/fallback per context column. Apple publishes no numbers.')
recipes = d['materials']['materialRecipes']
tint_light = recipes['tint']['light']
tint_dark = recipes['tint']['dark']
fallback = [recipes['fallback'][c] for c in COLS]
w('pub const MATERIAL_RECIPES: &[(&str, f32, [u32; CONTEXT_COUNT], [u32; CONTEXT_COUNT])] = &[')
for name, spec in recipes['materials'].items():
    tints = []
    for i, alpha in enumerate(spec['alpha']):
        base = tint_light if i % 2 == 0 else tint_dark
        rgb = hexargb(base) & 0xffffff
        a = int(round(alpha * 255))
        tints.append((a << 24) | rgb)
    w('    ("{}", {}f32, [{}], [{}]),'.format(
        name.lstrip('.'),
        float(spec['sigma']),
        ', '.join('0x{:08x}'.format(v) for v in tints),
        ', '.join('0x{:08x}'.format(hexargb(v)) for v in fallback)))
w('];')
w('')
w('/// Apple semantic colour role names; the values are resolved by the OS.')
w('pub const SEMANTIC_ROLES: &[&str] = &[')
for name in d['color']['semanticRoles']:
    w('    "{}",'.format(name))
w('];')

path = pathlib.Path(__file__).with_name('hig_data.rs')
path.write_text('\n'.join(out) + '\n')
print('system colours:', len(sc), '| grey ramp:', len(gr),
      '| hit targets:', len(d['layout']['hitTarget']), '| materials:', len(out))
