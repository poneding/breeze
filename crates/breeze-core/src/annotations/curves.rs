//! Document-space curves shared by overlays, selection, and PDF appearances.
use super::{Point, Rect};

#[derive(Clone, Copy, Debug)]
pub struct Cubic {
    pub start: Point,
    pub control1: Point,
    pub control2: Point,
    pub end: Point,
}

impl Cubic {
    pub fn sample(self, t: f32) -> Point {
        let u = 1. - t;
        let coordinate =
            |a, b, c, d| u * u * u * a + 3. * u * u * t * b + 3. * u * t * t * c + t * t * t * d;
        Point::new(
            coordinate(self.start.x, self.control1.x, self.control2.x, self.end.x),
            coordinate(self.start.y, self.control1.y, self.control2.y, self.end.y),
        )
    }
}

/// Smooth quarter waves with matching tangents, ending on both gesture anchors.
pub fn squiggle(first: Point, last: Point, width: f32) -> impl Iterator<Item = Cubic> {
    let dx = last.x - first.x;
    let dy = last.y - first.y;
    let length = dx.hypot(dy);
    // Wider strokes need wider waves to keep the troughs open. Short gestures
    // shrink the amplitude instead of producing a hook beyond their endpoints.
    // Bound generated geometry independently of document size or zoom.
    let half_waves = (length / (width * 3.).max(4.)).round().clamp(1., 512.) as usize;
    let segments = if length > f32::EPSILON {
        half_waves * 2
    } else {
        0
    };
    let amplitude = width.min(length / 4.);
    let map = move |t: f32, offset: f32| {
        let fraction = offset / length;
        Point::new(
            first.x + dx * t - dy * fraction,
            first.y + dy * t + dx * fraction,
        )
    };
    (0..segments).map(move |i| {
        let sine = [0., 1., 0., -1., 0.];
        let cosine = [1., 0., -1., 0., 1.];
        let phase = i % 4;
        let t = i as f32 / segments as f32;
        let step = 1. / segments as f32;
        let slope = amplitude * std::f32::consts::FRAC_PI_2 / 3.;
        Cubic {
            start: map(t, amplitude * sine[phase]),
            control1: map(
                t + step / 3.,
                amplitude * sine[phase] + slope * cosine[phase],
            ),
            control2: map(
                t + step * 2. / 3.,
                amplitude * sine[phase + 1] - slope * cosine[phase + 1],
            ),
            end: if i + 1 == segments {
                last
            } else {
                map(
                    (i + 1) as f32 / segments as f32,
                    amplitude * sine[phase + 1],
                )
            },
        }
    })
}

/// PDF InkList is a polyline; sample the same curve used by its vector appearance.
/// Eight subdivisions per quarter wave bound the deviation below 0.01 line widths.
pub fn squiggle_points(first: Point, last: Point, width: f32) -> Vec<Point> {
    std::iter::once(first)
        .chain(
            squiggle(first, last, width)
                .flat_map(|curve| (1..=8).map(move |i| curve.sample(i as f32 / 8.))),
        )
        .collect()
}

pub fn ellipse(r: Rect) -> [Cubic; 4] {
    let cx = r.x + r.width / 2.;
    let cy = r.y + r.height / 2.;
    let rx = r.width / 2.;
    let ry = r.height / 2.;
    let k = 0.552_284_8;
    let p = |x, y| Point::new(cx + x * rx, cy + y * ry);
    [
        Cubic {
            start: p(1., 0.),
            control1: p(1., k),
            control2: p(k, 1.),
            end: p(0., 1.),
        },
        Cubic {
            start: p(0., 1.),
            control1: p(-k, 1.),
            control2: p(-1., k),
            end: p(-1., 0.),
        },
        Cubic {
            start: p(-1., 0.),
            control1: p(-1., -k),
            control2: p(-k, -1.),
            end: p(0., -1.),
        },
        Cubic {
            start: p(0., -1.),
            control1: p(k, -1.),
            control2: p(1., -k),
            end: p(1., 0.),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waves_keep_their_anchors_envelope_and_tangents() {
        for width in [0.25, 2., 12., 100.] {
            for length in [0.6, 7., 80., 301.] {
                for (dx, dy) in [(1., 0.), (0., 1.), (-0.6, 0.8)] {
                    let first = Point::new(20., -10.);
                    let last = Point::new(first.x + dx * length, first.y + dy * length);
                    let curves: Vec<_> = squiggle(first, last, width).collect();
                    assert_eq!(curves[0].start, first);
                    assert_eq!(curves.last().unwrap().end, last);
                    for pair in curves.windows(2) {
                        assert_eq!(pair[0].end, pair[1].start);
                        let a = pair[0];
                        let b = pair[1];
                        assert!((a.end.x - a.control2.x - b.control1.x + b.start.x).abs() < 0.0001);
                        assert!((a.end.y - a.control2.y - b.control1.y + b.start.y).abs() < 0.0001);
                    }
                    for curve in curves {
                        for i in 0..=32 {
                            let p = curve.sample(i as f32 / 32.);
                            let along = (p.x - first.x) * dx + (p.y - first.y) * dy;
                            let across = (p.y - first.y) * dx - (p.x - first.x) * dy;
                            assert!((-0.0001..=length + 0.0001).contains(&along));
                            assert!(across.abs() <= width.min(length / 4.) + 0.0001);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn editable_polyline_stays_within_one_percent_of_the_curve_width() {
        for width in [0.25, 2., 12., 100.] {
            let first = Point::new(0., 0.);
            let last = Point::new(317., 91.);
            let points = squiggle_points(first, last, width);
            assert_eq!(points[0], first);
            assert_eq!(*points.last().unwrap(), last);
            for (curve, samples) in squiggle(first, last, width).zip(points.windows(9).step_by(8)) {
                for (i, pair) in samples.windows(2).enumerate() {
                    for j in 1..4 {
                        let fraction = j as f32 / 4.;
                        let p = curve.sample((i as f32 + fraction) / 8.);
                        let x = pair[0].x + (pair[1].x - pair[0].x) * fraction;
                        let y = pair[0].y + (pair[1].y - pair[0].y) * fraction;
                        assert!((p.x - x).hypot(p.y - y) < width * 0.01);
                    }
                }
            }
        }
    }

    #[test]
    fn degenerate_and_extreme_gestures_have_bounded_geometry() {
        let origin = Point::new(0., 0.);
        assert_eq!(squiggle(origin, origin, 2.).count(), 0);
        let far = Point::new(9_999_999., 0.);
        assert!(squiggle(origin, far, 0.25).count() <= 1024);
        assert!(squiggle_points(origin, far, 0.25).len() <= 8193);
        assert!(
            squiggle(origin, Point::new(300., 0.), 12.).count()
                < squiggle(origin, Point::new(300., 0.), 2.).count()
        );
    }
}
