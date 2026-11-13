//! Resolving text to faces, with an explicit, inspectable fallback chain.

use crate::provider::{FaceId, FaceMetrics, FontProvider, FontRequest, Script};

/// Outcome of trying to find a face that covers a whole run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackOutcome {
    /// The primary face covers the run; no fallback was attempted.
    NotNeeded,
    /// A later face in the chain covers the whole run.
    Used,
    /// No single face covers the run; mixed-script runs need per-cluster
    /// splitting, which belongs to the shaper.
    Unresolved,
}

/// A resolved face plus the metrics and width computed for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedFont {
    pub face: FaceId,
    pub metrics: FaceMetrics,
    pub width: f32,
    /// How many fallback steps were taken before a covering face was found.
    pub fallback_steps: u8,
    pub outcome: FallbackOutcome,
}

/// The font stack: a provider plus per-script caching of the chain.
pub struct FontStack {
    provider: Box<dyn FontProvider>,
    /// Cached fallback chains, invalidated only on provider swap.
    chains: std::collections::HashMap<Script, Vec<FaceId>>,
    /// Default request used when the caller names nothing.
    default_request: FontRequest,
    /// Counters, for tests and diagnostics.
    pub stats: StackStats,
}

/// How often the stack had to fall back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StackStats {
    pub resolves: u64,
    pub fallbacks: u64,
    /// Faces tried and rejected because they lacked a glyph.
    pub rejected: u64,
    /// Runs no single face could cover (mixed scripts).
    pub unresolved: u64,
}

impl FontStack {
    pub fn new(provider: impl FontProvider + 'static) -> Self {
        Self {
            provider: Box::new(provider),
            chains: std::collections::HashMap::new(),
            default_request: FontRequest::new(crate::provider::GenericFamily::SansSerif),
            stats: StackStats::default(),
        }
    }

    pub fn provider(&self) -> &dyn FontProvider {
        self.provider.as_ref()
    }

    pub fn provider_name(&self) -> &'static str {
        self.provider.name()
    }

    pub fn set_default_request(&mut self, request: FontRequest) {
        self.default_request = request;
    }

    /// Chain for `script`, cached after the first query.
    pub fn chain(&mut self, script: Script) -> &[FaceId] {
        self.chains
            .entry(script)
            .or_insert_with(|| self.provider.fallback_chain(script))
    }

    /// Pick the first face in the chain that covers every non-space char of `text`.
    ///
    /// Returns the primary face when nothing covers the whole run; per-cluster
    /// splitting is a shaping concern and lands with the shaper.
    pub fn face_for_text(
        &mut self,
        request: &FontRequest,
        text: &str,
    ) -> (FaceId, u8, FallbackOutcome) {
        self.stats.resolves += 1;
        let primary = self
            .provider
            .resolve(request)
            .or_else(|| self.chain(request.script).first().copied());

        let Some(primary) = primary else {
            return (FaceId(0), 0, FallbackOutcome::Unresolved);
        };

        let needs_fallback = text
            .chars()
            .any(|c| !c.is_whitespace() && !self.provider.has_glyph(primary, c));
        if !needs_fallback {
            return (primary, 0, FallbackOutcome::NotNeeded);
        }

        let chain = self.chain(request.script).to_vec();
        for (steps, face) in chain.iter().enumerate().skip(1) {
            let covers = text
                .chars()
                .all(|c| c.is_whitespace() || self.provider.has_glyph(*face, c));
            self.stats.rejected += 1;
            if covers {
                self.stats.fallbacks += 1;
                return (
                    *face,
                    steps.min(u8::MAX as usize) as u8,
                    FallbackOutcome::Used,
                );
            }
        }
        // No single face covers the run. Mixed-script runs need the shaper to split
        // per cluster; until then the primary stays responsible and the outcome says so.
        self.stats.unresolved += 1;
        (primary, 0, FallbackOutcome::Unresolved)
    }

    /// Resolve `text` and compute its width, using the default request.
    pub fn resolve_for_text(&mut self, text: &str, size: f32) -> ResolvedFont {
        let mut request = self.default_request.clone();
        // Narrow the chain to the script actually present in the run.
        request.script = Script::dominant(text);
        self.resolve(&request, text, size)
    }

    /// Resolve `text` with an explicit request.
    pub fn resolve(&mut self, request: &FontRequest, text: &str, size: f32) -> ResolvedFont {
        let (face, fallback_steps, outcome) = self.face_for_text(request, text);
        let metrics = self.provider.metrics(face);
        let width = self.provider.advance(face, text, size);
        ResolvedFont {
            face,
            metrics,
            width,
            fallback_steps,
            outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{GenericFamily, StaticProvider};

    fn stack() -> FontStack {
        // Face 0 covers only Latin; face 1 covers CJK.
        let provider = StaticProvider::new("test")
            .with_partial_face("Latin Only", 0.6, &['a', 'b', 'c', ' '])
            .with_partial_face("CJK Only", 0.75, &['中', '文', ' ']);
        FontStack::new(provider)
    }

    #[test]
    fn cjk_text_falls_back_to_the_next_face() {
        let mut s = stack();
        let r = s.resolve_for_text("中文", 10.0);
        assert_eq!(r.face, FaceId(1), "must pick the face that has the glyphs");
        assert_eq!(r.fallback_steps, 1);
        assert_eq!(s.stats.fallbacks, 1);
    }

    #[test]
    fn mixed_script_run_reports_unresolved_instead_of_silently_using_the_primary() {
        let mut s = stack();
        // Latin + CJK in one run: no single face covers everything.
        let r = s.resolve_for_text("abc中文", 10.0);
        assert_eq!(
            r.outcome,
            FallbackOutcome::Unresolved,
            "mixed-script runs must be reported, not silently misattributed"
        );
        assert_eq!(s.stats.unresolved, 1);
    }

    #[test]
    fn named_family_wins_over_the_generic_default() {
        let provider = StaticProvider::new("test")
            .with_face("First", 0.6)
            .with_face("Mono Face", 0.5);
        let mut s = FontStack::new(provider);
        let req = FontRequest::new(GenericFamily::Monospace).family("Mono Face");
        let r = s.resolve(&req, "abc", 10.0);
        assert_eq!(r.face, FaceId(1));
        assert!((r.width - 15.0).abs() < 0.01, "0.5 * 10 * 3 chars");
    }
}
