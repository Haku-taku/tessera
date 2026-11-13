//! Style tokens: named, data-only, and swappable at runtime.

pub mod cache;
pub mod capability;
pub mod common_plane;
pub mod control_metrics;
pub mod env;
pub mod legacy;
pub mod material;
pub mod motion;
pub mod pack;
pub mod roles;
pub mod style;
pub mod types;

pub use cache::StyleCache;
pub use capability::Capability;
pub use common_plane::CommonPlane;
pub use control_metrics::{ControlKind, ControlMetrics, ControlMetricsTable, Density};
pub use env::{Env, StyleScope};
pub use legacy::legacy_pack;
pub use material::Material;
pub use motion::{MotionName, MotionTable, MotionToken};
pub use pack::{PaletteHandle, Style, StyleBuilder, lerp_style, stub_type_table};
pub use roles::{RoleDict, RoleName};
pub use style::{SliderSpec, ToggleSpec};
pub use types::{MetricsSet, TextRole, TextStyleToken, TypeTable, WidgetMetrics};
