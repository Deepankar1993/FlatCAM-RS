//! Drill path optimization (port of `ToolDrilling`'s travel-reduction step).
//!
//! Excellon drill points come out of the parser in file order, which is
//! arbitrary with respect to machine travel. A simple greedy
//! nearest-neighbour reordering visits, at each step, the closest unvisited
//! hole — cutting the total rapid (G00) travel between drills without the cost
//! of a full TSP solve. Not optimal, but cheap and a large improvement over
//! raw file order in practice.

/// Total length of the polyline connecting `points` in order
/// (sum of Euclidean distances between consecutive points).
pub fn path_length(points: &[(f64, f64)]) -> f64 {
    points
        .windows(2)
        .map(|w| dist(w[0], w[1]))
        .sum()
}

/// Greedy nearest-neighbour ordering of `points`.
///
/// Begins at the point closest to `start`, then repeatedly appends the nearest
/// not-yet-visited point. Returns an empty `Vec` for empty input.
pub fn order_nearest(points: &[(f64, f64)], start: (f64, f64)) -> Vec<(f64, f64)> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut remaining: Vec<(f64, f64)> = points.to_vec();
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
        assert_eq!(order_nearest(&empty, (0.0, 0.0)).len(), 0);
        assert!((path_length(&empty)).abs() < 1e-9);

        let one = [(5.0, 5.0)];
        let ordered = order_nearest(&one, (0.0, 0.0));
        assert_eq!(ordered, vec![(5.0, 5.0)]);
        assert!((path_length(&one)).abs() < 1e-9);
    }

    #[test]
    fn ordering_collinear_points_is_no_worse() {
        // Points on the x-axis given in shuffled order.
        let shuffled = [
            (4.0, 0.0),
            (1.0, 0.0),
            (5.0, 0.0),
            (2.0, 0.0),
            (3.0, 0.0),
        ];
        let ordered = order_nearest(&shuffled, (0.0, 0.0));

        // Same set of points, just reordered.
        assert_eq!(ordered.len(), shuffled.len());
        let mut a = ordered.clone();
        let mut b = shuffled.to_vec();
        a.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
        b.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
        assert_eq!(a, b);

        // Greedy from (0,0) yields the monotone order 1..5, which is optimal here.
        let opt = vec![
            (1.0, 0.0),
            (2.0, 0.0),
            (3.0, 0.0),
            (4.0, 0.0),
            (5.0, 0.0),
        ];
        assert_eq!(ordered, opt);

        // And its travel must be no greater than the shuffled order's.
        assert!(path_length(&ordered) <= path_length(&shuffled) + 1e-9);
    }

    #[test]
    fn starts_from_point_closest_to_start() {
        let pts = [(10.0, 10.0), (1.0, 1.0), (5.0, 5.0)];
        let ordered = order_nearest(&pts, (0.0, 0.0));
        assert_eq!(ordered[0], (1.0, 1.0), "first visited is nearest to start");
    }
}
