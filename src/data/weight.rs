use crate::data::{Path2D, Path2DCmd};
use crate::util::{Interleaved, InterleavedItem};
use cgmath::{Vector2, Vector3};

/// Stroke weight.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeWeight(Vec<WeightCmd>);

/// Stroke weight commands. X corresponds to the normalized position, Y corresponds to weight,
/// Z corresponds to offset.
#[derive(Debug, Clone, PartialEq)]
pub enum WeightCmd {
    /// Line to a point (analogous to SVG L).
    LineTo(Vector3<f64>),

    /// Quadratic Bézier curve (analogous to SVG Q).
    QuadTo(Vector3<f64>, Vector3<f64>),

    /// Cubic Bézier curve (analogous to SVG C).
    CubicTo(Vector3<f64>, Vector3<f64>, Vector3<f64>),
}

impl WeightCmd {
    /// Remaps points using the given closure.
    pub fn remap_points<F: FnMut(&mut Vector3<f64>)>(&mut self, f: &mut F) {
        match self {
            WeightCmd::LineTo(a) => f(a),
            WeightCmd::QuadTo(a, b) => {
                f(a);
                f(b);
            }
            WeightCmd::CubicTo(a, b, c) => {
                f(a);
                f(b);
                f(c);
            }
        }
    }
}

impl StrokeWeight {
    /// Creates a new empty stroke weight profile.
    pub fn new() -> StrokeWeight {
        StrokeWeight(Vec::new())
    }

    /// Creates a new constant stroke weight profile (i.e. 1 all the way).
    pub fn constant() -> StrokeWeight {
        StrokeWeight(vec![
            WeightCmd::LineTo(Vector3::new(0., 1., 0.)),
            WeightCmd::LineTo(Vector3::new(1., 1., 0.)),
        ])
    }

    /// Returns a reference to the list of stroke weight commands.
    pub fn commands(&self) -> &[WeightCmd] {
        &self.0
    }

    /// Returns a mutable reference to the stroke weight commands.
    pub fn commands_mut(&mut self) -> &mut Vec<WeightCmd> {
        &mut self.0
    }

    fn subpath_weight(&self) -> Path2D {
        let mut path = Vec::new();
        for cmd in &self.0 {
            match cmd {
                WeightCmd::LineTo(a) => path.push(Path2DCmd::LineTo(vec3_xy(*a))),
                WeightCmd::QuadTo(a, b) => path.push(Path2DCmd::QuadTo(vec3_xy(*a), vec3_xy(*b))),
                WeightCmd::CubicTo(a, b, c) => {
                    path.push(Path2DCmd::CubicTo(vec3_xy(*a), vec3_xy(*b), vec3_xy(*c)))
                }
            }
        }
        path.into()
    }

    fn subpath_offset(&self) -> Path2D {
        let mut path = Vec::new();
        for cmd in &self.0 {
            match cmd {
                WeightCmd::LineTo(a) => path.push(Path2DCmd::LineTo(vec3_xz(*a))),
                WeightCmd::QuadTo(a, b) => path.push(Path2DCmd::QuadTo(vec3_xz(*a), vec3_xz(*b))),
                WeightCmd::CubicTo(a, b, c) => {
                    path.push(Path2DCmd::CubicTo(vec3_xz(*a), vec3_xz(*b), vec3_xz(*c)))
                }
            }
        }
        path.into()
    }

    /// Flattens this weight profile to vertices.
    pub fn flatten_to_verts(&self) -> Vec<Vector3<f32>> {
        // FIXME: this approach isn’t that great

        let mut weight_verts = self.subpath_weight().flatten_to_verts();
        let mut offset_verts = self.subpath_offset().flatten_to_verts();
        let weight_verts = if weight_verts.is_empty() {
            vec![Vector2::new(0., 1.)]
        } else {
            weight_verts.remove(0)
        };
        let offset_verts = if offset_verts.is_empty() {
            vec![Vector2::new(0., 1.)]
        } else {
            offset_verts.remove(0)
        };
        let mut last_a = 0;
        let mut last_b = 0;

        fn lerp_slice(slice: &[Vector2<f32>], i: usize, x: f32) -> f32 {
            let j = (i + 1).min(slice.len() - 1);
            let x1 = slice[i].x;
            let x2 = slice[j].x;
            let p = (x - x1) / (x2 - x1);
            let value = slice[i].y + (slice[j].y - slice[i].y) * p;
            if value.is_nan() {
                slice[i].y
            } else {
                value
            }
        }

        let mut prev_x = None;

        Interleaved::new(
            weight_verts.iter(),
            offset_verts.iter(),
            |v| (*v).x,
            |v| (*v).x,
        )
        .map(|item| match item {
            InterleavedItem::A(v, i) => {
                let offset = lerp_slice(&offset_verts, last_b, v.x);
                last_a = i;
                Vector3::new(v.x, v.y, offset)
            }
            InterleavedItem::B(v, i) => {
                let weight = lerp_slice(&weight_verts, last_a, v.x);
                last_b = i;
                Vector3::new(v.x, weight, v.y)
            }
        })
        .filter(|item| {
            // remove duplicates
            // FIXME: this is probably a symptom of a different issue
            if Some(item.x) == prev_x {
                return false;
            }
            prev_x = Some(item.x);
            true
        })
        .collect()
    }

    /// Remaps all points using the given closure.
    pub fn remap_points<F: FnMut(&mut Vector3<f64>)>(&mut self, f: &mut F) {
        self.0.iter_mut().for_each(|c| c.remap_points(f));
    }
}

fn vec3_xy(v: Vector3<f64>) -> Vector2<f64> {
    Vector2::new(v.x, v.y)
}
fn vec3_xz(v: Vector3<f64>) -> Vector2<f64> {
    Vector2::new(v.x, v.z)
}

impl From<Vec<WeightCmd>> for StrokeWeight {
    fn from(v: Vec<WeightCmd>) -> StrokeWeight {
        StrokeWeight(v)
    }
}
