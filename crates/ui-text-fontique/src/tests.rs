//! Tests for this module, split from the source file; one module per contract area.

pub(crate) mod tests_support;

mod contracts {
    use crate::*;

    #[test]
    fn isolated_provider_resolves_nothing() {
        let p = FontiqueProvider::new_isolated();
        assert!(
            p.resolve(&FontRequest::new(GenericFamily::SansSerif))
                .is_none()
        );
        assert!(p.fallback_chain(Script::LATIN).is_empty());
        assert!(p.face_data(FaceId(0)).is_none());
    }

    /// Skip loudly: a silently skipped test looks identical to a passing one.
    fn require_system(p: &FontiqueProvider) -> bool {
        if p.has_system_fonts() {
            return true;
        }
        eprintln!("skipped: no system fonts (build with `--features system` to run this test)");
        false
    }

    #[test]
    fn an_advance_is_the_shaped_width_of_the_face_it_names() {
        // 0.6 em per character made every face and every weight the same width, so a
        // caller holding a bold face still paid regular spacing.
        let p = system_or_isolated();
        if !require_system(&p) {
            return;
        }
        let text = "Hamburgefonstiv AV";
        let (Some(regular), Some(bold)) = (
            p.resolve(&FontRequest::new(GenericFamily::SansSerif)),
            p.resolve(&FontRequest::new(GenericFamily::SansSerif).weight(ui_core::Weight::BOLD)),
        ) else {
            return;
        };
        let shaped = |face| {
            let mut shaper = Shaper::new_with_axes(
                p.face_data(face).expect("a resolved face has bytes"),
                p.face_index(face),
                p.face_variation(face),
                p.face_axes(face),
            )
            .expect("the face parses");
            shaper.shape(text, 40.0, Script::LATIN).width
        };
        for face in [regular, bold] {
            let measured = p.advance(face, text, 40.0);
            let wanted = shaped(face);
            assert!(
                (measured - wanted).abs() < 0.01,
                "the advance must be the shaped width: {measured} against {wanted}"
            );
            assert_eq!(
                measured,
                p.advance(face, text, 40.0),
                "a second call reuses the cached shaper and cannot drift"
            );
        }
    }

    #[test]
    fn bold_and_regular_resolve_to_different_faces_and_bytes() {
        // The failure this pins: one slot per *family* meant every weight returned the
        // same id, and `fonts().first()` loaded Regular bytes for it.
        let p = system_or_isolated();
        if !require_system(&p) {
            return;
        }
        let regular = p.resolve(&FontRequest::new(GenericFamily::SansSerif));
        let bold =
            p.resolve(&FontRequest::new(GenericFamily::SansSerif).weight(ui_core::Weight::BOLD));
        let (Some(regular), Some(bold)) = (regular, bold) else {
            return;
        };
        assert_ne!(
            regular, bold,
            "bold must not resolve to the same face as regular"
        );
        // Both must load. Lengths can be equal on purpose: a variable font is a single
        // file whose faces differ by index, so "different bytes" is not the invariant.
        assert!(p.face_data(regular).is_some(), "regular bytes must load");
        assert!(p.face_data(bold).is_some(), "bold bytes must load");
    }

    #[test]
    fn a_registered_family_resolves_on_an_isolated_provider() {
        // A bundled icon font must resolve without any system font, and win under its own
        // name; otherwise an app renders `.notdef` on a machine with a different font set.
        let system = system_or_isolated();
        if !require_system(&system) {
            return;
        }
        let request =
            FontRequest::new(GenericFamily::SansSerif).family("Material Symbols Outlined");
        let Some(base) = system.resolve(&request) else {
            eprintln!("skipped: Material Symbols Outlined is not installed");
            return;
        };
        let Some(bytes) = system.face_data(base) else {
            eprintln!("skipped: Material Symbols Outlined has no loadable bytes");
            return;
        };
        let len = bytes.len();

        let mut p = FontiqueProvider::new_isolated();
        assert!(!p.has_system_fonts());
        let owned: std::sync::Arc<[u8]> = std::sync::Arc::from(bytes.bytes());
        assert!(
            p.register("Bundled Icons", owned),
            "registration must succeed"
        );
        let face = p
            .resolve(&FontRequest::new(GenericFamily::SansSerif).family("Bundled Icons"))
            .expect("a registered family must resolve without system fonts");
        assert_eq!(p.face_family_name(face).as_deref(), Some("Bundled Icons"));
        assert!(
            p.has_glyph(face, '\u{e88a}'),
            "the registered bytes must carry the glyph"
        );
        assert_eq!(p.face_data(face).map(|d| d.len()), Some(len));
    }

    /// The same bundled font handed over by a second surface is admitted once: the provider
    /// is shared by every surface on the thread and fontique keeps every face it is given,
    /// so a picker opened ten times would otherwise hold ten copies of its icon font.
    #[test]
    fn registering_a_family_again_keeps_one() {
        fn faces(p: &FontiqueProvider, name: &str) -> usize {
            let family = p.exact_family(name).expect("registered");
            let mut inner = p.inner.lock().expect("not poisoned");
            inner.family(family).map_or(0, |info| info.fonts().len())
        }

        let bytes: std::sync::Arc<[u8]> = std::sync::Arc::from(
            include_bytes!("../../assets/fonts/InterVariable.ttf").as_slice(),
        );
        let mut p = FontiqueProvider::new_isolated();
        assert!(p.register("Bundled Text", std::sync::Arc::clone(&bytes)));
        let admitted = faces(&p, "Bundled Text");
        assert!(admitted > 0, "the bytes must admit at least one face");
        // A second surface copies the same bytes, as `OpenOptions` does from a static.
        let copy: std::sync::Arc<[u8]> = std::sync::Arc::from(bytes.as_ref());
        assert!(p.register("Bundled Text", copy));
        assert_eq!(
            faces(&p, "Bundled Text"),
            admitted,
            "one set of faces, not a second admission of them"
        );
    }
}
