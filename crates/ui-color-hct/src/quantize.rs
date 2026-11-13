//! A picture's most populated colours: Wu's quantizer over a 5-bit RGB histogram.
//!
//! Wu 1992, *Efficient Statistical Computation for Optimal Color Quantization*: the
//! histogram is summed into 33³ prefix tables, and a box is split along its longest axis
//! wherever the two halves' variances are smallest, until the requested count is reached.

/// The side of the histogram cube: 32 bins per channel plus the zero plane a prefix needs.
const SIDE: usize = 33;
const CELLS: usize = SIDE * SIDE * SIDE;
/// The channels a cell sums: weight, r, g, b, and r² + g² + b².
const SUMS: usize = 5;

/// One colour a picture is mostly made of, with the pixels it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cluster {
    pub argb: u32,
    pub population: u32,
}

/// The `wanted` most populated colours of `rgba`, heaviest first.
///
/// Transparent pixels are not counted: they are not a colour the picture shows. Fewer
/// clusters than asked for come back when the picture has fewer colours to divide.
pub fn quantize(rgba: &[u8], wanted: usize) -> Vec<Cluster> {
    if wanted == 0 {
        return Vec::new();
    }
    let histogram = Histogram::of(rgba);
    let mut cubes = vec![Cube::whole()];
    while cubes.len() < wanted {
        let mut pick: Option<(usize, f64)> = None;
        for (index, cube) in cubes.iter().enumerate() {
            if histogram.split(cube).is_none() {
                continue;
            }
            let variance = histogram.variance(cube);
            if pick.is_none_or(|(_, best)| variance > best) {
                pick = Some((index, variance));
            }
        }
        let Some((index, _)) = pick else {
            break;
        };
        let (low, high) = histogram
            .split(&cubes[index])
            .expect("the cube was measured as splittable");
        cubes[index] = low;
        cubes.push(high);
    }
    let mut clusters: Vec<Cluster> = exact_colours(&cubes, rgba).into_iter().flatten().collect();
    clusters.sort_by(|a, b| b.population.cmp(&a.population).then(a.argb.cmp(&b.argb)));
    clusters
}

fn index(r: usize, g: usize, b: usize) -> usize {
    (r * SIDE + g) * SIDE + b
}

/// A box of the 5-bit colour cube, both bounds inclusive.
#[derive(Clone, Copy, Debug)]
struct Cube {
    r0: usize,
    r1: usize,
    g0: usize,
    g1: usize,
    b0: usize,
    b1: usize,
}

impl Cube {
    fn whole() -> Self {
        Self {
            r0: 0,
            r1: 31,
            g0: 0,
            g1: 31,
            b0: 0,
            b1: 31,
        }
    }

    fn span(&self, axis: usize) -> usize {
        match axis {
            0 => self.r1 - self.r0,
            1 => self.g1 - self.g0,
            _ => self.b1 - self.b0,
        }
    }

    /// The two boxes a cut `offset` bins above the lower bound leaves.
    fn split(&self, axis: usize, offset: usize) -> (Self, Self) {
        let mut low = *self;
        let mut high = *self;
        match axis {
            0 => {
                low.r1 = self.r0 + offset;
                high.r0 = low.r1 + 1;
            }
            1 => {
                low.g1 = self.g0 + offset;
                high.g0 = low.g1 + 1;
            }
            _ => {
                low.b1 = self.b0 + offset;
                high.b0 = low.b1 + 1;
            }
        }
        (low, high)
    }
}

struct Histogram {
    /// Prefix sums over the cube, five channels a cell.
    sums: Vec<[f64; SUMS]>,
}

impl Histogram {
    fn of(rgba: &[u8]) -> Self {
        let mut sums = vec![[0.0; SUMS]; CELLS];
        for texel in rgba.as_chunks::<4>().0 {
            let [red, green, blue, alpha] = *texel;
            if alpha == 0 {
                continue;
            }
            let (r, g, b) = (
                usize::from(red >> 3),
                usize::from(green >> 3),
                usize::from(blue >> 3),
            );
            let cell = &mut sums[index(r + 1, g + 1, b + 1)];
            cell[0] += 1.0;
            cell[1] += r as f64;
            cell[2] += g as f64;
            cell[3] += b as f64;
            cell[4] += (r * r + g * g + b * b) as f64;
        }
        // Three separable running sums, one per axis: the cube becomes its own prefix.
        for axis in 0..3 {
            for r in 1..SIDE {
                for g in 1..SIDE {
                    for b in 1..SIDE {
                        let cell = index(r, g, b);
                        let back = match axis {
                            0 => index(r - 1, g, b),
                            1 => index(r, g - 1, b),
                            _ => index(r, g, b - 1),
                        };
                        let below = sums[back];
                        for (sum, below) in sums[cell].iter_mut().zip(below) {
                            *sum += below;
                        }
                    }
                }
            }
        }
        Self { sums }
    }

    fn at(&self, r: usize, g: usize, b: usize, channel: usize) -> f64 {
        self.sums[index(r, g, b)][channel]
    }

    /// The summed channel over a box, from its eight prefix corners.
    fn volume(&self, cube: &Cube, channel: usize) -> f64 {
        let (r0, r1) = (cube.r0, cube.r1 + 1);
        let (g0, g1) = (cube.g0, cube.g1 + 1);
        let (b0, b1) = (cube.b0, cube.b1 + 1);
        self.at(r1, g1, b1, channel) - self.at(r1, g1, b0, channel) - self.at(r1, g0, b1, channel)
            + self.at(r1, g0, b0, channel)
            - self.at(r0, g1, b1, channel)
            + self.at(r0, g1, b0, channel)
            + self.at(r0, g0, b1, channel)
            - self.at(r0, g0, b0, channel)
    }

    fn weight(&self, cube: &Cube) -> f64 {
        self.volume(cube, 0)
    }

    /// How far a box's colours spread around their mean.
    fn variance(&self, cube: &Cube) -> f64 {
        let weight = self.weight(cube);
        if weight <= 0.0 {
            return 0.0;
        }
        let (r, g, b) = (
            self.volume(cube, 1),
            self.volume(cube, 2),
            self.volume(cube, 3),
        );
        (self.volume(cube, 4) - (r * r + g * g + b * b) / weight).max(0.0)
    }

    /// The cut along the box's longest axis whose halves spread the least.
    fn split(&self, cube: &Cube) -> Option<(Cube, Cube)> {
        let (span, axis) = [(cube.span(0), 0), (cube.span(1), 1), (cube.span(2), 2)]
            .into_iter()
            .max_by_key(|(span, _)| *span)?;
        let mut best: Option<(usize, f64)> = None;
        for offset in 1..span {
            let (low, high) = cube.split(axis, offset);
            if self.weight(&low) <= 0.0 || self.weight(&high) <= 0.0 {
                continue;
            }
            let spread = self.variance(&low) + self.variance(&high);
            if best.is_none_or(|(_, least)| spread < least) {
                best = Some((offset, spread));
            }
        }
        let (offset, _) = best?;
        Some(cube.split(axis, offset))
    }
}

/// The exact colour of each box's pixels. Wu clusters in five-bit bins; averaging the
/// pixels themselves keeps a flat block's colour its own instead of a bin centre.
fn exact_colours(cubes: &[Cube], rgba: &[u8]) -> Vec<Option<Cluster>> {
    let mut owner = vec![u16::MAX; 32 * 32 * 32];
    for (index, cube) in cubes.iter().enumerate() {
        if index > usize::from(u16::MAX) {
            break;
        }
        for r in cube.r0..=cube.r1 {
            for g in cube.g0..=cube.g1 {
                for b in cube.b0..=cube.b1 {
                    owner[(r << 10) | (g << 5) | b] = index as u16;
                }
            }
        }
    }
    let mut sums = vec![[0u64; 3]; cubes.len()];
    let mut counts = vec![0u32; cubes.len()];
    for texel in rgba.as_chunks::<4>().0 {
        let [red, green, blue, alpha] = *texel;
        if alpha == 0 {
            continue;
        }
        let bin =
            (usize::from(red >> 3) << 10) | (usize::from(green >> 3) << 5) | usize::from(blue >> 3);
        let index = owner[bin];
        if index == u16::MAX {
            continue;
        }
        let index = usize::from(index);
        sums[index][0] += u64::from(red);
        sums[index][1] += u64::from(green);
        sums[index][2] += u64::from(blue);
        counts[index] += 1;
    }
    sums.iter()
        .zip(&counts)
        .map(|(sum, &count)| {
            if count == 0 {
                return None;
            }
            let channel = |index: usize| ((sum[index] as f64 / f64::from(count)).round()) as u8;
            Some(Cluster {
                argb: 0xFF00_0000
                    | (u32::from(channel(0)) << 16)
                    | (u32::from(channel(1)) << 8)
                    | u32::from(channel(2)),
                population: count,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three blocks of known size come back as their colours, heaviest first; asking for
    /// fewer merges the lighter blocks, and a transparent pixel is not a colour at all.
    #[test]
    fn a_picture_quantizes_to_the_colours_it_is_mostly_made_of() {
        let mut rgba = Vec::new();
        for _ in 0..100 {
            rgba.extend_from_slice(&[255, 0, 0, 255]);
        }
        for _ in 0..50 {
            rgba.extend_from_slice(&[0, 255, 0, 255]);
        }
        for _ in 0..25 {
            rgba.extend_from_slice(&[0, 0, 255, 255]);
        }
        rgba.extend_from_slice(&[255, 255, 255, 0]);

        let clusters = quantize(&rgba, 3);
        let found: Vec<(u32, u32)> = clusters
            .iter()
            .map(|cluster| (cluster.argb, cluster.population))
            .collect();
        assert_eq!(
            found,
            vec![(0xFFFF_0000, 100), (0xFF00_FF00, 50), (0xFF00_00FF, 25)]
        );

        // Two clusters merge the pair the split leaves together; which pair is the
        // algorithm's choice, but the coverage is not.
        let merged = quantize(&rgba, 2);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].population, 150);
        assert_eq!(merged[1].population, 25);
        assert_eq!(
            merged.iter().map(|cluster| cluster.population).sum::<u32>(),
            175,
            "every opaque pixel belongs to one cluster"
        );

        // A flat block keeps its own colour: clustering is in five-bit bins, but the
        // colour reported is the pixels' own mean, not the bin's centre.
        let flat = [46u8, 125, 50, 255].repeat(10);
        assert_eq!(quantize(&flat, 4)[0].argb, 0xFF2E_7D32);
    }

    /// The clusters always cover the picture, the requested count is what comes back when
    /// the colours allow it, and the same bytes give the same answer.
    #[test]
    fn the_clusters_cover_every_opaque_pixel() {
        let mut rgba = Vec::new();
        for i in 0..=255u8 {
            rgba.extend_from_slice(&[i, 255 - i, i / 2, 255]);
        }
        for wanted in [1, 2, 8] {
            let clusters = quantize(&rgba, wanted);
            assert!(!clusters.is_empty() && clusters.len() <= wanted);
            assert_eq!(
                clusters
                    .iter()
                    .map(|cluster| cluster.population)
                    .sum::<u32>(),
                256
            );
            assert!(
                clusters
                    .windows(2)
                    .all(|pair| pair[0].population >= pair[1].population),
                "{clusters:?}"
            );
        }
        let eight = quantize(&rgba, 8);
        assert_eq!(eight.len(), 8, "a gradient holds more than eight bins");
        assert_eq!(quantize(&rgba, 8), eight, "the answer is deterministic");
        assert!(quantize(&[], 8).is_empty(), "no pixels, no colours");
    }
}
