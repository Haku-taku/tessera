// Generated from docs/reference/hig-tokens.json. Do not edit by hand.

pub const CONTEXT_COUNT: usize = 4;

/// System colours: (name, [light, dark, light-increased-contrast, dark-increased-contrast]).
pub const SYSTEM_COLORS: &[(&str, [u32; CONTEXT_COUNT])] = &[
    ("systemBlue", [0xff0088ff, 0xff0091ff, 0xff1e6ef4, 0xff5cb8ff]),
    ("systemBrown", [0xffac7f5e, 0xffb78a66, 0xff956d51, 0xffdba679]),
    ("systemCyan", [0xff00c0e8, 0xff3cd3fe, 0xff007eae, 0xff6dd9ff]),
    ("systemGray", [0xff8e8e93, 0xff8e8e93, 0xff6c6c70, 0xffaeaeb2]),
    ("systemGreen", [0xff34c759, 0xff30d158, 0xff008932, 0xff4ad968]),
    ("systemIndigo", [0xff6155f5, 0xff6d7cff, 0xff564ade, 0xffa7aaff]),
    ("systemMint", [0xff00c8b3, 0xff00dac3, 0xff008575, 0xff54dfcb]),
    ("systemOrange", [0xffff8d28, 0xffff9230, 0xffc55300, 0xffffa056]),
    ("systemPink", [0xffff2d55, 0xffff375f, 0xffe7124d, 0xffff8ac4]),
    ("systemPurple", [0xffcb30e0, 0xffdb34f2, 0xffb02fc2, 0xffea8dff]),
    ("systemRed", [0xffff383c, 0xffff4245, 0xffe9152d, 0xffff6165]),
    ("systemTeal", [0xff00c3d0, 0xff00d2e0, 0xff008198, 0xff3bddec]),
    ("systemYellow", [0xffffcc00, 0xffffd600, 0xffa16a00, 0xfffedf43]),
];

/// The iOS grey ramp beyond `systemGray`.
pub const GRAY_RAMP: &[(&str, [u32; CONTEXT_COUNT])] = &[
    ("systemGray2", [0xffaeaeb2, 0xff636366, 0xff8e8e93, 0xff7c7c80]),
    ("systemGray3", [0xffc7c7cc, 0xff48484a, 0xffaeaeb2, 0xff545456]),
    ("systemGray4", [0xffd1d1d6, 0xff3a3a3c, 0xffbcbcc0, 0xff444446]),
    ("systemGray5", [0xffe5e5ea, 0xff2c2c2e, 0xffd8d8dc, 0xff363638]),
    ("systemGray6", [0xfff2f2f7, 0xff1c1c1e, 0xffebebf0, 0xff242426]),
];

/// Disabled-state colours; the values are published, not derived.
pub const DISABLED_COLORS: &[(&str, [u32; CONTEXT_COUNT])] = &[
    ("inactiveGray", [0xff999999, 0xff757575, 0xff999999, 0xff757575]),
];

/// System fill colours, translucent by design.
pub const FILL_COLORS: &[(&str, [u32; CONTEXT_COUNT])] = &[
    ("systemFill", [0x33787880, 0x5b787880, 0x47787880, 0x70787880]),
    ("secondarySystemFill", [0x28787880, 0x51787880, 0x3d787880, 0x66787880]),
    ("tertiarySystemFill", [0x1e767680, 0x3d767680, 0x33767680, 0x51767680]),
    ("quaternarySystemFill", [0x14747480, 0x2d767680, 0x28747480, 0x42767680]),
];

/// Hit-target minimums in points, per platform: (platform, default, minimum).
pub const HIT_TARGETS: &[(&str, f32, f32)] = &[
    ("ios_ipados", 44.0f32, 28.0f32),
    ("macos", 28.0f32, 20.0f32),
    ("tvos", 66.0f32, 56.0f32),
    ("visionos", 60.0f32, 28.0f32),
    ("watchos", 44.0f32, 28.0f32),
];

/// Content materials, thinnest to thickest.
pub const MATERIALS: &[&str] = &[
    "ultraThinMaterial",
    "thinMaterial",
    "regularMaterial",
    "thickMaterial",
    "ultraThickMaterial",
    "bar",
];

/// Standard content materials as our derivation: (name, blur sigma in points, tint argb,
/// fallback argb), each tint/fallback per context column. Apple publishes no numbers.
pub const MATERIAL_RECIPES: &[(&str, f32, [u32; CONTEXT_COUNT], [u32; CONTEXT_COUNT])] = &[
    ("ultraThinMaterial", 10.0f32, [0x29ffffff, 0x291c1c1e, 0x4fffffff, 0x4f1c1c1e], [0xffffffff, 0xff000000, 0xffffffff, 0xff000000]),
    ("thinMaterial", 14.0f32, [0x52ffffff, 0x521c1c1e, 0x78ffffff, 0x781c1c1e], [0xffffffff, 0xff000000, 0xffffffff, 0xff000000]),
    ("regularMaterial", 18.0f32, [0x80ffffff, 0x801c1c1e, 0xa6ffffff, 0xa61c1c1e], [0xffffffff, 0xff000000, 0xffffffff, 0xff000000]),
    ("thickMaterial", 22.0f32, [0xa8ffffff, 0xa81c1c1e, 0xcfffffff, 0xcf1c1c1e], [0xffffffff, 0xff000000, 0xffffffff, 0xff000000]),
    ("ultraThickMaterial", 26.0f32, [0xd1ffffff, 0xd11c1c1e, 0xf7ffffff, 0xf71c1c1e], [0xffffffff, 0xff000000, 0xffffffff, 0xff000000]),
    ("bar", 18.0f32, [0x9effffff, 0x9e1c1c1e, 0xc4ffffff, 0xc41c1c1e], [0xffffffff, 0xff000000, 0xffffffff, 0xff000000]),
];

/// Apple semantic colour role names; the values are resolved by the OS.
pub const SEMANTIC_ROLES: &[&str] = &[
    "label",
    "secondaryLabel",
    "tertiaryLabel",
    "quaternaryLabel",
    "systemBackground",
    "secondarySystemBackground",
    "tertiarySystemBackground",
    "systemGroupedBackground",
    "separator",
    "opaqueSeparator",
    "systemFill",
    "secondarySystemFill",
    "tertiarySystemFill",
    "quaternarySystemFill",
    "tintColor",
];
