"""Regenerate docs/reference/md3-color-roles.json from the MCU reference source.

Each role's tone lambda is decoded into a small structured form the Rust side can evaluate
without a C++ parser:

  {"kind": "pair",             "light": L, "dark": D}                 # s.is_dark ? D : L
  {"kind": "const",            "value": V}                            # return V
  {"kind": "light_curve",      "dark": D, "curve": [a,b,c,d]}          # dark const, light curve
  {"kind": "dark_curve",       "light": L, "curve": [a,b,c,d]}         # light const, dark curve
  {"kind": "both_curve",       "dark": [..], "light": [..]}
  {"kind": "fidelity_default", "light": L, "dark": D, "fidelity": "..."}  # fidelity off
  {"kind": "mono_pair",        "light": L, "dark": D}                  # monochrome branch dropped

Only the monochrome variant and the fidelity schemes differ; both are recorded as metadata so
nothing is silently lost. TonalSpot takes neither branch.
"""
import json, pathlib, re

ROOT = pathlib.Path('/home/ollie/git/crabsnap-ng')
SRC = pathlib.Path(
    '/home/ollie/git/noctalia/third_party/material_color_utilities/cpp'
    '/dynamiccolor/material_dynamic_colors.cc')
text = SRC.read_text()


def norm(s):
    return re.sub(r'\s+', ' ', str(s)).strip()


def constructor_args(body):
    marker = 'return DynamicColor('
    if marker not in body:
        return None
    i = body.index(marker) + len(marker)
    depth, instr, prev = 1, False, ''
    parts, cur = [], []
    for ch in body[i:]:
        if instr:
            cur.append(ch)
            if ch == '"' and prev != '\\':
                instr = False
            prev = ch
            continue
        if ch == '"':
            instr = True
            cur.append(ch)
            prev = ch
            continue
        if ch in '([{':
            depth += 1
        elif ch in ')]}':
            depth -= 1
            if depth == 0:
                break
        if ch == ',' and depth == 1:
            parts.append(''.join(cur))
            cur = []
        else:
            cur.append(ch)
        prev = ch
    if cur:
        parts.append(''.join(cur))
    out = {}
    for p in parts:
        m = re.match(r'\s*/\*\s*(\w+)=\s*\*/(.*)', p, re.S)
        if m:
            out[m.group(1)] = m.group(2).strip()
    return out


CURVE = r'ContrastCurve\(([^)]*)\)'


def curve_tuple(s):
    return [float(x) for x in s.split(',')]


def parse_tone(t):
    t = norm(t)
    m = re.fullmatch(r'\[\]\(const DynamicScheme& s\) -> double \{'
                     r' return s\.is_dark \? ([0-9.]+) : ([0-9.]+); \}', t)
    if m:
        return {'kind': 'pair', 'dark': float(m.group(1)), 'light': float(m.group(2))}
    m = re.fullmatch(r'\[\]\(const DynamicScheme& s\) -> double \{ return ([0-9.]+); \}', t)
    if m:
        return {'kind': 'const', 'value': float(m.group(1))}
    m = re.search(r'return s\.is_dark \? ([0-9.]+) : ' + CURVE, t)
    if m:
        return {'kind': 'light_curve', 'dark': float(m.group(1)), 'curve': curve_tuple(m.group(2))}
    m = re.search(r'return s\.is_dark \? ' + CURVE + r'[^:]*: ([0-9.]+)', t)
    if m:
        return {'kind': 'dark_curve', 'light': float(m.group(2)), 'curve': curve_tuple(m.group(1))}
    m = re.search(r'return s\.is_dark \? ' + CURVE + r'[^:]*: ' + CURVE, t)
    if m:
        return {'kind': 'both_curve', 'dark': curve_tuple(m.group(1)),
                'light': curve_tuple(m.group(2))}
    stereo = t
    fidelity = None
    m = re.search(r'if \(IsFidelity\(s\)\) \{ ([^;]*;?) \}', stereo)
    if m:
        fidelity = norm(m.group(1))
        stereo = stereo.replace(m.group(0), '')
    mono = re.search(r'if \(IsMonochrome\(s\)\) \{ ([^}]*) \}', stereo)
    if mono:
        stereo = stereo.replace(mono.group(0), '')
    # `double initialTone = s.is_dark ? 30.0 : 90.0;` then `if (!IsFidelity(s)) { return initialTone; }`
    pre = re.search(r'double initialTone = s\.is_dark \? ([0-9.]+) : ([0-9.]+);', stereo)
    if pre:
        return {'kind': 'fidelity_default', 'dark': float(pre.group(1)),
                'light': float(pre.group(2)), 'fidelity': norm(t)}
    stereo = stereo.replace('else {', '{')
    m = re.search(r'return IsMonochrome\(s\) \? ([0-9.]+) : ([0-9.]+);', stereo)
    if m:
        return {'kind': 'mono_pair', 'light': float(m.group(2)), 'dark': float(m.group(2)),
                'alt_monochrome': float(m.group(1))}
    m = re.search(r'return s\.is_dark \? ([0-9.]+) : ([0-9.]+);', stereo)
    if m:
        d, l = float(m.group(1)), float(m.group(2))
        out = {'kind': 'fidelity_default' if fidelity else 'pair', 'dark': d, 'light': l}
        if fidelity:
            out['fidelity'] = fidelity
        return out
    return {'kind': 'raw', 'expr': t}


def parse_curve(c):
    m = re.fullmatch(CURVE, norm(c))
    return curve_tuple(m.group(1)) if m else None


def parse_bg(b):
    b = norm(b)
    if b in ('', 'nullopt'):
        return None
    if 'highestSurface' in b:
        return 'highestSurface'
    m = re.search(r'return (\w+)\(\)', b)
    if not m:
        raise SystemExit('unparsed background expression: ' + b)
    return m.group(1)


def parse_delta(d):
    d = norm(d)
    m = re.search(r'ToneDeltaPair\((\w+)\(\), (\w+)\(\), ([0-9.]+), '
                  r'TonePolarity::k(\w+), (true|false)\)', d)
    if not m:
        return None
    return {'a': FN_TO_ROLE.get(m.group(1), m.group(1)),
            'b': FN_TO_ROLE.get(m.group(2), m.group(2)),
            'delta': float(m.group(3)),
            'polarity': m.group(4).lower(), 'stay_together': m.group(5) == 'true'}


fns = re.findall(r'DynamicColor MaterialDynamicColors::(\w+)\(\) \{(.*?)\n\}', text, re.S)
# fn name -> role name, so delta pairs can be stored with the names the Rust side uses.
FN_TO_ROLE = {}
for fn, body in fns:
    args = constructor_args(body)
    if args and 'name' in args:
        FN_TO_ROLE[fn] = args['name'].strip('"')
roles = []
for fn, body in fns:
    args = constructor_args(body)
    if args is None:
        continue
    name = args['name'].strip('"')
    palette = re.search(r'return s\.(\w+)_palette', args.get('palette', ''))
    roles.append({
        'role': name,
        'fn': fn,
        'palette': palette.group(1) if palette else None,
        'tone': parse_tone(args.get('tone', '')),
        'isBackground': norm(args.get('isBackground', 'false')) == 'true',
        'background': parse_bg(args.get('background', '')),
        'secondBackground': parse_bg(args.get('secondBackground', '')),
        'contrastCurve': parse_curve(args.get('contrastCurve', 'nullopt')),
        'toneDeltaPair': parse_delta(args.get('toneDeltaPair', 'nullopt')),
    })

assert len(roles) == 49, len(roles)
old = json.loads((ROOT / 'docs/reference/md3-color-roles.json').read_text())
doc = {
    '$source': old['$source'],
    'keyColors': old['keyColors'],
    'toneLaw': old['toneLaw'],
    'roles': roles,
}
doc['$source']['rolesNote'] = (
    'Decoded from material_dynamic_colors.cc into structured tones. TonalSpot takes neither '
    'the fidelity nor the monochrome branch, so fidelity_default roles use their light/dark '
    'constants and mono_pair roles use dark==light.')
(ROOT / 'docs/reference/md3-color-roles.json').write_text(
    json.dumps(doc, indent=1, ensure_ascii=False) + '\n')

kinds = {}
for r in roles:
    kinds[r['tone']['kind']] = kinds.get(r['tone']['kind'], 0) + 1
print('roles', len(roles), '| tone kinds', kinds)
print('with curve', sum(1 for r in roles if r['contrastCurve']),
      '| delta', sum(1 for r in roles if r['toneDeltaPair']),
      '| bg', sum(1 for r in roles if r['background']),
      '| bg2', sum(1 for r in roles if r['secondBackground']))
print('still raw:', [r['role'] for r in roles if r['tone']['kind'] == 'raw'])
