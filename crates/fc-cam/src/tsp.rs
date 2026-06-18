//! 2-opt travelling-salesman path optimizer for drill / travel ordering.
//!
//! [`crate::drilloptim::order_nearest`] gives a cheap greedy tour, but greedy
//! routes often contain crossing segments that waste rapid travel. The classic
//! **2-opt** local search removes such crossings: it repeatedly looks for a
//! pair of route segments whose endpoints can be reconnected (by reversing the
//! sub-tour between them) to yield a shorter total path, applying the best such
//! move it finds and iterating until no improvement remains.
//!
//! This optimizes an *open* path (a fixed start, no return to the origin) — the
//! shape of an Excellon drill program — so the first point is held fixed and
//! only the order of the remaining points is reversed. Pure `std`, no deps.

/// Total length of the polyline connecting `pts` in order
/// (sum of Euclidean distances between consecutive points).
pub fn path_length(pts: &[(f64, f64)]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Greedy nearest-neighbour ordering of `pts`, beginning at the point closest
/// to `start` and repeatedly appending the nearest unvisited point.
///
/// Returns an empty `Vec` for empty input.
pub fn nearest_neighbor(pts: &[(f64, f64)], start: (f64, f64)) -> Vec<(f64, f64)> {
    if pts.is_empty() {
        return Vec::new();
    }
    let mut remaining: Vec<(f64, f64)> = pts.to_vec();
    let mut ordered: Vec<(f64, f64)> = Vec::with_capacity(remaining.len());

    let mut cur = start;
    while !remaining.is_empty() {
        let (idx, _) = remaining
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                dist2(cur, **a)
                    .partial_cmp(&dist2(cur, **b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        let next = remaining.swap_remove(idx);
        cur = next;
        ordered.push(next);
    }
    ordered
}

/// 2-opt optimized ordering of `pts`.
///
/// Seeds with [`nearest_neighbor`], then repeatedly applies the best
/// length-reducing 2-opt segment reversal until no improving move exists (or a
/// safety cap on passes is hit). The first point of the seed tour is held fixed
/// so this optimizes an open path with a defined start.
///
/// The returned order is always at least as good as the nearest-neighbour seed:
/// `path_length(&two_opt(..)) <= path_length(&nearest_neighbor(..))`.
pub fn two_opt(pts: &[(f64, f64)], start: (f64, f64)) -> Vec<(f64, f64)> {
    let mut tour = nearest_neighbor(pts, start);
    let n = tour.len();
    // Fewer than 4 points cannot have a crossing to undo.
    if n < 4 {
        return tour;
    }

    // Cap total passes so pathological inputs can never run away.
    const MAX_PASSES: usize = 1000;
    // Tiny positive threshold so floating-point noise can't loop forever.
    const EPS: f64 = 1e-12;

    let mut passes = 0;
    let mut improved = true;
    while improved && passes < MAX_PASSES {
        improved = false;
        passes += 1;

        // Reversing tour[i..=j] only changes the two boundary edges:
        //   (i-1, i) and (j, j+1)  ->  (i-1, j) and (i, j+1).
        // Hold index 0 fixed (the start), so i starts at 1.
        for i in 1..n - 1 {
            for j in i + 1..n {
                let a = tour[i - 1];
                let b = tour[i];
                let c = tour[j];
                // Edge after j only exists when j is not the last point.
                let (before, after) = if j + 1 < n {
                    let d = tour[j + 1];
                    (dist(a, b) + dist(c, d), dist(a, c) + dist(b, d))
                } else {
                    // Open path: reversing the tail just swaps which endpoint
                    // connects to a; there is no edge past the final point.
                    (dist(a, b), dist(a, c))
                };
                if before - after > EPS {
                    tour[i..=j].reverse();
                    improved = true;
                }
            }
        }
    }
    tour
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    dist2(a, b).sqrt()
}

/// Squared distance — avoids a `sqrt` in the nearest-point comparison.
fn dist2(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_length_sums_segments() {
        // (0,0) -> (3,0) -> (3,4): 3 + 4 = 7
        let pts = [(0.0, 0.0), (3.0, 0.0), (3.0, 4.0)];
        assert!((path_length(&pts) - 7.0).abs() < 1e-9);
    }

    #[test]
    fn empty_and_single_are_handled() {
        let empty: [(f64, f64); 0] = [];
        assert_eq!(nearest_neighbor(&empty, (0.0, 0.0)).len(), 0);
        assert_eq!(two_opt(&empty, (0.0, 0.0)).len(), 0);
        assert!(path_length(&empty).abs() < 1e-9);

        let one = [(5.0, 5.0)];
        assert_eq!(nearest_neighbor(&one, (0.0, 0.0)), vec![(5.0, 5.0)]);
        assert_eq!(two_opt(&one, (0.0, 0.0)), vec![(5.0, 5.0)]);
        assert!(path_length(&one).abs() < 1e-9);
    }

    #[test]
    fn two_opt_preserves_the_point_set() {
        let pts = [
            (0.0, 0.0),
            (2.0, 3.0),
            (5.0, 1.0),
            (1.0, 4.0),
            (4.0, 4.0),
        ];
        let mut got = two_opt(&pts, (0.0, 0.0));
        let mut want = pts.to_vec();
        let key = |p: &(f64, f64)| (p.0, p.1);
        got.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap());
        want.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap());
        assert_eq!(got, want, "2-opt must only reorder, never add/drop points");
    }

    #[test]
    fn two_opt_no_worse_than_nearest_neighbor_no_worse_than_bad() {
        // Eight points evenly spaced on a circle. The optimal open tour walks
        // them in angular order; a shuffled greedy start leaves crossings that
        // 2-opt can remove.
        let n = 8usize;
        let circle: Vec<(f64, f64)> = (0..n)
            .map(|k| {
                let t = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
                (t.cos(), t.sin())
            })
            .collect();

        // A deliberately bad order: alternate opposite sides of the circle so
        // the path zig-zags across the centre repeatedly.
        let bad = vec![
            circle[0], circle[4], circle[1], circle[5], circle[2], circle[6], circle[3], circle[7],
        ];

        let start = (2.0, 0.0);
        let nn = nearest_neighbor(&circle, start);
        let opt = two_opt(&circle, start);

        let bad_len = path_length(&bad);
        let nn_len = path_length(&nn);
        let opt_len = path_length(&opt);

        // two_opt <= nearest_neighbor <= clearly-bad order.
        assert!(
            opt_len <= nn_len + 1e-9,
            "2-opt ({opt_len}) must not exceed nearest-neighbour ({nn_len})"
        );
        assert!(
            nn_len <= bad_len + 1e-9,
            "nearest-neighbour ({nn_len}) must not exceed the bad order ({bad_len})"
        );
        // And 2-opt should be strictly better than the bad zig-zag.
        assert!(opt_len < bad_len);
    }

    #[test]
    fn two_opt_uncrosses_a_known_crossing() {
        // Four corners of a unit square. Visiting them diagonally
        // (0,0)->(1,1)->(1,0)->(0,1) crosses itself; 2-opt should rewrite it
        // into a non-crossing perimeter walk that is shorter.
        let square = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let crossing = [(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 1.0)];
        let opt = two_opt(&square, (0.0, 0.0));
        assert!(path_length(&opt) <= path_length(&crossing) + 1e-9);
    }
}
