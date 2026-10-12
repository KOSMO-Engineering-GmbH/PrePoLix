//! Structured meshes of simple shapes: a box of hexahedra and a rectangle of quadrilaterals,
//! made by three nested loops without a mesher. Their sides are node sets and surfaces named
//! after the axis and side they lie on (`X-`, `X+`, `Y-` ...), so that boundary conditions
//! and loads need no selection.

use crate::element::ElementShape;
use crate::mesh::{Element, FeMesh, NodeId, Part, SurfaceDefinition};

/// Names of the sides of a structured mesh, by axis: the side at the smallest coordinate,
/// then the one at the largest.
pub const SIDES: [[&str; 2]; 3] = [["X-", "X+"], ["Y-", "Y+"], ["Z-", "Z+"]];

/// Corners of a hexahedron in CalculiX order, as steps of the grid.
const HEX_CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];
/// Midside nodes of a 20-node hexahedron in CalculiX order, in half steps of the grid.
const HEX_MIDS: [[usize; 3]; 12] = [
    [1, 0, 0],
    [2, 1, 0],
    [1, 2, 0],
    [0, 1, 0],
    [1, 0, 2],
    [2, 1, 2],
    [1, 2, 2],
    [0, 1, 2],
    [0, 0, 1],
    [2, 0, 1],
    [2, 2, 1],
    [0, 2, 1],
];
const QUAD_CORNERS: [[usize; 3]; 4] = [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]];
const QUAD_MIDS: [[usize; 3]; 4] = [[1, 0, 0], [2, 1, 0], [1, 2, 0], [0, 1, 0]];

/// A box from `origin` with edge lengths `size`, divided into `divisions` hexahedra along
/// each axis, of the element type `type_name`; `quadratic` elements have midside nodes
/// (20-node hexahedra). Its elements form the part `part`.
pub fn box_mesh(
    origin: [f64; 3],
    size: [f64; 3],
    divisions: [u32; 3],
    type_name: &str,
    quadratic: bool,
    part: &str,
) -> FeMesh {
    structured(origin, size, divisions, 3, type_name, quadratic, part)
}

/// A rectangle in the XY plane from `origin` with edge lengths `size`, divided into
/// `divisions` quadrilaterals, of the plane element type `type_name` (CPS, CPE or CAX);
/// `quadratic` elements have midside nodes (8-node quadrilaterals).
pub fn rectangle_mesh(
    origin: [f64; 2],
    size: [f64; 2],
    divisions: [u32; 2],
    type_name: &str,
    quadratic: bool,
    part: &str,
) -> FeMesh {
    let origin = [origin[0], origin[1], 0.0];
    let size = [size[0], size[1], 0.0];
    let divisions = [divisions[0], divisions[1], 0];
    structured(origin, size, divisions, 2, type_name, quadratic, part)
}

fn structured(
    origin: [f64; 3],
    size: [f64; 3],
    divisions: [u32; 3],
    dimensions: usize,
    type_name: &str,
    quadratic: bool,
    part: &str,
) -> FeMesh {
    let n: [usize; 3] = std::array::from_fn(|axis| {
        if axis < dimensions {
            divisions[axis].max(1) as usize
        } else {
            0
        }
    });
    // Points of the grid: nodes at the corners, and in between for quadratic elements.
    let step = if quadratic { 2 } else { 1 };
    let points = n.map(|n| n * step + 1);
    let index = |p: [usize; 3]| p[0] + points[0] * (p[1] + points[1] * p[2]);
    let (corners, mids, shape) = match (dimensions, quadratic) {
        (3, false) => (&HEX_CORNERS[..], &[][..], ElementShape::Hex8),
        (3, true) => (&HEX_CORNERS[..], &HEX_MIDS[..], ElementShape::Hex20),
        (_, false) => (&QUAD_CORNERS[..], &[][..], ElementShape::Quad4),
        (_, true) => (&QUAD_CORNERS[..], &QUAD_MIDS[..], ElementShape::Quad8),
    };
    // The grid points of each element's nodes.
    let mut cells = Vec::with_capacity(n[0] * n[1] * n[2].max(1));
    for k in 0..n[2].max(1) {
        for j in 0..n[1] {
            for i in 0..n[0] {
                let base = [i * step, j * step, k * step];
                let at = |offset: &[usize; 3], scale: usize| {
                    index(std::array::from_fn(|a| base[a] + offset[a] * scale))
                };
                let nodes: Vec<usize> = (corners.iter().map(|c| at(c, step)))
                    .chain(mids.iter().map(|m| at(m, 1)))
                    .collect();
                cells.push(nodes);
            }
        }
    }
    // Node ids in the order of the grid, for the points elements use.
    let mut ids: Vec<NodeId> = vec![0; points.iter().product()];
    for &point in cells.iter().flatten() {
        ids[point] = 1;
    }
    let mut mesh = FeMesh::default();
    let mut next: NodeId = 0;
    for (point, id) in ids.iter_mut().enumerate() {
        if *id == 0 {
            continue;
        }
        next += 1;
        *id = next;
        let grid = [
            point % points[0],
            point / points[0] % points[1],
            point / (points[0] * points[1]),
        ];
        let coords = std::array::from_fn(|a| {
            let cells = (points[a] - 1).max(1) as f64;
            origin[a] + size[a] * grid[a] as f64 / cells
        });
        mesh.set_node(next, coords);
    }
    let mut elements = Vec::with_capacity(cells.len());
    for (number, cell) in cells.iter().enumerate() {
        let id = number as u32 + 1;
        let element = Element {
            id,
            type_name: type_name.to_owned(),
            shape,
            nodes: cell.iter().map(|&p| ids[p]).collect(),
        };
        mesh.add_element(element)
            .expect("structured elements have their nodes");
        elements.push(id);
    }
    // Sides: the nodes on them, and the element faces whose corners all lie on them.
    for axis in 0..dimensions {
        for (end, name) in SIDES[axis].iter().enumerate() {
            let on_side = |point: usize| {
                let along = [
                    point % points[0],
                    point / points[0] % points[1],
                    point / (points[0] * points[1]),
                ][axis];
                along == if end == 0 { 0 } else { points[axis] - 1 }
            };
            let nodes: Vec<NodeId> = (ids.iter().enumerate())
                .filter(|(point, id)| **id != 0 && on_side(*point))
                .map(|(_, id)| *id)
                .collect();
            let mut faces = Vec::new();
            for (cell, element) in cells.iter().zip(mesh.elements()) {
                for (face, topology) in element.faces().iter().enumerate() {
                    if topology.corners.iter().all(|&c| on_side(cell[c])) {
                        faces.push((element.id, face as u8 + 1));
                    }
                }
            }
            mesh.node_sets.insert((*name).to_owned(), nodes);
            mesh.surfaces
                .insert((*name).to_owned(), SurfaceDefinition::ElementFaces(faces));
        }
    }
    mesh.parts.push(Part {
        name: part.to_uppercase(),
        elements,
    });
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_has_its_nodes_elements_and_sides() {
        let mesh = box_mesh([0.0; 3], [2.0, 1.0, 1.0], [2, 1, 1], "C3D8", false, "Box");
        assert_eq!(mesh.node_count(), 12);
        assert_eq!(mesh.element_count(), 2);
        assert_eq!(mesh.bounds(), Some(([0.0; 3], [2.0, 1.0, 1.0])));
        assert_eq!(mesh.parts[0].name, "BOX");
        // A positive volume: the corners go round counter-clockwise seen from above.
        let element = &mesh.elements()[0];
        let p = |i: usize| mesh.node(element.nodes[i]).unwrap();
        assert_eq!(p(1), [1.0, 0.0, 0.0]);
        assert_eq!(p(3), [0.0, 1.0, 0.0]);
        assert_eq!(p(4), [0.0, 0.0, 1.0]);
        assert_eq!(mesh.node_sets["X-"].len(), 4);
        assert_eq!(mesh.node_sets["X+"].len(), 4);
        assert_eq!(mesh.node_sets["Z+"].len(), 6);
        let faces = |name: &str| match &mesh.surfaces[name] {
            SurfaceDefinition::ElementFaces(faces) => faces.clone(),
            other => panic!("{other:?}"),
        };
        // CalculiX's faces of a hexahedron: S1 at the bottom, S2 at the top, S3 to S6
        // around it starting at Y-.
        assert_eq!(faces("Z-"), [(1, 1), (2, 1)]);
        assert_eq!(faces("Z+"), [(1, 2), (2, 2)]);
        assert_eq!(faces("Y-"), [(1, 3), (2, 3)]);
        assert_eq!(faces("X+"), [(2, 4)]);
        assert_eq!(faces("Y+"), [(1, 5), (2, 5)]);
        assert_eq!(faces("X-"), [(1, 6)]);
    }

    #[test]
    fn quadratic_boxes_leave_out_face_and_cell_centres() {
        let mesh = box_mesh([1.0, 2.0, 3.0], [1.0; 3], [2, 2, 2], "C3D20R", true, "Box");
        assert_eq!(mesh.element_count(), 8);
        // The corners of the cells and the middles of their edges.
        let corners = 3 * 3 * 3;
        let edges = 3 * (2 * 3 * 3);
        assert_eq!(mesh.node_count(), corners + edges);
        assert_eq!(mesh.bounds(), Some(([1.0, 2.0, 3.0], [2.0, 3.0, 4.0])));
        let element = &mesh.elements()[0];
        // The midside node between corners 1 and 2.
        let mid = mesh.node(element.nodes[8]).unwrap();
        assert_eq!(mid, [1.25, 2.0, 3.0]);
        // The side's nodes include the midside ones: 5 x 5 less the 4 face centres.
        assert_eq!(mesh.node_sets["X-"].len(), 21);
    }

    #[test]
    fn a_rectangle_has_edges_as_sides() {
        let mesh = rectangle_mesh([0.0, 0.0], [3.0, 1.0], [3, 2], "CPS8", true, "Rectangle");
        assert_eq!(mesh.element_count(), 6);
        assert_eq!(mesh.node_count(), 7 * 5 - 6);
        assert!(!mesh.node_sets.contains_key("Z-"));
        assert_eq!(mesh.node_sets["Y+"].len(), 7);
        let SurfaceDefinition::ElementFaces(faces) = &mesh.surfaces["X+"] else {
            panic!()
        };
        // Edge faces of a plane element: S2 from corner 2 to 3.
        assert_eq!(faces, &[(3, 2), (6, 2)]);
        assert!(mesh.coords().iter().all(|c| c[2] == 0.0));
    }
}
