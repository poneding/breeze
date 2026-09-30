//! Annotation geometry uses unrotated PDF user-space points, never screen pixels.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Rect {
    pub fn between(a: Point, b: Point) -> Self {
        Self {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            width: (a.x - b.x).abs(),
            height: (a.y - b.y).abs(),
        }
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.x + self.width && p.y >= self.y && p.y <= self.y + self.height
    }
    pub fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite() && v.abs() < 1e7)
            && self.width >= 0.
            && self.height >= 0.
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeometry {
    pub crop: Rect,
    pub rotation: i32,
}
impl PageGeometry {
    pub fn size(self) -> (f32, f32) {
        if self.rotation.rem_euclid(180) == 90 {
            (self.crop.height, self.crop.width)
        } else {
            (self.crop.width, self.crop.height)
        }
    }
    /// Display coordinates are top-left-origin points after the page rotation.
    pub fn display(self, p: Point) -> Point {
        let x = p.x - self.crop.x;
        let y = p.y - self.crop.y;
        let w = self.crop.width;
        let h = self.crop.height;
        match self.rotation.rem_euclid(360) {
            90 => Point::new(y, x),
            180 => Point::new(w - x, y),
            270 => Point::new(h - y, w - x),
            _ => Point::new(x, h - y),
        }
    }
    pub fn document(self, p: Point) -> Point {
        let w = self.crop.width;
        let h = self.crop.height;
        let v = match self.rotation.rem_euclid(360) {
            90 => Point::new(p.y, p.x),
            180 => Point::new(w - p.x, p.y),
            270 => Point::new(w - p.y, h - p.x),
            _ => Point::new(p.x, h - p.y),
        };
        Point::new(v.x + self.crop.x, v.y + self.crop.y)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Highlight,
    Area,
    Underline,
    Strikeout,
    Ink,
    Note,
    Ellipse,
    Rectangle,
    Line,
    Squiggle,
    Arrow,
}
impl Kind {
    pub fn text_markup(self) -> bool {
        matches!(self, Self::Highlight | Self::Underline | Self::Strikeout)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub page: usize,
    pub kind: Kind,
    pub points: Vec<Point>,
    pub quads: Vec<Rect>,
    pub color: [f32; 3],
    pub opacity: f32,
    pub width: f32,
    pub text: String,
}
impl Annotation {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.points.is_empty()
            || self.points.len() > 20_000
            || self.quads.len() > 20_000
            || self.text.len() > 16_384
        {
            return Err("Annotation exceeds content limits");
        }
        if !self
            .points
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite() && p.x.abs() < 1e7 && p.y.abs() < 1e7)
            || !self.quads.iter().all(|q| q.valid())
        {
            return Err("Invalid annotation coordinates");
        }
        if !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || !self.width.is_finite()
            || !(0.25..=100.).contains(&self.width)
            || !self
                .color
                .iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(c))
        {
            return Err("Invalid annotation style");
        }
        if self.kind == Kind::Note && self.text.trim().is_empty() {
            return Err("A note needs text");
        }
        if self.kind != Kind::Note && self.points.len() < 2 {
            return Err("A gesture needs two points");
        }
        if self.kind != Kind::Note
            && !self
                .points
                .windows(2)
                .any(|s| (s[0].x - s[1].x).hypot(s[0].y - s[1].y) > 0.5)
        {
            return Err("The gesture is too small");
        }
        if matches!(self.kind, Kind::Area | Kind::Ellipse | Kind::Rectangle) {
            let bounds = Rect::between(self.points[0], self.points[self.points.len() - 1]);
            if bounds.width < 0.5 || bounds.height < 0.5 {
                return Err("The shape has no area");
            }
        }
        if self.kind.text_markup() && self.quads.is_empty() {
            return Err("Text markup requires glyph bounds");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct History {
    pub items: Vec<Annotation>,
    undo: Vec<Vec<Annotation>>,
    redo: Vec<Vec<Annotation>>,
}
impl History {
    pub fn replace(&mut self, items: Vec<Annotation>) {
        self.undo.push(std::mem::replace(&mut self.items, items));
        while self.undo.len() > 50
            || self
                .undo
                .iter()
                .map(|items| {
                    items
                        .iter()
                        .map(|a| a.points.len() * 8 + a.quads.len() * 16 + a.text.len() + 128)
                        .sum::<usize>()
                })
                .sum::<usize>()
                > 8 * 1024 * 1024
        {
            if self.undo.is_empty() {
                break;
            }
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn add(&mut self, item: Annotation) {
        let mut items = self.items.clone();
        items.push(item);
        self.replace(items);
    }
    pub fn undo(&mut self) {
        if let Some(items) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.items, items));
        }
    }
    pub fn redo(&mut self) {
        if let Some(items) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.items, items));
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transforms_roundtrip() {
        for rotation in [0, 90, 180, 270, -90] {
            let g = PageGeometry {
                crop: Rect {
                    x: 20.,
                    y: 30.,
                    width: 200.,
                    height: 300.,
                },
                rotation,
            };
            for p in [
                Point::new(20., 30.),
                Point::new(123., 234.),
                Point::new(220., 330.),
            ] {
                assert_eq!(p, g.document(g.display(p)));
            }
        }
    }
    #[test]
    fn coordinate_axes() {
        let g = PageGeometry {
            crop: Rect {
                x: 20.,
                y: 30.,
                width: 200.,
                height: 300.,
            },
            rotation: 0,
        };
        assert_eq!(g.display(Point::new(20., 330.)), Point::new(0., 0.));
    }
    #[test]
    fn history_roundtrip() {
        let mut h = History::default();
        h.replace(Vec::new());
        assert!(h.can_undo());
        h.undo();
        assert!(h.can_redo());
        h.redo();
        assert!(h.can_undo());
    }
}
