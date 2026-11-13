//! Flex main-axis distribution, following CSS Flexbox §9.7.

/// One item's main-axis inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    /// Flex base size, before min/max clamping.
    pub basis: f32,
    pub grow: f32,
    pub shrink: f32,
    /// Resolved minimum main size; the content floor already folded in.
    pub min: f32,
    pub max: f32,
    /// `clamp(basis, min, max)`: what the item wants before free space is shared.
    pub hypothetical: f32,
}

/// Slack for comparisons and the freeze decision.
const EPS: f32 = 1e-4;

/// Resolve one line's main sizes: free space to growers, a deficit to scaled shrinkers,
/// freezing each item as it reaches its own bound.
///
/// `available` is the container's inner main size; `gap` separates adjacent items.
pub fn solve_line(items: &[Item], available: f32, gap: f32) -> Vec<f32> {
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }
    let gap_total = gap * (n.saturating_sub(1)) as f32;
    let hypothetical_total: f32 = items.iter().map(|i| i.hypothetical).sum::<f32>() + gap_total;
    if (hypothetical_total - available).abs() <= EPS {
        return items.iter().map(|i| i.hypothetical).collect();
    }
    let growing = hypothetical_total < available;
    let factor = |i: &Item| if growing { i.grow } else { i.shrink };
    let mut target: Vec<f32> = items.iter().map(|i| i.basis).collect();
    let mut frozen = vec![false; n];
    for (idx, item) in items.iter().enumerate() {
        // Nothing to gain from a zero factor; a basis already past its hypothetical freezes.
        let past = (growing && item.basis > item.hypothetical)
            || (!growing && item.basis < item.hypothetical);
        if factor(item) == 0.0 || past {
            frozen[idx] = true;
            target[idx] = item.hypothetical;
        }
    }
    let consumed = |target: &[f32], frozen: &[bool]| -> f32 {
        let sum: f32 = target
            .iter()
            .enumerate()
            .map(|(i, t)| if frozen[i] { *t } else { items[i].basis })
            .sum();
        sum + gap_total
    };
    let initial_free = available - consumed(&target, &frozen);
    // Each pass freezes at least one item, so this terminates within `n` rounds.
    while !frozen.iter().all(|f| *f) {
        let mut remaining = available - consumed(&target, &frozen);
        let unfrozen_factor: f32 = items
            .iter()
            .enumerate()
            .filter(|(i, _)| !frozen[*i])
            .map(|(_, item)| factor(item))
            .sum();
        // Factors below one only ever claim their fraction of the original free space.
        if unfrozen_factor < 1.0 {
            let scaled = initial_free * unfrozen_factor;
            if scaled.abs() < remaining.abs() {
                remaining = scaled;
            }
        }
        let weights: Vec<f32> = items
            .iter()
            .enumerate()
            .filter(|(i, _)| !frozen[*i])
            .map(|(_, item)| {
                if growing {
                    item.grow
                } else {
                    item.shrink * item.basis
                }
            })
            .collect();
        let total: f32 = weights.iter().sum();
        if total <= 0.0 || !remaining.is_finite() {
            break;
        }
        let mut violation = 0.0;
        let mut delta = vec![0.0; n];
        let mut next = target.clone();
        let mut w = 0usize;
        for i in 0..n {
            if frozen[i] {
                continue;
            }
            let share = remaining * weights[w] / total;
            w += 1;
            let unclamped = items[i].basis + share;
            let clamped = unclamped
                .max(items[i].min)
                .min(items[i].max.max(items[i].min));
            next[i] = clamped;
            delta[i] = clamped - unclamped;
            violation += delta[i];
        }
        target = next;
        if violation.abs() <= EPS {
            frozen.iter_mut().for_each(|f| *f = true);
        } else if violation > 0.0 {
            for i in 0..n {
                if !frozen[i] && delta[i] > 0.0 {
                    frozen[i] = true;
                }
            }
        } else {
            for i in 0..n {
                if !frozen[i] && delta[i] < 0.0 {
                    frozen[i] = true;
                }
            }
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(basis: f32, grow: f32, shrink: f32, min: f32, max: f32) -> Item {
        Item {
            basis,
            grow,
            shrink,
            min,
            max,
            hypothetical: basis.max(min).min(max),
        }
    }

    /// A deficit is shared in proportion to shrink × basis, not to shrink alone.
    #[test]
    fn a_deficit_is_shared_by_shrink_times_basis() {
        let line = [
            item(100.0, 0.0, 1.0, 0.0, f32::INFINITY),
            item(300.0, 0.0, 1.0, 0.0, f32::INFINITY),
        ];
        let sizes = solve_line(&line, 200.0, 0.0);
        assert!((sizes[0] - 50.0).abs() < 1e-3, "{sizes:?}");
        assert!((sizes[1] - 150.0).abs() < 1e-3, "{sizes:?}");
    }
}
