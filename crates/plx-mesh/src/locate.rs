//! Where a point lies in a mesh: the element that contains it and the values of that
//! element's shape functions there, the weights that interpolate nodal values at the point.
//! The natural coordinates of the point are found by Newton's method on the element's own
//! mapping, so curved quadratic elements count with their true shape.

use crate::element::{ElementFamily, ElementShape};
use crate::jacobian::{point_derivatives, shape_functions};
use crate::mesh::FeMesh;

/// Tolerance of the natural coordinates: a point on an element's boundary belongs to it.
const INSIDE: f64 = 1e-6;
/// Largest distance of a point from a surface element (a 2D model's plane), relative to the
/// element's size, to lie in it.
const OFF_PLANE: f64 = 1e-3;
/// How far a point may lie outside the mesh, relative to the size of the nearest element, to
/// take the values of the nearest point of that element: a point on a curved surface lies
/// just outside a mesh of straight elements.
pub const SNAP: f64 = 0.05;

/// Where a point lies in a mesh, see [`FeMesh::locate`].
#[derive(Clone, Debug, PartialEq)]
pub enum PointLocation {
    /// In an element, or within [`SNAP`] of one: interpolation weights of nodal values by
    /// index into [`FeMesh::coords`], and how far the point is from the element (0 inside).
    Found {
        weights: Vec<(usize, f64)>,
        distance: f64,
    },
    /// Outside the mesh, at this distance from its nearest node.
    Outside { distance: f64 },
    /// The mesh has no solid or surface elements.
    Empty,
}

impl FeMesh {
    /// Where `point` lies: in a solid element first, then in a surface element (of a 2D
    /// model or a shell). Beams and trusses are left out.
    pub fn locate(&self, point: [f64; 3]) -> PointLocation {
        let coords = self.coords();
        // The nearest element outside, as (distance, weights).
        let mut near: Option<(f64, Vec<(usize, f64)>)> = None;
        let mut surface: Option<Vec<(usize, f64)>> = None;
        let mut any = false;
        let mut xs = [[0.0; 3]; 20];
        let mut indices = [0usize; 20];
        for element in self.elements() {
            let shape = element.shape;
            let family = shape.family();
            if family == ElementFamily::Line {
                continue;
            }
            let count = shape.node_count();
            if element.nodes.len() < count {
                continue;
            }
            let mut complete = true;
            for (k, &node) in element.nodes[..count].iter().enumerate() {
                match self.node_index(node) {
                    Some(index) => {
                        indices[k] = index;
                        xs[k] = coords[index];
                    }
                    None => complete = false,
                }
            }
            if !complete {
                continue;
            }
            any = true;
            let (low, high) = bounds(&xs[..count]);
            let size = (0..3).map(|k| high[k] - low[k]).fold(0.0, f64::max);
            let margin = size * SNAP;
            if (0..3).any(|k| point[k] < low[k] - margin || point[k] > high[k] + margin) {
                continue;
            }
            if family == ElementFamily::Surface && surface.is_some() {
                continue;
            }
            let Some(xi) = natural(shape, &xs[..count], point) else {
                continue;
            };
            let weights = |xi: [f64; 3]| {
                let mut n = [0.0; 20];
                shape_functions(shape, xi, &mut n);
                (0..count).map(|k| (indices[k], n[k])).collect::<Vec<_>>()
            };
            let clamped = clamp(shape, xi);
            let gap = distance(point, at(shape, &xs[..count], clamped));
            let inside = is_inside(shape, xi);
            if family == ElementFamily::Solid && inside {
                return PointLocation::Found {
                    weights: weights(xi),
                    distance: 0.0,
                };
            }
            if family == ElementFamily::Surface && inside && gap <= OFF_PLANE * size {
                surface = Some(weights(clamped));
                continue;
            }
            if gap <= margin && near.as_ref().is_none_or(|(d, ..)| gap < *d) {
                near = Some((gap, weights(clamped)));
            }
        }
        if let Some(weights) = surface {
            return PointLocation::Found {
                weights,
                distance: 0.0,
            };
        }
        if let Some((distance, weights)) = near {
            return PointLocation::Found { weights, distance };
        }
        if !any {
            return PointLocation::Empty;
        }
        let distance = (coords.iter())
            .map(|&c| self::distance(point, c))
            .fold(f64::INFINITY, f64::min);
        PointLocation::Outside { distance }
    }
}

/// The natural coordinates whose image is nearest to `point`: by Newton's method for solids,
/// by Gauss-Newton in the element's surface for surface elements. `None` if the mapping is
/// degenerate.
fn natural(shape: ElementShape, xs: &[[f64; 3]], point: [f64; 3]) -> Option<[f64; 3]> {
    let solid = shape.family() == ElementFamily::Solid;
    let mut xi = center(shape);
    for _ in 0..30 {
        let x = at(shape, xs, xi);
        let r = sub(point, x);
        let derivatives = point_derivatives(shape, xi);
        // Columns of the Jacobian: dx/dxi_k.
        let mut j = [[0.0; 3]; 3];
        for (d, c) in derivatives.iter().zip(xs) {
            for k in 0..3 {
                for i in 0..3 {
                    j[k][i] += d[k] * c[i];
                }
            }
        }
        let step = if solid {
            solve3(j, r)?
        } else {
            // Normal equations of the two tangents.
            let (a, b) = (j[0], j[1]);
            let (aa, ab, bb) = (dot(a, a), dot(a, b), dot(b, b));
            let det = aa * bb - ab * ab;
            if det.abs() <= f64::EPSILON * aa * bb || !det.is_finite() {
                return None;
            }
            let (ra, rb) = (dot(a, r), dot(b, r));
            [(bb * ra - ab * rb) / det, (aa * rb - ab * ra) / det, 0.0]
        };
        for k in 0..3 {
            // Far outside the element the mapping means little; keep the iteration near it.
            xi[k] = (xi[k] + step[k]).clamp(-3.0, 3.0);
        }
        if step.iter().map(|s| s.abs()).fold(0.0, f64::max) < 1e-12 {
            break;
        }
    }
    Some(xi)
}

/// The point the element maps natural coordinates `xi` to.
fn at(shape: ElementShape, xs: &[[f64; 3]], xi: [f64; 3]) -> [f64; 3] {
    let mut n = [0.0; 20];
    shape_functions(shape, xi, &mut n);
    let mut x = [0.0; 3];
    for (ni, c) in n.iter().zip(xs) {
        for i in 0..3 {
            x[i] += ni * c[i];
        }
    }
    x
}

/// The natural coordinates of the element's centre.
fn center(shape: ElementShape) -> [f64; 3] {
    match shape {
        ElementShape::Tet4 | ElementShape::Tet10 => [0.25; 3],
        ElementShape::Tri3 | ElementShape::Tri6 | ElementShape::Wedge6 | ElementShape::Wedge15 => {
            [1.0 / 3.0, 1.0 / 3.0, 0.0]
        }
        _ => [0.0; 3],
    }
}

/// Whether natural coordinates lie in the element.
fn is_inside(shape: ElementShape, xi: [f64; 3]) -> bool {
    let [r, s, t] = xi;
    let triangle = r >= -INSIDE && s >= -INSIDE && r + s <= 1.0 + INSIDE;
    let unit = |x: f64| x.abs() <= 1.0 + INSIDE;
    match shape {
        ElementShape::Tet4 | ElementShape::Tet10 => {
            r >= -INSIDE && s >= -INSIDE && t >= -INSIDE && r + s + t <= 1.0 + INSIDE
        }
        ElementShape::Tri3 | ElementShape::Tri6 => triangle,
        ElementShape::Wedge6 | ElementShape::Wedge15 => triangle && unit(t),
        ElementShape::Quad4 | ElementShape::Quad8 => unit(r) && unit(s),
        ElementShape::Hex8 | ElementShape::Hex20 => unit(r) && unit(s) && unit(t),
        ElementShape::Line2 | ElementShape::Line3 => false,
    }
}

/// Natural coordinates moved into the element: onto its boundary from outside.
fn clamp(shape: ElementShape, xi: [f64; 3]) -> [f64; 3] {
    let simplex = |values: &mut [f64]| {
        for v in values.iter_mut() {
            *v = v.max(0.0);
        }
        let sum: f64 = values.iter().sum();
        if sum > 1.0 {
            for v in values.iter_mut() {
                *v /= sum;
            }
        }
    };
    let mut xi = xi;
    match shape {
        ElementShape::Tet4 | ElementShape::Tet10 => simplex(&mut xi),
        ElementShape::Tri3 | ElementShape::Tri6 => simplex(&mut xi[..2]),
        ElementShape::Wedge6 | ElementShape::Wedge15 => {
            simplex(&mut xi[..2]);
            xi[2] = xi[2].clamp(-1.0, 1.0);
        }
        _ => {
            for v in &mut xi {
                *v = v.clamp(-1.0, 1.0);
            }
        }
    }
    if shape.family() == ElementFamily::Surface {
        xi[2] = 0.0;
    }
    xi
}

/// `x` of `j x = r`, with `j` given by its columns.
fn solve3(j: [[f64; 3]; 3], r: [f64; 3]) -> Option<[f64; 3]> {
    let det = dot(j[0], cross(j[1], j[2]));
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    Some([
        dot(r, cross(j[1], j[2])) / det,
        dot(j[0], cross(r, j[2])) / det,
        dot(j[0], cross(j[1], r)) / det,
    ])
}

fn bounds(xs: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for x in xs {
        for k in 0..3 {
            low[k] = low[k].min(x[k]);
            high[k] = high[k].max(x[k]);
        }
    }
    (low, high)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Element;

    /// One element of `shape` with nodes at `coords`.
    fn single(shape: ElementShape, type_name: &str, coords: &[[f64; 3]]) -> FeMesh {
        let mut mesh = FeMesh::default();
        for (i, c) in coords.iter().enumerate() {
            mesh.set_node(i as u32 + 1, *c);
        }
        mesh.add_element(Element {
            id: 1,
            type_name: type_name.into(),
            shape,
            nodes: (1..=coords.len() as u32).collect(),
        })
        .unwrap();
        mesh
    }

    fn value(mesh: &FeMesh, point: [f64; 3], field: impl Fn([f64; 3]) -> f64) -> Option<f64> {
        match mesh.locate(point) {
            PointLocation::Found { weights, .. } => Some(
                (weights.iter())
                    .map(|&(i, w)| w * field(mesh.coords()[i]))
                    .sum(),
            ),
            _ => None,
        }
    }

    /// A distorted hexahedron: corners of the unit cube moved.
    fn hex() -> Vec<[f64; 3]> {
        vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.1, 0.0],
            [2.2, 1.5, 0.1],
            [-0.1, 1.2, 0.0],
            [0.1, 0.0, 1.0],
            [2.0, -0.1, 1.3],
            [2.1, 1.4, 1.2],
            [0.0, 1.1, 0.9],
        ]
    }

    #[test]
    fn linear_fields_are_reproduced_in_a_distorted_hexahedron() {
        let mesh = single(ElementShape::Hex8, "C3D8", &hex());
        let field = |p: [f64; 3]| 3.0 * p[0] - p[1] + 2.0 * p[2] + 1.0;
        for point in [[1.0, 0.6, 0.5], [0.3, 0.2, 0.1], [1.9, 1.2, 1.0]] {
            let v = value(&mesh, point, field).unwrap();
            assert!((v - field(point)).abs() < 1e-9, "{point:?}: {v}");
        }
        // A node is found with its own value.
        let v = value(&mesh, hex()[6], field).unwrap();
        assert!((v - field(hex()[6])).abs() < 1e-9);
    }

    #[test]
    fn quadratic_fields_are_reproduced_in_a_quadratic_tetrahedron() {
        let corners = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let mut coords = corners.to_vec();
        for [a, b] in [[0, 1], [1, 2], [2, 0], [0, 3], [1, 3], [2, 3]] {
            coords.push(std::array::from_fn(|k| {
                0.5 * (corners[a][k] + corners[b][k])
            }));
        }
        let mesh = single(ElementShape::Tet10, "C3D10", &coords);
        let field = |p: [f64; 3]| p[0] * p[0] + p[1] * p[2] - p[2];
        let point = [0.2, 0.3, 0.1];
        let v = value(&mesh, point, field).unwrap();
        assert!((v - field(point)).abs() < 1e-9, "{v}");
    }

    #[test]
    fn plane_elements_and_points_outside() {
        let coords = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let mesh = single(ElementShape::Quad4, "CPS4", &coords);
        let field = |p: [f64; 3]| p[0] + 10.0 * p[1];
        let v = value(&mesh, [1.5, 0.25, 0.0], field).unwrap();
        assert!((v - 4.0).abs() < 1e-9);
        // Just outside the edge: the value at the edge.
        let v = value(&mesh, [2.05, 0.5, 0.0], field).unwrap();
        assert!((v - 7.0).abs() < 1e-9);
        // Out of the plane or far away: no value, with the distance to the nearest node.
        assert!(value(&mesh, [1.0, 0.5, 0.5], field).is_none());
        match mesh.locate([5.0, 1.0, 0.0]) {
            PointLocation::Outside { distance } => assert!((distance - 3.0).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
        assert_eq!(FeMesh::default().locate([0.0; 3]), PointLocation::Empty);
    }
}
