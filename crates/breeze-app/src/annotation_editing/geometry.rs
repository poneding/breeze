use breeze_core::annotations::{Annotation, Kind, PageGeometry, Point, Rect};

pub(super) fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy).max(0.001)).clamp(0., 1.);
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

fn path(annotation: &Annotation) -> Vec<Point> {
    let Some(&first) = annotation.points.first() else {
        return Vec::new();
    };
    let last = *annotation.points.last().unwrap_or(&first);
    match annotation.kind {
        Kind::Arrow => {
            let angle = (last.y - first.y).atan2(last.x - first.x);
            let length = (annotation.width * 5.).max(8.);
            vec![
                first,
                last,
                Point::new(
                    last.x - length * (angle - 0.5).cos(),
                    last.y - length * (angle - 0.5).sin(),
                ),
                last,
                Point::new(
                    last.x - length * (angle + 0.5).cos(),
                    last.y - length * (angle + 0.5).sin(),
                ),
            ]
        }
        Kind::Squiggle => {
            let dx = last.x - first.x;
            let dy = last.y - first.y;
            let length = dx.hypot(dy).max(1.);
            let steps = (length / 2.).ceil().clamp(2., 2000.) as usize;
            (0..=steps)
                .map(|i| {
                    let t = i as f32 / steps as f32;
                    let wave = (t * length / 8. * std::f32::consts::TAU).sin() * annotation.width;
                    Point::new(
                        first.x + dx * t - dy / length * wave,
                        first.y + dy * t + dx / length * wave,
                    )
                })
                .collect()
        }
        _ => annotation.points.clone(),
    }
}

pub(super) fn hit(annotation: &Annotation, p: Point, tolerance: f32) -> bool {
    let Some(&first) = annotation.points.first() else {
        return false;
    };
    let last = *annotation.points.last().unwrap_or(&first);
    let r = Rect::between(first, last);
    let tolerance = tolerance + annotation.width / 2.;
    match annotation.kind {
        Kind::Note => false, // Native text elements supply their measured hit targets.
        Kind::Highlight => annotation.quads.iter().any(|quad| quad.contains(p)),
        Kind::Underline | Kind::Strikeout => annotation.quads.iter().any(|q| {
            let y = q.y
                + if annotation.kind == Kind::Strikeout {
                    q.height / 2.
                } else {
                    0.
                };
            segment_distance(p, Point::new(q.x, y), Point::new(q.x + q.width, y)) <= tolerance
        }),
        Kind::Area => r.contains(p),
        Kind::Rectangle => {
            let corners = [
                first,
                Point::new(first.x, last.y),
                last,
                Point::new(last.x, first.y),
                first,
            ];
            corners
                .windows(2)
                .any(|edge| segment_distance(p, edge[0], edge[1]) <= tolerance)
        }
        Kind::Ellipse => {
            let rx = r.width / 2.;
            let ry = r.height / 2.;
            if rx <= 0. || ry <= 0. {
                return false;
            }
            let radius = ((p.x - r.x - rx) / rx).hypot((p.y - r.y - ry) / ry);
            (radius - 1.).abs() * rx.min(ry) <= tolerance
        }
        Kind::Line => segment_distance(p, first, last) <= tolerance,
        _ => path(annotation)
            .windows(2)
            .any(|edge| segment_distance(p, edge[0], edge[1]) <= tolerance),
    }
}

pub(super) fn display_bounds(annotation: &Annotation, geometry: PageGeometry) -> Rect {
    let points: Vec<_> = if annotation.kind.text_markup() {
        annotation
            .quads
            .iter()
            .flat_map(|q| {
                [
                    Point::new(q.x, q.y),
                    Point::new(q.x + q.width, q.y + q.height),
                ]
            })
            .collect()
    } else {
        path(annotation)
    };
    let mut min = Point::new(f32::MAX, f32::MAX);
    let mut max = Point::new(f32::MIN, f32::MIN);
    for point in points {
        let p = geometry.display(point);
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    if min.x == f32::MAX {
        return Rect {
            x: 0.,
            y: 0.,
            width: 0.,
            height: 0.,
        };
    }
    let padding = annotation.width / 2. + 2.;
    Rect {
        x: min.x - padding,
        y: min.y - padding,
        width: max.x - min.x + padding * 2.,
        height: max.y - min.y + padding * 2.,
    }
}

pub(super) fn translated(
    annotation: &Annotation,
    delta: Point,
    geometry: PageGeometry,
) -> Annotation {
    let crop = geometry.crop;
    let min_x = annotation
        .points
        .iter()
        .map(|p| p.x)
        .fold(f32::MAX, f32::min);
    let max_x = annotation
        .points
        .iter()
        .map(|p| p.x)
        .fold(f32::MIN, f32::max);
    let min_y = annotation
        .points
        .iter()
        .map(|p| p.y)
        .fold(f32::MAX, f32::min);
    let max_y = annotation
        .points
        .iter()
        .map(|p| p.y)
        .fold(f32::MIN, f32::max);
    let x = delta
        .x
        .max((crop.x - min_x).min(0.))
        .min((crop.x + crop.width - max_x).max(0.));
    let y = delta
        .y
        .max((crop.y - min_y).min(0.))
        .min((crop.y + crop.height - max_y).max(0.));
    let mut edited = annotation.clone();
    for p in &mut edited.points {
        p.x += x;
        p.y += y;
    }
    for q in &mut edited.quads {
        q.x += x;
        q.y += y;
    }
    edited
}
