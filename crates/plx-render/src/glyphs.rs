//! Arrows along a vector field on the surface of a part, drawn as line lists: a shaft and two
//! head strokes, or a plain stroke for directions without a sign (principal directions).

use std::collections::{HashMap, HashSet};

use glam::DVec3;
use plx_mesh::{PartSkin, face_normal};

use crate::mesh::Vertex;

/// One arrow at a node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrow {
    /// Index into the node coordinates.
    pub node: usize,
    /// Unit direction.
    pub direction: [f64; 3],
    /// Length as a share of the full arrow length, 0 to 1.
    pub scale: f64,
    pub color: [f32; 3],
}

/// The corner nodes on the surface of a part with their averaged face normals.
pub fn surface_nodes(coords: &[[f64; 3]], skin: &PartSkin) -> Vec<(usize, [f64; 3])> {
    let mut normals: HashMap<usize, DVec3> = HashMap::new();
    let mut order = Vec::new();
    for face in &skin.faces {
        let normal = DVec3::from(face_normal(coords, &face.corners));
        for &node in &face.corners {
            let sum = normals.entry(node).or_insert_with(|| {
                order.push(node);
                DVec3::ZERO
            });
            *sum += normal;
        }
    }
    order
        .into_iter()
        .map(|node| {
            let normal = normals[&node].try_normalize().unwrap_or(DVec3::ZERO);
            (node, normal.to_array())
        })
        .collect()
}

/// Area of the surface of a part, summed over its faces.
pub fn surface_area(coords: &[[f64; 3]], skin: &PartSkin) -> f64 {
    let mut area = 0.0;
    for face in &skin.faces {
        let p = |i: usize| DVec3::from(coords[face.corners[i]]);
        for i in 1..face.corners.len().saturating_sub(1) {
            area += 0.5 * (p(i) - p(0)).cross(p(i + 1) - p(0)).length();
        }
    }
    area
}

/// Picks nodes about `spacing` apart: the first one in each cube of that size, so that the
/// arrows spread evenly whatever the mesh density. Nodes facing different ways (by the
/// largest component of their normal) do not compete for a cube, so the two sides of a thin
/// wall both get arrows. `taken` holds the cubes already used, so that several parts share
/// one grid.
pub fn spread_nodes(
    coords: &[[f64; 3]],
    nodes: impl IntoIterator<Item = (usize, [f64; 3])>,
    spacing: f64,
    taken: &mut HashSet<[i64; 4]>,
) -> Vec<usize> {
    let mut picked = Vec::new();
    if spacing.is_nan() || spacing <= 0.0 {
        return picked;
    }
    for (node, normal) in nodes {
        let [x, y, z] = coords[node].map(|c| (c / spacing).floor() as i64);
        let axis = (0..3)
            .max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs()))
            .unwrap_or(0);
        let facing = 2 * axis as i64 + i64::from(normal[axis] < 0.0);
        if taken.insert([x, y, z, facing]) {
            picked.push(node);
        }
    }
    picked
}

/// Line vertices of the arrows at the node positions `coords`, shifted by `-origin`: each
/// starts at its node, or is centred on it without heads for a line field. Arrows are
/// `length` long at full scale. Each is drawn twice, lifted off the surface by a fraction of
/// its length to either side along `normals`: the surface hides neither the copy outside a
/// solid, whichever way its face normals point, nor the arrows on both sides of a shell.
pub fn arrow_lines(
    coords: &[[f64; 3]],
    normals: &HashMap<usize, [f64; 3]>,
    arrows: &[Arrow],
    length: f64,
    line_field: bool,
    origin: DVec3,
) -> Vec<Vertex> {
    let mut lines = Vec::with_capacity(arrows.len() * if line_field { 4 } else { 12 });
    for arrow in arrows {
        let d = DVec3::from(arrow.direction);
        let l = length * arrow.scale;
        if l.is_nan() || l <= 0.0 || !d.is_finite() {
            continue;
        }
        let normal = normals
            .get(&arrow.node)
            .map_or(DVec3::ZERO, |&n| DVec3::from(n));
        let vertex = |p: DVec3| Vertex {
            position: p.as_vec3().to_array(),
            normal: [0.0; 3],
            color: arrow.color,
            scalar: 1.0,
        };
        // Heads open in the plane of the surface where the arrow lies in it.
        let side = d
            .cross(normal)
            .try_normalize()
            .unwrap_or_else(|| d.any_orthonormal_vector());
        for lift in [1.0, -1.0] {
            let at = DVec3::from(coords[arrow.node]) + normal * (lift * 0.02 * length) - origin;
            if line_field {
                lines.extend([vertex(at - d * 0.5 * l), vertex(at + d * 0.5 * l)]);
                continue;
            }
            let tip = at + d * l;
            let back = tip - d * 0.3 * l;
            lines.extend([
                vertex(at),
                vertex(tip),
                vertex(tip),
                vertex(back + side * 0.15 * l),
                vertex(tip),
                vertex(back - side * 0.15 * l),
            ]);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use plx_mesh::SkinFace;

    /// A 2 x 1 rectangle of two unit squares in the x-y plane.
    fn strip() -> (Vec<[f64; 3]>, PartSkin) {
        let coords = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [2.0, 1.0, 0.0],
        ];
        let face = |corners: Vec<usize>| SkinFace {
            element: 0,
            face: 0,
            corners,
            mids: Vec::new(),
            region: 0,
        };
        let skin = PartSkin {
            faces: vec![face(vec![0, 1, 4, 3]), face(vec![1, 2, 5, 4])],
            edges: Vec::new(),
            lines: Vec::new(),
            line_elements: Vec::new(),
        };
        (coords, skin)
    }

    #[test]
    fn nodes_are_spread_by_the_spacing() {
        let (coords, skin) = strip();
        assert_eq!(surface_area(&coords, &skin), 2.0);
        let nodes = surface_nodes(&coords, &skin);
        assert_eq!(nodes.len(), 6);
        assert_eq!(nodes[0].1, [0.0, 0.0, 1.0]);
        let mut taken = HashSet::new();
        let all = spread_nodes(&coords, nodes.iter().copied(), 0.5, &mut taken);
        assert_eq!(all.len(), 6);
        let mut taken = HashSet::new();
        let few = spread_nodes(&coords, nodes.iter().copied(), 1.5, &mut taken);
        assert_eq!(few.len(), 2, "cells [0,1.5) and [1.5,3) in x, one in y");
        // The other side of the wall has cells of its own.
        let back = nodes.iter().map(|&(n, [x, y, z])| (n, [x, y, -z]));
        assert_eq!(spread_nodes(&coords, back, 1.5, &mut taken).len(), 2);
    }

    #[test]
    fn arrows_start_at_their_node_and_strokes_are_centred() {
        let (coords, skin) = strip();
        let normals: HashMap<usize, [f64; 3]> = surface_nodes(&coords, &skin).into_iter().collect();
        let arrow = Arrow {
            node: 4,
            direction: [1.0, 0.0, 0.0],
            scale: 0.5,
            color: [1.0, 0.0, 0.0],
        };
        let arrows = arrow_lines(&coords, &normals, &[arrow], 2.0, false, DVec3::ZERO);
        assert_eq!(arrows.len(), 12, "on either side of the surface");
        assert_eq!(arrows[0].position, [1.0, 1.0, 0.04]);
        assert_eq!(arrows[1].position, [2.0, 1.0, 0.04]);
        assert_eq!(arrows[6].position, [1.0, 1.0, -0.04]);
        // The head opens in the surface.
        assert!((arrows[3].position[2] - 0.04).abs() < 1e-6);
        let strokes = arrow_lines(&coords, &normals, &[arrow], 2.0, true, DVec3::ZERO);
        assert_eq!(strokes.len(), 4);
        assert_eq!(strokes[0].position[0], 0.5);
        let undefined = Arrow {
            direction: [f64::NAN; 3],
            ..arrow
        };
        assert!(arrow_lines(&coords, &normals, &[undefined], 2.0, true, DVec3::ZERO).is_empty());
    }
}
