//! Where one mesh covers another, e.g. a submodel the region of its global model it details.
//!
//! An element counts as covered when a point near one of its corners or its centre lies in
//! the other mesh. The points sit a little inside the element, so an element that only
//! touches the other mesh at a shared face or edge stays, while one reaching into it goes.

use plx_mesh::{ElementShape, FeMesh};

use crate::path::{bounds, in_tetrahedron, in_triangle, tetrahedra, triangles};

/// How far the test points near the corners lie towards the centre of the element.
const TOWARDS_CENTRE: f64 = 0.2;

/// Per element of `mesh`, in the order of [`FeMesh::elements`]: whether `by` covers it.
/// Line elements are never covered; only solid, shell and 2D elements of `by` cover.
pub fn covered_elements(mesh: &FeMesh, by: &FeMesh) -> Vec<bool> {
    let grid = Grid::new(by);
    let coords = mesh.coords();
    let mut nodes = Vec::with_capacity(20);
    mesh.elements()
        .iter()
        .map(|element| {
            let Some(grid) = &grid else {
                return false;
            };
            let corners = corner_count(element.shape);
            if corners < 3 {
                return false;
            }
            nodes.clear();
            nodes.extend((element.nodes.iter().take(corners)).map(|&id| mesh.node_index(id)));
            let Some(corners) = nodes.iter().copied().collect::<Option<Vec<usize>>>() else {
                return false;
            };
            let count = corners.len() as f64;
            let centre =
                [0, 1, 2].map(|k| corners.iter().map(|&n| coords[n][k]).sum::<f64>() / count);
            let near = |&n: &usize| {
                let p = coords[n];
                [0, 1, 2].map(|k| p[k] + TOWARDS_CENTRE * (centre[k] - p[k]))
            };
            std::iter::once(centre)
                .chain(corners.iter().map(near))
                .any(|p| grid.contains(p))
        })
        .collect()
}

/// The elements of a mesh sorted into the cells of a regular grid by their bounding boxes.
struct Grid<'a> {
    mesh: &'a FeMesh,
    low: [f64; 3],
    cell: [f64; 3],
    counts: [usize; 3],
    /// Element indices by cell, x fastest.
    cells: Vec<Vec<usize>>,
    /// Node indices of each element: its tetrahedra and triangles.
    tets: Vec<Vec<[usize; 4]>>,
    tris: Vec<Vec<[usize; 3]>>,
}

impl<'a> Grid<'a> {
    fn new(mesh: &'a FeMesh) -> Option<Self> {
        let coords = mesh.coords();
        let mut boxes = Vec::new();
        let (mut tets, mut tris) = (Vec::new(), Vec::new());
        for element in mesh.elements() {
            let nodes: Option<Vec<usize>> = element
                .nodes
                .iter()
                .map(|&id| mesh.node_index(id))
                .collect();
            let Some(nodes) = nodes else {
                continue;
            };
            let solid = tetrahedra(element.shape, &nodes);
            let flat = solid
                .is_none()
                .then(|| triangles(element.shape, &nodes))
                .flatten();
            if solid.is_none() && flat.is_none() {
                continue;
            }
            let (mut low, mut high) = bounds(coords, &nodes);
            // Shells and 2D elements are flat: their box gets the tolerance of a point off
            // their plane.
            let size = (0..3).map(|k| high[k] - low[k]).fold(0.0, f64::max);
            for k in 0..3 {
                low[k] -= 1e-3 * size;
                high[k] += 1e-3 * size;
            }
            boxes.push((low, high));
            tets.push(solid.unwrap_or_default());
            tris.push(flat.unwrap_or_default());
        }
        let first = boxes.first()?;
        let (mut low, mut high) = *first;
        for (l, h) in &boxes {
            for k in 0..3 {
                low[k] = low[k].min(l[k]);
                high[k] = high[k].max(h[k]);
            }
        }
        // About one element per cell; flat directions get a single cell.
        let per_axis = (boxes.len() as f64).cbrt().ceil().clamp(1.0, 200.0) as usize;
        let extent = [0, 1, 2].map(|k| high[k] - low[k]);
        let largest = extent
            .iter()
            .copied()
            .fold(0.0, f64::max)
            .max(f64::MIN_POSITIVE);
        let counts = extent.map(|e| ((e / largest * per_axis as f64).ceil() as usize).max(1));
        let cell = [0, 1, 2].map(|k| (extent[k] / counts[k] as f64).max(f64::MIN_POSITIVE));
        let mut grid = Self {
            mesh,
            low,
            cell,
            counts,
            cells: vec![Vec::new(); counts[0] * counts[1] * counts[2]],
            tets,
            tris,
        };
        for (index, (l, h)) in boxes.iter().enumerate() {
            let (from, to) = (grid.index(*l), grid.index(*h));
            for z in from[2]..=to[2] {
                for y in from[1]..=to[1] {
                    for x in from[0]..=to[0] {
                        let cell = grid.cell_index([x, y, z]);
                        grid.cells[cell].push(index);
                    }
                }
            }
        }
        Some(grid)
    }

    fn index(&self, p: [f64; 3]) -> [usize; 3] {
        [0, 1, 2].map(|k| {
            let i = ((p[k] - self.low[k]) / self.cell[k]).floor();
            (i.max(0.0) as usize).min(self.counts[k] - 1)
        })
    }

    fn cell_index(&self, [x, y, z]: [usize; 3]) -> usize {
        x + self.counts[0] * (y + self.counts[1] * z)
    }

    fn contains(&self, p: [f64; 3]) -> bool {
        let outside = (0..3).any(|k| {
            p[k] < self.low[k] || p[k] > self.low[k] + self.cell[k] * self.counts[k] as f64
        });
        if outside {
            return false;
        }
        let coords = self.mesh.coords();
        self.cells[self.cell_index(self.index(p))].iter().any(|&e| {
            self.tets[e]
                .iter()
                .any(|t| in_tetrahedron(coords, t, p).is_some())
                || self.tris[e]
                    .iter()
                    .any(|t| in_triangle(coords, t, p).is_some())
        })
    }
}

fn corner_count(shape: ElementShape) -> usize {
    match shape {
        ElementShape::Line2 | ElementShape::Line3 => 2,
        ElementShape::Tri3 | ElementShape::Tri6 => 3,
        ElementShape::Quad4 | ElementShape::Quad8 | ElementShape::Tet4 | ElementShape::Tet10 => 4,
        ElementShape::Wedge6 | ElementShape::Wedge15 => 6,
        ElementShape::Hex8 | ElementShape::Hex20 => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plx_mesh::{Element, Part};

    /// Unit hexahedra along x from `from`, `n` of them, `size` long each.
    fn bar(from: f64, size: f64, n: u32, first_id: u32) -> FeMesh {
        let mut mesh = FeMesh::default();
        for i in 0..=n {
            let x = from + i as f64 * size;
            for (k, [y, z]) in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
                .into_iter()
                .enumerate()
            {
                mesh.set_node(first_id + i * 4 + k as u32, [x, y, z]);
            }
        }
        for i in 0..n {
            let a = first_id + i * 4;
            let b = a + 4;
            mesh.add_element(Element {
                id: i + 1,
                type_name: "C3D8".into(),
                shape: ElementShape::Hex8,
                nodes: vec![a, b, b + 1, a + 1, a + 3, b + 3, b + 2, a + 2],
            })
            .unwrap();
        }
        mesh.parts.push(Part {
            name: "BAR".into(),
            elements: (1..=n).collect(),
        });
        mesh
    }

    #[test]
    fn elements_reaching_into_the_submodel_are_covered() {
        // Global: ten unit elements from 0 to 10. Submodel: finer, from 3.5 to 6.
        let global = bar(0.0, 1.0, 10, 1);
        let submodel = bar(3.5, 0.25, 10, 1);
        let covered = covered_elements(&global, &submodel);
        let expected = [3, 4, 5];
        for (index, covered) in covered.iter().enumerate() {
            assert_eq!(*covered, expected.contains(&index), "element {}", index + 1);
        }
    }

    #[test]
    fn elements_only_touching_the_submodel_stay() {
        // The submodel's ends lie on faces of the global elements.
        let global = bar(0.0, 1.0, 10, 1);
        let submodel = bar(4.0, 0.5, 4, 1);
        let covered = covered_elements(&global, &submodel);
        let shown: Vec<usize> = (0..10).filter(|&i| covered[i]).collect();
        assert_eq!(shown, [4, 5]);
    }

    #[test]
    fn nothing_is_covered_by_an_empty_mesh() {
        let global = bar(0.0, 1.0, 3, 1);
        assert_eq!(covered_elements(&global, &FeMesh::default()), [false; 3]);
    }
}
