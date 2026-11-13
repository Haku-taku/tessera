"""Generate dimensions_data.rs (MD3 + Fluent + HIG scales) from the reference JSON files."""
import json, pathlib

ROOT = pathlib.Path(__file__).resolve().parents[4]
md3 = json.loads((ROOT / 'docs/reference/md3-scales.json').read_text())
fluent = json.loads((ROOT / 'docs/reference/fluent-tokens.json').read_text())
hig = json.loads((ROOT / 'docs/reference/hig-tokens.json').read_text())

out = []
w = out.append
w('// Generated from docs/reference/{md3-scales,fluent-tokens,hig-tokens}.json.')
w('')
w('use crate::dimension::{Curvature::*, DurationStep, HitTarget, MotionScale, RadiusScale,')
w('    RadiusStep, SpacingScale, SpacingStep, TypeRamp, TypeStep};')
w('use crate::source::Source;')
w('')


def f(v):
    # f32 literal; keep -1 sentinel exact.
    if isinstance(v, int):
        return '%df32' % v
    return repr(float(v)) + 'f32'


def radius_scale(name, steps, curvature, cut, source):
    w('pub static %s_RADIUS: &[RadiusStep] = &[' % name)
    for s in steps:
        w('    RadiusStep { name: "%s", points: %s },' % (s['name'], f(s['points'])))
    w('];')
    w('pub static %s_RADIUS_SCALE: RadiusScale = RadiusScale {' % name)
    w('    steps: %s_RADIUS,' % name)
    w('    curvature: %s,' % curvature)
    w('    supports_cut: %s,' % ('true' if cut else 'false'))
    w('    source: Source::%s,' % source)
    w('};')
    w('')


# ---- MD3 radius: the documented ten steps; -1 marks Full ----
md3_corners = [{'name': c['name'], 'points': -1.0 if c['dp'] < 0 else float(c['dp'])}
               for c in md3['shape']['cornerScale']]
radius_scale('MD3', md3_corners, 'Round', True,
             'Spec("m3.material.io/styles/shape/corner-radius-scale")')

w('// MD3 spacing: 8dp base, the documented multiples plus 2/4/6/10 nested values.')
w('pub static MD3_SPACING: &[SpacingStep] = &[')
for s in md3['spacing']['tokens']:
    w('    SpacingStep { name: "%s", multiple: %s },' % (s['name'], f(s['multiple'])))
w('];')
w('pub static MD3_NESTED: &[f32] = &[' + ', '.join(f(v) for v in md3['spacing']['nested']) + '];')
w('pub static MD3_SPACING_SCALE: SpacingScale = SpacingScale {')
w('    base_points: %s,' % f(md3['spacing']['baseDp']))
w('    steps: MD3_SPACING,')
w('    nested: MD3_NESTED,')
w('    source: Source::Spec("m3.material.io/styles/spacing/overview"),')
w('};')
w('')

w('pub static MD3_TYPE: &[TypeStep] = &[')
for s in md3['typography']['styles']:
    w('    TypeStep { name: "%s", size: %s, line_height: %s, weight: %d },'
      % (s['name'], f(s['size']), f(s['lineHeight']), s['weight']))
w('];')
w('pub static MD3_TYPE_RAMP: TypeRamp = TypeRamp {')
w('    steps: MD3_TYPE,')
w('    scalable: false,')
w('    source: Source::Spec("M3 2021 type scale (typography.dart)"),')
w('};')
w('')

w('pub static MD3_DURATIONS: &[DurationStep] = &[')
for s in md3['motion']['durations']:
    w('    DurationStep { name: "%s", ms: %s },' % (s['name'], f(s['ms'])))
w('];')
w('pub static MD3_CURVES: &[(&str, [f32; 4])] = &[')
for s in md3['motion']['curves']:
    w('    ("%s", [%s]),' % (s['name'], ', '.join(repr(float(c)) + 'f32' for c in s['cubic'])))
w('];')
w('pub static MD3_MOTION: MotionScale = MotionScale {')
w('    durations: MD3_DURATIONS,')
w('    curves: MD3_CURVES,')
w('    source: Source::Spec("Flutter Curves (md3 docs motion tables unavailable)"),')
w('};')
w('')
w('pub static MD3_HIT: HitTarget = HitTarget {')
w('    default_points: %s,' % f(md3['hitTarget']['default']))
w('    minimum_points: %s,' % f(md3['hitTarget']['min']))
w('    source: Source::Spec("M3 touch target 48dp"),')
w('};')
w('')

# ---- Fluent scales: lengths come as strings in the token file; reuse the dumped tables ----
import re
ft = fluent['scales']
def num(v):
    m = re.fullmatch(r'-?[0-9.]+', v)
    return float(v) if m else None

# Radii: 11 tokens; Circular is the capsule sentinel.
w('// Fluent radii: 0..40 plus `circular` (capsule sentinel -1).')
w('pub static FLUENT_RADIUS: &[RadiusStep] = &[')
order = ['borderRadiusNone', 'borderRadiusSmall', 'borderRadiusMedium', 'borderRadiusLarge',
         'borderRadiusXLarge', 'borderRadius2XLarge', 'borderRadius3XLarge', 'borderRadius4XLarge',
         'borderRadius5XLarge', 'borderRadius6XLarge', 'borderRadiusCircular']
short = {'borderRadiusNone': 'none', 'borderRadiusSmall': 'small', 'borderRadiusMedium': 'medium',
         'borderRadiusLarge': 'large', 'borderRadiusXLarge': 'xLarge', 'borderRadius2XLarge': '2XLarge',
         'borderRadius3XLarge': '3XLarge', 'borderRadius4XLarge': '4XLarge',
         'borderRadius5XLarge': '5XLarge', 'borderRadius6XLarge': '6XLarge',
         'borderRadiusCircular': 'circular'}
for k in order:
    v = ft['borderRadius'][k]
    pts = -1.0 if k == 'borderRadiusCircular' else float(v.replace('px', ''))
    w('    RadiusStep { name: "%s", points: %s },' % (short[k], f(pts)))
w('];')
w('pub static FLUENT_RADIUS_SCALE: RadiusScale = RadiusScale {')
w('    steps: FLUENT_RADIUS,')
w('    curvature: Round,')
w('    supports_cut: false,')
w('    source: Source::Spec("@fluentui/tokens borderRadius"),')
w('};')
w('')

# Spacing: 11 horizontal steps; base 4 (Fluent\'s smallest is 2, so base=4 with halves is wrong) — use base 1.
w('// Fluent spacing has no single base; store literal points with base 1.')
hsteps = ['spacingHorizontalNone', 'spacingHorizontalXXS', 'spacingHorizontalXS',
          'spacingHorizontalSNudge', 'spacingHorizontalS', 'spacingHorizontalMNudge',
          'spacingHorizontalM', 'spacingHorizontalL', 'spacingHorizontalXL',
          'spacingHorizontalXXL', 'spacingHorizontalXXXL']
hshort = {'spacingHorizontalNone': 'none', 'spacingHorizontalXXS': 'xxs', 'spacingHorizontalXS': 'xs',
          'spacingHorizontalSNudge': 'sNudge', 'spacingHorizontalS': 's',
          'spacingHorizontalMNudge': 'mNudge', 'spacingHorizontalM': 'm',
          'spacingHorizontalL': 'l', 'spacingHorizontalXL': 'xl',
          'spacingHorizontalXXL': 'xxl', 'spacingHorizontalXXXL': 'xxxl'}
w('pub static FLUENT_SPACING: &[SpacingStep] = &[')
for k in hsteps:
    pts = float(ft['spacings'][k].replace('px', ''))
    w('    SpacingStep { name: "%s", multiple: %s },' % (hshort[k], f(pts)))
w('];')
w('pub static FLUENT_SPACING_SCALE: SpacingScale = SpacingScale {')
w('    base_points: 1.0f32,')
w('    steps: FLUENT_SPACING,')
w('    nested: &[],')
w('    source: Source::Spec("@fluentui/tokens spacings"),')
w('};')
w('')

# Typography: Fluent publishes fontSize primitives plus composite styles; use the composites.
w('// Fluent type: Base400 (16) body, Base500 (20) subtitle, Base600 (24) title, Base300 (14) caption.')
w('pub static FLUENT_TYPE: &[TypeStep] = &[')
w('    TypeStep { name: "title", size: 24f32, line_height: 32f32, weight: 600 },')
w('    TypeStep { name: "subtitle", size: 20f32, line_height: 28f32, weight: 600 },')
w('    TypeStep { name: "body", size: 16f32, line_height: 22f32, weight: 400 },')
w('    TypeStep { name: "caption", size: 14f32, line_height: 20f32, weight: 400 },')
w('    TypeStep { name: "hint", size: 12f32, line_height: 16f32, weight: 400 },')
w('];')
w('pub static FLUENT_TYPE_RAMP: TypeRamp = TypeRamp {')
w('    steps: FLUENT_TYPE,')
w('    scalable: false,')
w('    source: Source::Spec("@fluentui/tokens fontSizeBase* + typographyStyles"),')
w('};')
w('')

w('pub static FLUENT_DURATIONS: &[DurationStep] = &[')
for k, v in ft['durations'].items():
    if isinstance(v, str):
        w('    DurationStep { name: "%s", ms: %s },' % (k.replace('duration', '').lower(), f(float(v.replace('ms', '')))))
w('];')
w('pub static FLUENT_CURVES: &[(&str, [f32; 4])] = &[')
for k, v in ft['curves'].items():
    if isinstance(v, str) and 'cubic-bezier' in v:
        nums = [float(x) for x in v[v.index('(') + 1:v.index(')')].split(',')]
        w('    ("%s", [%s]),' % (k.replace('curve', '').lower(), ', '.join(repr(n) + 'f32' for n in nums)))
w('];')
w('pub static FLUENT_MOTION: MotionScale = MotionScale {')
w('    durations: FLUENT_DURATIONS,')
w('    curves: FLUENT_CURVES,')
w('    source: Source::Spec("@fluentui/tokens durations + curves"),')
w('};')
w('')

w('// Fluent hit target: 32dp comfortable, 24dp floor (Fluent 2 control sizing).')
w('pub static FLUENT_HIT: HitTarget = HitTarget {')
w('    default_points: 32f32,')
w('    minimum_points: 24f32,')
w('    source: Source::Spec("@fluentui/tokens control sizing"),')
w('};')
w('')

# ---- HIG: radii and spacing are convention (Apple publishes none); type + hit target are spec ----
w('// HIG radii: Apple publishes no scale; these are our convention (continuous only).')
w('pub static HIG_RADIUS: &[RadiusStep] = &[')
for name, pts in [('small', 8.0), ('medium', 12.0), ('card', 16.0), ('large', 20.0), ('capsule', -1.0)]:
    w('    RadiusStep { name: "%s", points: %s },' % (name, f(pts)))
w('];')
w('pub static HIG_RADIUS_SCALE: RadiusScale = RadiusScale {')
w('    steps: HIG_RADIUS,')
w('    curvature: Continuous,')
w('    supports_cut: false,')
w('    source: Source::Convention("our 8pt convention; Apple publishes no radius scale"),')
w('};')
w('')

w('// HIG spacing: 8pt grid; Apple publishes no numeric scale, so these are convention.')
w('pub static HIG_SPACING: &[SpacingStep] = &[')
for i, name in enumerate(['xxs', 'xs', 's', 'm', 'l', 'xl', 'xxl', 'xxxl']):
    w('    SpacingStep { name: "%s", multiple: %s },' % (name, f(float([0.5, 1, 1.5, 2, 2.5, 3, 4, 5][i]))))
w('];')
w('pub static HIG_SPACING_SCALE: SpacingScale = SpacingScale {')
w('    base_points: 8f32,')
w('    steps: HIG_SPACING,')
w('    nested: &[4f32],')
w('    source: Source::Convention("8pt grid; Apple publishes no scale"),')
w('};')
w('')

w('// HIG type: the iOS ramp (default Dynamic Type size); scalable, unlike the other two.')
w('pub static HIG_TYPE: &[TypeStep] = &[')
ios = highest = hig['typography']['iosTextStyles']
pairs = [('title', 'largeTitle'), ('subtitle', 'title2'), ('body', 'body'),
         ('caption', 'footnote'), ('hint', 'caption1')]
for key, src in pairs:
    s = ios[src]
    w('    TypeStep { name: "%s", size: %s, line_height: %s, weight: %d },'
      % (key, f(s['size']), f(s['lineHeight']),
         400 if s['weight'] == 'regular' else (600 if s['weight'] == 'semibold' else 700)))
w('];')
w('pub static HIG_TYPE_RAMP: TypeRamp = TypeRamp {')
w('    steps: HIG_TYPE,')
w('    scalable: true,')
w('    source: Source::Spec("Apple HIG text styles (default Dynamic Type size)"),')
w('};')
w('')

w('// HIG motion: the HIG publishes no numbers, so these are our convention.')
w('pub static HIG_DURATIONS: &[DurationStep] = &[')
for name, ms in [('quick', 200.0), ('standard', 300.0), ('slow', 500.0)]:
    w('    DurationStep { name: "%s", ms: %s },' % (name, f(ms)))
w('];')
w('pub static HIG_CURVES: &[(&str, [f32; 4])] = &[')
for name, c in [('standard', [0.25, 0.1, 0.25, 1.0]), ('decelerate', [0.0, 0.0, 0.2, 1.0]),
                ('accelerate', [0.4, 0.0, 1.0, 1.0])]:
    w('    ("%s", [%s]),' % (name, ', '.join(repr(x) + 'f32' for x in c)))
w('];')
w('pub static HIG_MOTION: MotionScale = MotionScale {')
w('    durations: HIG_DURATIONS,')
w('    curves: HIG_CURVES,')
w('    source: Source::Convention("HIG publishes no motion numbers"),')
w('};')
w('')
w('// HIG hit target: the one dimension Apple does publish (iOS 44 default, 28 hard floor).')
w('pub static HIG_HIT: HitTarget = HitTarget {')
w('    default_points: 44f32,')
w('    minimum_points: 28f32,')
w('    source: Source::Spec("Apple HIG hit targets (iOS)"),')
w('};')

path = pathlib.Path(__file__).with_name('dimensions_data.rs')
path.write_text('\n'.join(out) + '\n')
print('generated', path, '| lines:', len(out))
