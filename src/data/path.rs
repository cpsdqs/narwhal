use cgmath::Vector2;
use lyon::math::Point;
use lyon::path::builder::{FlatPathBuilder, PathBuilder};
use lyon::path::{self, PathEvent};
use std::mem;

const CURVE_TOLERANCE: f32 = 0.1;

/// Two-dimensional path.
#[derive(Debug, Clone, PartialEq)]
pub struct Path2D(Vec<Path2DCmd>);

/// Path2D commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Path2DCmd {
    /// Jump to a point (analogous to SVG M).
    JumpTo(Vector2<f64>),

    /// Line to a point (analogous to SVG L).
    LineTo(Vector2<f64>),

    /// Quadratic Bézier curve (analogous to SVG Q).
    QuadTo(Vector2<f64>, Vector2<f64>),

    /// Cubic bézier curve (analogous to SVG C).
    CubicTo(Vector2<f64>, Vector2<f64>, Vector2<f64>),

    /// Close the current shape (analogous to SVG Z).
    CloseShape,
}

impl Path2D {
    /// Creates a new empty path.
    pub fn new() -> Path2D {
        Path2D(Vec::new())
    }

    /// Returns a reference to the list of path commands.
    pub fn commands(&self) -> &[Path2DCmd] {
        &self.0
    }

    /// Returns a mutable reference to the path commands.
    pub fn commands_mut(&mut self) -> &mut Vec<Path2DCmd> {
        &mut self.0
    }

    /// Flattens this path to vertices. Each embedded Vec is one contiguous shape separated by jump
    /// commands.
    pub fn flatten_to_verts(&self) -> Vec<Vec<Vector2<f32>>> {
        let mut builder = path::default::Path::builder().flattened(CURVE_TOLERANCE);
        let mut is_first = false;

        for command in &self.0 {
            // ensure M exists before command
            if command.needs_move_if_first() && is_first {
                if let Some(point) = command.point() {
                    builder.move_to(Point::new(point.x as f32, point.y as f32));
                }
            }
            is_first = command.next_will_need_move();

            match command {
                Path2DCmd::JumpTo(v) => builder.move_to(Point::new(v.x as f32, v.y as f32)),
                Path2DCmd::LineTo(v) => builder.line_to(Point::new(v.x as f32, v.y as f32)),
                Path2DCmd::QuadTo(c, p) => {
                    builder.quadratic_bezier_to(
                        Point::new(c.x as f32, c.y as f32),
                        Point::new(p.x as f32, p.y as f32),
                    );
                }
                Path2DCmd::CubicTo(c1, c2, p) => {
                    builder.cubic_bezier_to(
                        Point::new(c1.x as f32, c1.y as f32),
                        Point::new(c2.x as f32, c2.y as f32),
                        Point::new(p.x as f32, p.y as f32),
                    );
                }
                Path2DCmd::CloseShape => builder.close(),
            }
        }

        let path = builder.build();

        let mut groups = Vec::new();
        let mut group = Vec::new();
        let mut group_start = None;

        for event in path.path_iter() {
            match event {
                PathEvent::MoveTo(p) => {
                    group_start = Some(p);
                    let old_group = mem::replace(&mut group, Vec::new());
                    if !old_group.is_empty() {
                        groups.push(old_group);
                    }
                    group.push((p.x, p.y).into());
                }
                PathEvent::LineTo(p) => group.push((p.x, p.y).into()),
                PathEvent::Close => {
                    if let Some(p) = group_start {
                        group.push((p.x, p.y).into());
                        group_start = None;
                        groups.push(mem::replace(&mut group, Vec::new()));
                    }
                }
                _ => unimplemented!(),
            }
        }

        if !group.is_empty() {
            groups.push(group);
        }

        groups
    }
}

impl From<Vec<Path2DCmd>> for Path2D {
    fn from(t: Vec<Path2DCmd>) -> Path2D {
        Path2D(t)
    }
}

impl Path2DCmd {
    fn needs_move_if_first(&self) -> bool {
        match self {
            Path2DCmd::JumpTo(_) | Path2DCmd::CloseShape => false,
            _ => true,
        }
    }

    fn next_will_need_move(&self) -> bool {
        match self {
            Path2DCmd::CloseShape => true,
            _ => false,
        }
    }

    fn point(&self) -> Option<Vector2<f64>> {
        match self {
            Path2DCmd::JumpTo(v)
            | Path2DCmd::LineTo(v)
            | Path2DCmd::QuadTo(v, _)
            | Path2DCmd::CubicTo(_, v, _) => Some(*v),
            _ => None,
        }
    }
}
