//! Grid: L0 tracks plus L1 `minmax`, content keywords, `repeat` and dense flow.

use crate::layout::{AutoFlow, GridArea, Len, Repeat, Track, TrackBound};

/// Where one item landed; the order matches the input order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub row_span: usize,
    pub col_span: usize,
}

/// What one item asks of the tracks it spans.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: usize,
    pub span: usize,
    /// Narrowest the content can be: the floor a track may not go under.
    pub min: f32,
    /// Widest the content wants: the growth limit.
    pub max: f32,
}

/// Resolve a declared length against the space available on its axis.
pub fn resolve_len(len: Len, available: f32) -> Option<f32> {
    match len {
        Len::Px(v) => Some(v),
        Len::Pct(p) if available.is_finite() => Some(available * p / 100.0),
        _ => None,
    }
}

/// Resolve every item's cell, growing the grid for items outside the template.
pub fn place_items(
    areas: &[Option<GridArea>],
    spans: &[(usize, usize)],
    cols: usize,
    rows: usize,
    flow: AutoFlow,
) -> (Vec<Cell>, usize, usize) {
    let mut ncols = cols.max(1);
    let mut nrows = rows.max(1);
    for a in areas.iter().flatten() {
        ncols = ncols.max(a.col as usize + a.col_span as usize);
        nrows = nrows.max(a.row as usize + a.row_span as usize);
    }
    let mut cells = vec![
        Cell {
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1
        };
        areas.len()
    ];
    let mut used = vec![false; ncols * nrows];
    // Definite items first, so auto flow sees their cells taken.
    for (i, area) in areas.iter().enumerate() {
        let Some(a) = area else { continue };
        let cell = Cell {
            row: a.row as usize,
            col: a.col as usize,
            row_span: a.row_span as usize,
            col_span: a.col_span as usize,
        };
        for r in cell.row..(cell.row + cell.row_span).min(nrows) {
            for c in cell.col..(cell.col + cell.col_span).min(ncols) {
                used[r * ncols + c] = true;
            }
        }
        cells[i] = cell;
    }
    let (mut r, mut c) = (0usize, 0usize);
    for (i, area) in areas.iter().enumerate() {
        if area.is_some() {
            continue;
        }
        let (row_span, col_span) = spans.get(i).copied().unwrap_or((1, 1));
        let (row_span, col_span) = (row_span.max(1), col_span.max(1));
        // Dense flow restarts the scan at the origin for every item, so an earlier hole
        // is filled instead of staying empty for the rest of the grid.
        if flow.dense() {
            r = 0;
            c = 0;
        }
        loop {
            if flow.rows_first() {
                if c + col_span > ncols {
                    c = 0;
                    r += 1;
                }
                if r + row_span > nrows {
                    nrows = r + row_span;
                    used.resize(ncols * nrows, false);
                }
            } else {
                if r + row_span > nrows {
                    // A span taller than the template makes implicit rows, as CSS does;
                    // advancing the column instead would probe past the occupancy grid.
                    nrows = r + row_span;
                    used.resize(ncols * nrows, false);
                }
                if c + col_span > ncols {
                    let old = ncols;
                    ncols = c + col_span;
                    let mut grown = vec![false; ncols * nrows];
                    for row in 0..nrows {
                        for col in 0..old {
                            grown[row * ncols + col] = used[row * old + col];
                        }
                    }
                    used = grown;
                }
            }
            let free =
                (r..r + row_span).all(|rr| (c..c + col_span).all(|cc| !used[rr * ncols + cc]));
            if free {
                break;
            }
            if flow.rows_first() {
                c += 1;
            } else {
                r += 1;
            }
        }
        for rr in r..r + row_span {
            for cc in c..c + col_span {
                used[rr * ncols + cc] = true;
            }
        }
        cells[i] = Cell {
            row: r,
            col: c,
            row_span,
            col_span,
        };
        if flow.rows_first() {
            c += 1;
        } else {
            r += 1;
        }
    }
    (cells, ncols, nrows)
}

/// A `minmax()` bound as a length, or `None` for `auto` and `fr`.
fn bound_len(bound: TrackBound, available: f32) -> Option<f32> {
    match bound {
        TrackBound::Len(len) => resolve_len(len, available),
        TrackBound::Fr(_) => None,
    }
}

/// The flex factor a track takes part in the free-space split with, if any.
fn fr_factor(track: &Track) -> Option<f32> {
    match track {
        Track::Fr(f) => Some(f.max(0.0)),
        Track::MinMax(_, TrackBound::Fr(f)) => Some(f.max(0.0)),
        _ => None,
    }
}

/// The size a track starts from before the content contribution is shared out.
fn base_size(track: &Track, available: f32) -> f32 {
    match track {
        Track::Fixed(v) => v.max(0.0),
        Track::MinMax(min, _) => bound_len(*min, available).unwrap_or(0.0).max(0.0),
        _ => 0.0,
    }
}

/// The content an item asks of its spanned tracks in this pass.
///
/// `min-content` / `max-content` keywords override the pass; a mixed span takes the last one.
fn span_content(tracks: &[Track], span: &Span, use_min: bool) -> f32 {
    let mut content_is_min = use_min;
    let end = (span.start + span.span).min(tracks.len());
    for track in tracks.iter().take(end).skip(span.start.min(tracks.len())) {
        match track {
            Track::MinContent => content_is_min = true,
            Track::MaxContent => content_is_min = false,
            _ => {}
        }
    }
    if content_is_min { span.min } else { span.max }
}

/// Resolve track sizes from the content contributions.
///
/// Each track starts at its definite floor, grows by the min/max-content contribution it
/// shares with the items spanning it, and `minmax` clamps the result into its own bounds.
/// `fr` tracks then split what the other tracks leave over.
pub fn track_sizes(
    tracks: &[Track],
    spans: &[Span],
    available: f32,
    gap: f32,
    definite: bool,
    use_min: bool,
) -> Vec<f32> {
    let n = tracks.len();
    let mut sizes: Vec<f32> = tracks.iter().map(|t| base_size(t, available)).collect();
    if n == 0 {
        return sizes;
    }
    // A `fr` track only absorbs free space when the container has a definite extent;
    // otherwise it behaves like `auto`, exactly as an indefinite grid does in CSS.
    let flexible = |t: &Track| match t {
        Track::Fixed(_) => false,
        // A definite container lets a flexible track take its share, not its content.
        _ if definite && fr_factor(t).is_some() => false,
        _ => true,
    };
    let fr_in = |start: usize, span: usize| {
        (start..(start + span).min(n)).any(|i| fr_factor(&tracks[i]).is_some())
    };
    for s in spans {
        if definite && fr_in(s.start, s.span) {
            continue;
        }
        let end = (s.start + s.span).min(n);
        if s.start >= end {
            continue;
        }
        let autos = (s.start..end).filter(|i| flexible(&tracks[*i])).count();
        if autos == 0 {
            continue;
        }
        let taken: f32 = (s.start..end)
            .filter(|i| !flexible(&tracks[*i]))
            .map(|i| sizes[i])
            .sum();
        let budget =
            (span_content(tracks, s, use_min) - gap * s.span.saturating_sub(1) as f32 - taken)
                .max(0.0);
        let per = budget / autos as f32;
        for i in s.start..end {
            if flexible(&tracks[i]) {
                sizes[i] = sizes[i].max(per);
            }
        }
    }
    // `minmax` bounds apply after the content has been shared out: the track may grow past
    // its minimum but never past its maximum. An `Fr` upper bound is unbounded here and
    // is settled by the free-space split below.
    for (i, t) in tracks.iter().enumerate() {
        if let Track::MinMax(min, max) = t {
            let lo = bound_len(*min, available).unwrap_or(0.0).max(0.0);
            let hi = bound_len(*max, available).unwrap_or(f32::INFINITY);
            sizes[i] = sizes[i].max(lo).min(hi.max(lo));
        }
    }
    if definite && tracks.iter().any(|t| fr_factor(t).is_some()) {
        let fr = fr_size(tracks, &sizes, available, gap);
        for (i, t) in tracks.iter().enumerate() {
            if let Some(f) = fr_factor(t) {
                // A track takes `max(base, factor × fr)`: its own share, never base plus share.
                sizes[i] = sizes[i].max(f * fr);
            }
        }
    }
    sizes
}

/// CSS Grid §12.7.1: the size of one `fr`, from the space the rigid tracks leave over.
///
/// A track whose base exceeds its share counts as rigid for this pass, so every pass drops
/// at least one; `n + 1` passes settle. A flex sum under one keeps the rest of the space.
fn fr_size(tracks: &[Track], sizes: &[f32], available: f32, gap: f32) -> f32 {
    let space = available - gap * tracks.len().saturating_sub(1) as f32;
    // A NaN space must count as none: clippy's suggested `space <= 0.0` would let it through.
    if space.is_nan() || space <= 0.0 {
        return 0.0;
    }
    let mut fr = f32::INFINITY;
    for _ in 0..=tracks.len() {
        let mut rigid = 0.0f32;
        let mut factors = 0.0f32;
        for (i, t) in tracks.iter().enumerate() {
            match fr_factor(t) {
                Some(f) if f * fr >= sizes[i] => factors += f,
                _ => rigid += sizes[i],
            }
        }
        let last = fr;
        fr = (space - rigid).max(0.0) / factors.max(1.0);
        // A track whose share no longer covers its base turned rigid this pass; when none
        // does, the fraction is settled.
        let turned = tracks
            .iter()
            .enumerate()
            .any(|(i, t)| fr_factor(t).is_some_and(|f| f * fr < sizes[i] && f * last >= sizes[i]));
        if !turned {
            break;
        }
    }
    fr
}

/// Extend a template to `n` tracks with implicit `Auto` ones.
pub fn expand(tracks: &[Track], n: usize) -> Vec<Track> {
    let mut out = tracks.to_vec();
    out.resize(n, Track::Auto);
    out
}

/// A template with every `repeat()` spelled out, plus the tracks an `auto-fit` added.
pub struct ExpandedTracks {
    pub tracks: Vec<Track>,
    /// Indices an `auto-fit` repeat produced; those no item lands in collapse after placement.
    pub auto_fit: Vec<usize>,
}

/// The definite size a track is counted at, or `None` when it has no definite component.
fn definite_track_size(track: &Track, available: f32) -> Option<f32> {
    match track {
        Track::Fixed(v) => Some(v.max(0.0)),
        Track::MinMax(min, max) => {
            let lo = bound_len(*min, available);
            let hi = bound_len(*max, available);
            match (lo, hi) {
                // `max` below `min` is pinned up to `min`, so the larger one counts.
                (Some(lo), Some(hi)) => Some(lo.max(hi)),
                (Some(lo), None) => Some(lo),
                (None, Some(hi)) => Some(hi),
                (None, None) => None,
            }
        }
        _ => None,
    }
}

/// How many tracks an auto repeat fits: `floor((available + gap) / (track + gap))`.
fn auto_repeat_count(track: &Track, available: f32, gap: f32) -> usize {
    let Some(size) = definite_track_size(track, available) else {
        return 1;
    };
    if !available.is_finite() || size <= 0.0 {
        return 1;
    }
    let count = ((available + gap) / (size + gap)).floor();
    if count.is_finite() {
        (count as usize).max(1)
    } else {
        1
    }
}

/// Spell out every `repeat()` in a template.
pub fn expand_repeat(tracks: &[Track], available: f32, gap: f32) -> ExpandedTracks {
    let mut out: Vec<Track> = Vec::new();
    let mut auto_fit: Vec<usize> = Vec::new();
    for track in tracks {
        match track {
            Track::Repeat(Repeat::Count(n), inner) => {
                for _ in 0..*n {
                    let sub = expand_repeat(std::slice::from_ref(inner.as_ref()), available, gap);
                    let offset = out.len();
                    auto_fit.extend(sub.auto_fit.into_iter().map(|i| i + offset));
                    out.extend(sub.tracks);
                }
            }
            Track::Repeat(mode, inner) => {
                let count = auto_repeat_count(inner, available, gap);
                for _ in 0..count {
                    if *mode == Repeat::AutoFit {
                        auto_fit.push(out.len());
                    }
                    out.push((**inner).clone());
                }
            }
            other => out.push(other.clone()),
        }
    }
    ExpandedTracks {
        tracks: out,
        auto_fit,
    }
}

/// Drop the `auto-fit` tracks no item landed in, renumbering the cells around them.
///
/// Returns the track count after the collapse. The gutter of a collapsed track goes with
/// it, which is why a span shrinks by the number of tracks it lost.
pub fn collapse_auto_fit(
    cells: &mut [Cell],
    tracks: &mut Vec<Track>,
    auto_fit: &[usize],
    columns: bool,
) -> usize {
    let start_of = |c: &Cell| if columns { c.col } else { c.row };
    let span_of = |c: &Cell| if columns { c.col_span } else { c.row_span };
    let mut occupied = vec![false; tracks.len()];
    for cell in cells.iter() {
        let end = (start_of(cell) + span_of(cell)).min(tracks.len());
        for slot in occupied
            .iter_mut()
            .take(end)
            .skip(start_of(cell).min(tracks.len()))
        {
            *slot = true;
        }
    }
    let mut remove = vec![false; tracks.len()];
    for i in auto_fit {
        if *i < remove.len() && !occupied[*i] {
            remove[*i] = true;
        }
    }
    if !remove.contains(&true) {
        return tracks.len();
    }
    // `remap[i]` is where old track `i` lands; a removed one maps to the next kept track.
    let mut remap = vec![0usize; tracks.len() + 1];
    let mut kept = 0usize;
    for (i, gone) in remove.iter().enumerate() {
        remap[i] = kept;
        if !gone {
            kept += 1;
        }
    }
    remap[tracks.len()] = kept;
    for cell in cells.iter_mut() {
        let start = start_of(cell);
        let end = (start + span_of(cell)).min(tracks.len());
        let span = (start.min(tracks.len())..end)
            .filter(|i| !remove[*i])
            .count()
            .max(1);
        let new_start = remap[start.min(tracks.len())].min(kept.saturating_sub(1));
        if columns {
            cell.col = new_start;
            cell.col_span = span;
        } else {
            cell.row = new_start;
            cell.row_span = span;
        }
    }
    let mut i = 0usize;
    tracks.retain(|_| {
        let gone = remove[i];
        i += 1;
        !gone
    });
    tracks.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::TrackBound;

    /// A `minmax` floor is a floor: the track still takes its `fr` share, not the floor on top.
    #[test]
    fn a_minmax_track_takes_its_share_rather_than_the_share_on_top_of_its_floor() {
        let tracks = [
            Track::MinMax(TrackBound::Len(Len::Px(200.0)), TrackBound::Fr(1.0)),
            Track::Fr(1.0),
        ];
        let sizes = track_sizes(&tracks, &[], 500.0, 0.0, true, false);
        assert_eq!(sizes, vec![250.0, 250.0], "both tracks are one fr of 500");
    }

    /// A flex sum under one leaves the rest of the space alone, as `flex-grow` does.
    #[test]
    fn a_flex_sum_below_one_leaves_the_rest_of_the_space_alone() {
        let sizes = track_sizes(&[Track::Fr(0.5)], &[], 300.0, 0.0, true, false);
        assert_eq!(
            sizes,
            vec![150.0],
            "half of one fr of 300, not the whole box"
        );
    }
}
