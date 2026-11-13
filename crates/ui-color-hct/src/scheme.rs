//! MD3's tonal-spot scheme: the role colours one seed derives.
//!
//! The palettes and the fixed tone each role sits at are material-color-utilities'
//! `SchemeTonalSpot` and `MaterialDynamicColors` (Dart binding); the contrast curves that
//! move those tones for the other levels are not here yet.

use crate::cam::cam_from_argb;
use crate::palette::TonalPalette;

/// One role of the scheme, in the order the tonal-spot table lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Primary,
    OnPrimary,
    PrimaryContainer,
    OnPrimaryContainer,
    Secondary,
    OnSecondary,
    SecondaryContainer,
    OnSecondaryContainer,
    Tertiary,
    OnTertiary,
    TertiaryContainer,
    OnTertiaryContainer,
    Error,
    OnError,
    ErrorContainer,
    OnErrorContainer,
    Surface,
    OnSurface,
    SurfaceVariant,
    OnSurfaceVariant,
    Outline,
    OutlineVariant,
    InverseSurface,
    InverseOnSurface,
    InversePrimary,
    Shadow,
    Scrim,
}

impl Role {
    pub const ALL: [Self; 27] = [
        Self::Primary,
        Self::OnPrimary,
        Self::PrimaryContainer,
        Self::OnPrimaryContainer,
        Self::Secondary,
        Self::OnSecondary,
        Self::SecondaryContainer,
        Self::OnSecondaryContainer,
        Self::Tertiary,
        Self::OnTertiary,
        Self::TertiaryContainer,
        Self::OnTertiaryContainer,
        Self::Error,
        Self::OnError,
        Self::ErrorContainer,
        Self::OnErrorContainer,
        Self::Surface,
        Self::OnSurface,
        Self::SurfaceVariant,
        Self::OnSurfaceVariant,
        Self::Outline,
        Self::OutlineVariant,
        Self::InverseSurface,
        Self::InverseOnSurface,
        Self::InversePrimary,
        Self::Shadow,
        Self::Scrim,
    ];

    /// The role's name as MD3 spells it: `on-primary-container`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::OnPrimary => "on-primary",
            Self::PrimaryContainer => "primary-container",
            Self::OnPrimaryContainer => "on-primary-container",
            Self::Secondary => "secondary",
            Self::OnSecondary => "on-secondary",
            Self::SecondaryContainer => "secondary-container",
            Self::OnSecondaryContainer => "on-secondary-container",
            Self::Tertiary => "tertiary",
            Self::OnTertiary => "on-tertiary",
            Self::TertiaryContainer => "tertiary-container",
            Self::OnTertiaryContainer => "on-tertiary-container",
            Self::Error => "error",
            Self::OnError => "on-error",
            Self::ErrorContainer => "error-container",
            Self::OnErrorContainer => "on-error-container",
            Self::Surface => "surface",
            Self::OnSurface => "on-surface",
            Self::SurfaceVariant => "surface-variant",
            Self::OnSurfaceVariant => "on-surface-variant",
            Self::Outline => "outline",
            Self::OutlineVariant => "outline-variant",
            Self::InverseSurface => "inverse-surface",
            Self::InverseOnSurface => "inverse-on-surface",
            Self::InversePrimary => "inverse-primary",
            Self::Shadow => "shadow",
            Self::Scrim => "scrim",
        }
    }

    /// The index the colour table stores this role at: its declaration order.
    pub const fn index(self) -> usize {
        self as usize
    }

    fn slot(self) -> usize {
        self.index()
    }
}

/// The role colours one seed derives.
#[derive(Clone, Copy, Debug)]
pub struct Scheme {
    roles: [u32; Role::ALL.len()],
}

impl Scheme {
    /// The tonal-spot scheme of `seed`, at the standard contrast level.
    pub fn tonal_spot(seed: u32) -> Self {
        let hue = cam_from_argb(seed).hue;
        let primary = TonalPalette::new(hue, 36.0);
        let secondary = TonalPalette::new(hue, 16.0);
        let tertiary = TonalPalette::new((hue + 60.0).rem_euclid(360.0), 24.0);
        let neutral = TonalPalette::new(hue, 6.0);
        let neutral_variant = TonalPalette::new(hue, 8.0);
        let error = TonalPalette::new(25.0, 84.0);
        // The light column of `MaterialDynamicColors`: the tone each role sits at.
        let table = [
            (Role::Primary, &primary, 40.0),
            (Role::OnPrimary, &primary, 100.0),
            (Role::PrimaryContainer, &primary, 90.0),
            (Role::OnPrimaryContainer, &primary, 30.0),
            (Role::Secondary, &secondary, 40.0),
            (Role::OnSecondary, &secondary, 100.0),
            (Role::SecondaryContainer, &secondary, 90.0),
            (Role::OnSecondaryContainer, &secondary, 30.0),
            (Role::Tertiary, &tertiary, 40.0),
            (Role::OnTertiary, &tertiary, 100.0),
            (Role::TertiaryContainer, &tertiary, 90.0),
            (Role::OnTertiaryContainer, &tertiary, 30.0),
            (Role::Error, &error, 40.0),
            (Role::OnError, &error, 100.0),
            (Role::ErrorContainer, &error, 90.0),
            (Role::OnErrorContainer, &error, 30.0),
            (Role::Surface, &neutral, 98.0),
            (Role::OnSurface, &neutral, 10.0),
            (Role::SurfaceVariant, &neutral_variant, 90.0),
            (Role::OnSurfaceVariant, &neutral_variant, 30.0),
            (Role::Outline, &neutral_variant, 50.0),
            (Role::OutlineVariant, &neutral_variant, 80.0),
            (Role::InverseSurface, &neutral, 20.0),
            (Role::InverseOnSurface, &neutral, 95.0),
            (Role::InversePrimary, &primary, 80.0),
            (Role::Shadow, &neutral, 0.0),
            (Role::Scrim, &neutral, 0.0),
        ];
        let mut roles = [0; Role::ALL.len()];
        for (role, palette, tone) in table {
            roles[role.slot()] = palette.tone(tone);
        }
        Self { roles }
    }

    /// The colour of one role, as ARGB.
    pub fn colour(&self, role: Role) -> u32 {
        self.roles[role.slot()]
    }

    /// Every role with its colour, in `Role::ALL`'s order.
    pub fn roles(&self) -> impl Iterator<Item = (Role, u32)> + '_ {
        Role::ALL.into_iter().map(|role| (role, self.colour(role)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contrast::ratio_of_tones;
    use crate::lab::tone_of;

    /// Every role sits at the tone `MaterialDynamicColors` gives it, the baseline seed is
    /// its own primary, and each role with a contrast curve meets that curve against the
    /// surface or container it is drawn on.
    #[test]
    fn a_seed_yields_the_md3_roles_of_the_tonal_spot_scheme() {
        let scheme = Scheme::tonal_spot(0xFF67_50A4);
        let tones = [
            (Role::Primary, 40.0),
            (Role::OnPrimary, 100.0),
            (Role::PrimaryContainer, 90.0),
            (Role::OnPrimaryContainer, 30.0),
            (Role::Secondary, 40.0),
            (Role::OnSecondary, 100.0),
            (Role::SecondaryContainer, 90.0),
            (Role::OnSecondaryContainer, 30.0),
            (Role::Tertiary, 40.0),
            (Role::OnTertiary, 100.0),
            (Role::TertiaryContainer, 90.0),
            (Role::OnTertiaryContainer, 30.0),
            (Role::Error, 40.0),
            (Role::OnError, 100.0),
            (Role::ErrorContainer, 90.0),
            (Role::OnErrorContainer, 30.0),
            (Role::Surface, 98.0),
            (Role::OnSurface, 10.0),
            (Role::SurfaceVariant, 90.0),
            (Role::OnSurfaceVariant, 30.0),
            (Role::Outline, 50.0),
            (Role::OutlineVariant, 80.0),
            (Role::InverseSurface, 20.0),
            (Role::InverseOnSurface, 95.0),
            (Role::InversePrimary, 80.0),
            (Role::Shadow, 0.0),
            (Role::Scrim, 0.0),
        ];
        for (role, tone) in tones {
            let at = tone_of(scheme.colour(role));
            assert!(
                (at - tone).abs() < 0.6,
                "{} sits at {at}, not {tone}",
                role.name()
            );
        }
        // The seed is a source, not the primary: the tonal-spot primary is the seed's hue
        // at chroma 36, which is duller than MD3's baseline purple (chroma 47.9).
        let chroma = |role: Role| cam_from_argb(scheme.colour(role)).chroma;
        assert!(chroma(Role::Primary) > chroma(Role::Secondary) + 10.0);
        assert!(chroma(Role::Secondary) > chroma(Role::Surface));
        assert!(chroma(Role::Surface) < 8.0, "the surface stays neutral");

        let ratio =
            |a: Role, b: Role| ratio_of_tones(tone_of(scheme.colour(a)), tone_of(scheme.colour(b)));
        for (text, on, wanted) in [
            (Role::Primary, Role::Surface, 4.5),
            // Tone 100 is as light as a palette goes, so a body colour on a tone-40
            // accent reaches 6.4:1, not the 7 the curve asks for.
            (Role::OnPrimary, Role::Primary, 4.5),
            (Role::OnPrimaryContainer, Role::PrimaryContainer, 4.5),
            (Role::OnSecondary, Role::Secondary, 4.5),
            (Role::OnSecondaryContainer, Role::SecondaryContainer, 4.5),
            (Role::OnTertiary, Role::Tertiary, 4.5),
            (Role::OnError, Role::Error, 4.5),
            (Role::OnSurface, Role::Surface, 7.0),
            (Role::OnSurfaceVariant, Role::SurfaceVariant, 4.5),
            (Role::Outline, Role::Surface, 3.0),
            (Role::InversePrimary, Role::InverseSurface, 4.5),
        ] {
            let found = ratio(text, on);
            assert!(
                found >= wanted - 0.01,
                "{} on {} is {found}, under {wanted}",
                text.name(),
                on.name()
            );
        }
    }

    /// The colour table indexes by declaration order, so `ALL` must list the roles in it.
    #[test]
    fn the_role_list_is_the_declaration_order() {
        for (index, role) in Role::ALL.into_iter().enumerate() {
            assert_eq!(role.index(), index, "{}", role.name());
        }
    }
}
