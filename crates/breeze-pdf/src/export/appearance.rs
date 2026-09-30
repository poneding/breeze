//! Vector normal appearances use annotation-local bounds and explicit graphics state.
use anyhow::{Result, ensure};
use breeze_core::annotations::{Annotation, Kind, Point, Rect};
use lopdf::{
    Document, Object, ObjectId, Stream,
    content::{Content, Operation},
    dictionary,
};

pub(super) fn array(values: impl IntoIterator<Item = f32>) -> Object {
    Object::Array(values.into_iter().map(Object::Real).collect())
}
pub(super) fn bounds(a: &Annotation) -> Result<Rect> {
    let first = a.points[0];
    if a.kind == Kind::Note {
        return Ok(Rect {
            x: first.x,
            y: first.y - 18.,
            width: 18.,
            height: 18.,
        });
    }
    let mut points = a.points.clone();
    if a.kind.text_markup() {
        points = a
            .quads
            .iter()
            .flat_map(|r| {
                [
                    Point::new(r.x, r.y),
                    Point::new(r.x + r.width, r.y + r.height),
                ]
            })
            .collect();
    }
    let min_x = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let min_y = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_x = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let max_y = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    let pad = if a.kind == Kind::Arrow {
        (a.width * 5.).max(8.) + 2.
    } else {
        a.width * 2. + 2.
    };
    let r = Rect {
        x: min_x - pad,
        y: min_y - pad,
        width: max_x - min_x + 2. * pad,
        height: max_y - min_y + 2. * pad,
    };
    ensure!(
        r.valid() && r.width > 0. && r.height > 0.,
        "Invalid annotation appearance bounds"
    );
    Ok(r)
}
pub(super) fn squiggle(a: &Annotation) -> Vec<Point> {
    let first = a.points[0];
    let last = a.points[a.points.len() - 1];
    let dx = last.x - first.x;
    let dy = last.y - first.y;
    let length = dx.hypot(dy).max(1.);
    let steps = (length / 2.).ceil().clamp(2., 2000.) as usize;
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let offset = (t * length / 8. * std::f32::consts::TAU).sin() * a.width;
            Point::new(
                first.x + dx * t - dy / length * offset,
                first.y + dy * t + dx / length * offset,
            )
        })
        .collect()
}
struct Drawing(Vec<Operation>);
impl Drawing {
    fn op(&mut self, name: &str, values: &[f32]) {
        self.0.push(Operation::new(
            name,
            values.iter().copied().map(Object::Real).collect(),
        ));
    }
    fn path(&mut self, points: &[Point], closed: bool) {
        if let Some(first) = points.first() {
            self.op("m", &[first.x, first.y]);
            for p in &points[1..] {
                self.op("l", &[p.x, p.y]);
            }
            if closed {
                self.op("h", &[]);
            }
            self.op("S", &[]);
        }
    }
    fn rect(&mut self, r: Rect, fill: bool) {
        self.op("re", &[r.x, r.y, r.width, r.height]);
        self.op(if fill { "f" } else { "S" }, &[]);
    }
}
pub(super) fn build(doc: &mut Document, a: &Annotation, r: Rect) -> Result<ObjectId> {
    let opacity = doc.add_object(dictionary! {"Type"=>"ExtGState","CA"=>a.opacity,"ca"=>a.opacity});
    let mut d = Drawing(Vec::new());
    d.op("q", &[]);
    d.op("cm", &[1., 0., 0., 1., -r.x, -r.y]);
    d.0.push(Operation::new(
        "gs",
        vec![Object::Name(b"Opacity".to_vec())],
    ));
    d.op("RG", &a.color);
    d.op("rg", &a.color);
    d.op("w", &[a.width]);
    d.op("J", &[1.]);
    d.op("j", &[1.]);
    let first = a.points[0];
    let last = a.points[a.points.len() - 1];
    let shape = Rect::between(first, last);
    match a.kind {
        Kind::Highlight => {
            for quad in &a.quads {
                d.rect(*quad, true);
            }
        }
        Kind::Underline | Kind::Strikeout => {
            for quad in &a.quads {
                let y = quad.y
                    + if a.kind == Kind::Strikeout {
                        quad.height / 2.
                    } else {
                        0.
                    };
                d.path(
                    &[Point::new(quad.x, y), Point::new(quad.x + quad.width, y)],
                    false,
                );
            }
        }
        Kind::Area => d.rect(shape, true),
        Kind::Rectangle => d.rect(shape, false),
        Kind::Ink => d.path(&a.points, false),
        Kind::Squiggle => d.path(&squiggle(a), false),
        Kind::Line | Kind::Arrow => {
            d.path(&[first, last], false);
            if a.kind == Kind::Arrow {
                let angle = (last.y - first.y).atan2(last.x - first.x);
                let length = (a.width * 5.).max(8.);
                for side in [-0.5, 0.5] {
                    d.path(
                        &[
                            last,
                            Point::new(
                                last.x - length * (angle + side).cos(),
                                last.y - length * (angle + side).sin(),
                            ),
                        ],
                        false,
                    );
                }
            }
        }
        Kind::Ellipse => {
            let cx = shape.x + shape.width / 2.;
            let cy = shape.y + shape.height / 2.;
            let rx = shape.width / 2.;
            let ry = shape.height / 2.;
            let k = 0.552_284_8;
            d.op("m", &[cx + rx, cy]);
            d.op(
                "c",
                &[cx + rx, cy + k * ry, cx + k * rx, cy + ry, cx, cy + ry],
            );
            d.op(
                "c",
                &[cx - k * rx, cy + ry, cx - rx, cy + k * ry, cx - rx, cy],
            );
            d.op(
                "c",
                &[cx - rx, cy - k * ry, cx - k * rx, cy - ry, cx, cy - ry],
            );
            d.op(
                "c",
                &[cx + k * rx, cy - ry, cx + rx, cy - k * ry, cx + rx, cy],
            );
            d.op("h", &[]);
            d.op("S", &[]);
        }
        Kind::Note => {
            // Notes are Unicode sticky notes, not a guessed-font inline FreeText export.
            d.op("rg", &[1., 0.94, 0.65]);
            d.op("RG", &[0.35, 0.29, 0.13]);
            d.op("w", &[0.7]);
            d.op("re", &[r.x + 1., r.y + 1., 16., 16.]);
            d.op("B", &[]);
            for y in [5., 9., 13.] {
                d.path(
                    &[
                        Point::new(r.x + 4., r.y + y),
                        Point::new(r.x + 14., r.y + y),
                    ],
                    false,
                );
            }
        }
    }
    d.op("Q", &[]);
    let bytes = Content { operations: d.0 }.encode()?;
    Ok(doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Form","FormType"=>1,"BBox"=>array([0.,0.,r.width,r.height]),"Resources"=>dictionary!{"ExtGState"=>dictionary!{"Opacity"=>opacity}}},bytes)))
}
