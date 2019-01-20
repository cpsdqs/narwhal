//! Stroke tessellator.
//!
//! Because lyon and libtess2 kept breaking when tessellating weighted stroke
//! outlines, here’s a naïve stroke tessellator implementation that yields
//! decent results.

use cgmath::Vector2;
use std::f32::consts::PI;

/// Stroke tessellator point.
#[derive(Debug, Clone, Copy)]
pub struct TessPoint {
    pub pos: Vector2<f32>,
    pub radius: f32,
}

fn vec_from_angle(angle: f32) -> Vector2<f32> {
    Vector2::new(angle.cos(), angle.sin())
}

/// a mod b with correct handling of negative numbers
fn proper_mod(a: f32, b: f32) -> f32 {
    ((a % b) + b) % b
}

/// Tessellates stroke points and creates arcs (a round join) if an angle exceeds `arc_threshold`.
/// Also adds round line caps.
///
/// Triangles will have counter-clockwise winding, except sometimes around sharp angles.
///
/// # Panics
/// - will panic if `arc_threshold` is `0`
pub fn tessellate(points: &[TessPoint], arc_threshold: f32) -> (Vec<Vector2<f32>>, Vec<u16>) {
    assert!(
        arc_threshold != 0.,
        "Stroke tessellator: arc threshold is 0"
    );

    // The first stroke point and its outgoing angle
    let mut first_point: Option<(TessPoint, f32)> = None;

    // The last stroke point and its incoming angle
    let mut last_point: Option<(TessPoint, f32)> = None;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // index of the previous vertex on the left side
    let mut prev_index_left: Option<u16> = None;

    // index of the previous vertex on the right side
    let mut prev_index_right: Option<u16> = None;

    for i in 0..points.len() {
        let point = points[i];

        // incoming angle
        let in_angle = last_point.map(|(last_point, _)| {
            let diff = point.pos - last_point.pos;
            diff.y.atan2(diff.x)
        });

        // outgoing angle
        let out_angle = if i < points.len() - 1 {
            let diff = points[i + 1].pos - point.pos;
            Some(diff.y.atan2(diff.x))
        } else {
            None
        };

        if first_point.is_none() {
            first_point = Some((point, out_angle.unwrap_or(0.)));
        }

        let outline_angle = in_angle.unwrap_or(out_angle.unwrap_or(0.));

        let outline_left = point.pos + vec_from_angle(outline_angle - PI / 2.) * point.radius;
        let outline_right = point.pos + vec_from_angle(outline_angle + PI / 2.) * point.radius;

        let index_left = vertices.len() as u16;
        vertices.push(outline_left);
        let index_right = vertices.len() as u16;
        vertices.push(outline_right);

        // make triangles if the previous two outline points exist
        if let (Some(prev_left), Some(prev_right)) = (prev_index_left, prev_index_right) {
            // left    1  x
            //  --->   | \
            // right   2--3
            //    prev-^  ^-current
            indices.push(prev_left);
            indices.push(prev_right);
            indices.push(index_right);

            // left    1--3
            //  --->    \ |
            // right   x  2
            //    prev-^  ^-current
            indices.push(prev_left);
            indices.push(index_right);
            indices.push(index_left);
        }

        prev_index_left = Some(index_left);
        prev_index_right = Some(index_right);

        if let (Some(in_angle), Some(out_angle)) = (in_angle, out_angle) {
            // interpolate arc points if the incoming and outgoing angle differ too much
            //
            // left -----x._ARC
            //           .   HERE
            //           .    ,.\
            // --->------p..--    \.
            //             \.       \.
            //               \.       \.
            // right--.        \        \
            //
            // where p is the current point
            //   and x is the left outline point

            // relative out angle in ]-π, π]
            let out_angle_off = proper_mod(out_angle - in_angle - PI, 2. * PI) - PI;

            if out_angle_off.abs() > arc_threshold {
                let steps = (out_angle_off.abs() / arc_threshold).ceil() as usize;
                let step_amount = out_angle_off / steps as f32;
                let arc_on_left = out_angle_off < 0.;

                for step in 0..steps {
                    let ipoint = point.pos
                        + vec_from_angle(
                            in_angle
                                + (step as f32) * step_amount
                                + if arc_on_left { -PI / 2. } else { PI / 2. },
                        ) * point.radius;

                    let index_ipoint = vertices.len() as u16;
                    vertices.push(ipoint);

                    if arc_on_left {
                        if let Some(prev_index_left) = prev_index_left {
                            indices.push(prev_index_left);
                            indices.push(index_right);
                            indices.push(index_ipoint);
                        }

                        prev_index_left = Some(index_ipoint);
                    } else {
                        if let Some(prev_index_right) = prev_index_right {
                            indices.push(index_ipoint);
                            indices.push(index_left);
                            indices.push(prev_index_right);
                        }

                        prev_index_right = Some(index_ipoint);
                    }
                }
            }
        }

        last_point = Some((point, in_angle.unwrap_or(0.)));
    }

    if let (Some((first_point, first_angle)), Some((last_point, last_angle))) =
        (first_point, last_point)
    {
        // stroke caps

        let first_point_index = vertices.len() as u16;
        vertices.push(first_point.pos);

        let last_point_index = vertices.len() as u16;
        vertices.push(last_point.pos);

        let mut angle = -PI / 2.;
        let mut prev_cap_indices = None;

        while angle <= PI / 2. {
            let start_cap_point =
                first_point.pos + vec_from_angle(PI + first_angle + angle) * first_point.radius;
            let end_cap_point =
                last_point.pos + vec_from_angle(last_angle + angle) * last_point.radius;

            let start_cap_index = vertices.len() as u16;
            vertices.push(start_cap_point);

            let end_cap_index = vertices.len() as u16;
            vertices.push(end_cap_point);

            if let Some((prev_start_cap_index, prev_end_cap_index)) = prev_cap_indices {
                indices.push(first_point_index);
                indices.push(start_cap_index);
                indices.push(prev_start_cap_index);

                indices.push(last_point_index);
                indices.push(end_cap_index);
                indices.push(prev_end_cap_index);
            }

            prev_cap_indices = Some((start_cap_index, end_cap_index));

            if angle > PI / 2. - arc_threshold && angle < PI / 2. {
                // ensure that PI / 2 is passed
                angle = PI / 2.;
            } else {
                angle += arc_threshold;
            }
        }
    }

    (vertices, indices)
}
