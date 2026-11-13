"""Generate md3_data.rs from docs/reference/md3-color-roles.json (data only, no logic)."""
import json, pathlib

ROOT = pathlib.Path(__file__).resolve().parents[4]
doc = json.loads((ROOT / 'docs/reference/md3-color-roles.json').read_text())
roles = doc['roles']
assert len(roles) == 49, len(roles)

PALETTES = {'primary': 'Primary', 'secondary': 'Secondary', 'tertiary': 'Tertiary',
            'neutral': 'Neutral', 'neutral_variant': 'NeutralVariant', 'error': 'Error'}
BG = {'highestSurface': 'HighestSurface'}
ROLE_TO_BG = {
    'highestSurface': 'HighestSurface',
    'Background': 'Background',
    'Primary': 'Primary',
    'Secondary': 'Secondary',
    'Tertiary': 'Tertiary',
    'Error': 'Error',
    'PrimaryContainer': 'PrimaryContainer',
    'SecondaryContainer': 'SecondaryContainer',
    'TertiaryContainer': 'TertiaryContainer',
    'ErrorContainer': 'ErrorContainer',
    'InverseSurface': 'InverseSurface',
    'PrimaryFixed': 'PrimaryFixed',
    'SecondaryFixed': 'SecondaryFixed',
    'TertiaryFixed': 'TertiaryFixed',
    'PrimaryFixedDim': 'PrimaryFixedDim',
    'SecondaryFixedDim': 'SecondaryFixedDim',
    'TertiaryFixedDim': 'TertiaryFixedDim',
}

out = []
w = out.append
w('// Generated from docs/reference/md3-color-roles.json. Do not edit by hand.')
w('')
w('use super::tone::{Curve, DeltaPair, Polarity};')
w('use super::{Background, Role, ToneRule};')
w('use crate::md3::palette::PaletteId;')
w('')

# Per-role tone rule table, written by hand-shape from the decoded JSON.
w('/// Every role, in the order the source declares them.')
w('pub const ROLES: &[Role] = &[')


def f(v):
    # Always a float literal so Rust accepts it in f64 positions.
    return repr(float(v))


def curve(vals):
    return 'Curve([{}])'.format(', '.join(f(v) for v in vals))


def tone_of(t):
    k = t['kind']
    if k == 'pair':
        return 'Pair {{ light: {}, dark: {} }}'.format(f(t['light']), f(t['dark']))
    if k == 'const':
        return 'Const({})'.format(f(t['value']))
    if k == 'light_curve':
        return 'LightCurve {{ dark: {}, curve: {} }}'.format(f(t['dark']), curve(t['curve']))
    if k == 'dark_curve':
        return 'DarkCurve {{ light: {}, curve: {} }}'.format(f(t['light']), curve(t['curve']))
    if k == 'both_curve':
        return 'BothCurve {{ light: {}, dark: {} }}'.format(curve(t['light']), curve(t['dark']))
    if k == 'fidelity_default':
        return 'Pair {{ light: {}, dark: {} }}'.format(f(t['light']), f(t['dark']))
    if k == 'mono_pair':
        return 'Pair {{ light: {}, dark: {} }}'.format(f(t['light']), f(t['dark']))
    raise SystemExit('unhandled tone kind ' + k)


for r in roles:
    palette = PALETTES[r['palette']]
    tone = tone_of(r['tone'])
    bg = 'None'
    if r['background']:
        bg = 'Some(Background::{})'.format(ROLE_TO_BG[r['background']])
    bg2 = 'None'
    if r['secondBackground']:
        bg2 = 'Some(Background::{})'.format(ROLE_TO_BG[r['secondBackground']])
    curve_opt = 'None'
    if r['contrastCurve']:
        curve_opt = 'Some([{}])'.format(', '.join(f(v) for v in r['contrastCurve']))
    delta = 'None'
    if r['toneDeltaPair']:
        d = r['toneDeltaPair']
        delta = 'Some(DeltaPair {{ a: "{}", b: "{}", delta: {}, polarity: Polarity::{}, stay_together: {} }})'.format(
            d['a'], d['b'], f(d['delta']), d['polarity'].capitalize(), 'true' if d['stay_together'] else 'false')
    w('    Role {')
    w('        name: "{}",'.format(r['role']))
    w('        palette: PaletteId::{},'.format(palette))
    w('        tone: ToneRule::{},'.format(tone))
    w('        is_background: {},'.format('true' if r['isBackground'] else 'false'))
    w('        background: {},'.format(bg))
    w('        second_background: {},'.format(bg2))
    w('        curve: {},'.format(curve_opt))
    w('        delta: {},'.format(delta))
    w('    },')
w('];')

path = pathlib.Path(__file__).with_name('md3_data.rs')
path.write_text('\n'.join(out) + '\n')
print('roles written:', len(roles))
print('palettes:', sorted({r['palette'] for r in roles}))
print('backgrounds:', sorted({r['background'] for r in roles if r['background']}))
