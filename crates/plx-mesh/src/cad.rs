use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::mesh::{ElementId, FeMesh, NodeId};

/// A face, edge or vertex of the CAD geometry, by Gmsh's tag. Tags stay the same each time
/// the geometry is read, so a selection by CAD entity survives remeshing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CadEntity {
    Face(i32),
    Edge(i32),
    Vertex(i32),
}

impl CadEntity {
    /// The entity of Gmsh's dimension (0 vertex, 1 edge, 2 face) and tag.
    pub fn of(dim: i32, tag: i32) -> Option<Self> {
        match dim {
            0 => Some(Self::Vertex(tag)),
            1 => Some(Self::Edge(tag)),
            2 => Some(Self::Face(tag)),
            _ => None,
        }
    }

    /// Gmsh's dimension of the entity.
    pub fn dim(self) -> i32 {
        match self {
            Self::Vertex(_) => 0,
            Self::Edge(_) => 1,
            Self::Face(_) => 2,
        }
    }

    pub fn tag(self) -> i32 {
        match self {
            Self::Vertex(tag) | Self::Edge(tag) | Self::Face(tag) => tag,
        }
    }

    /// The entity marked as not found in a changed geometry: Gmsh's tags are positive, so
    /// the negative tag names nothing, and the selection holding it shows that something
    /// must be picked again instead of quietly taking another entity.
    pub fn lost(self) -> Self {
        let tag = -self.tag().abs();
        match self {
            Self::Vertex(_) => Self::Vertex(tag),
            Self::Edge(_) => Self::Edge(tag),
            Self::Face(_) => Self::Face(tag),
        }
    }

    /// Whether the entity was not found in a changed geometry, see [`Self::lost`].
    pub fn is_lost(self) -> bool {
        self.tag() <= 0
    }
}

/// Where the CAD entities lie in a mesh generated from the geometry: kept with the mesh and
/// rebuilt with it, so that selections by CAD entity find their nodes and faces again.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CadMap {
    /// Nodes on each CAD face, edge and vertex, with those on its boundary.
    pub nodes: BTreeMap<CadEntity, Vec<NodeId>>,
    /// Element faces on each CAD face: faces of solid elements as (element, face number),
    /// shell elements with face 1.
    pub faces: BTreeMap<i32, Vec<(ElementId, u8)>>,
    /// The mesh segments along each CAD edge, by their end nodes. 2D elements' edges are
    /// found from these, since flipping an element renumbers its edges.
    pub segments: BTreeMap<i32, Vec<[NodeId; 2]>>,
}

impl CadMap {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Whether the mesh has the entity.
    pub fn contains(&self, entity: CadEntity) -> bool {
        self.nodes.contains_key(&entity)
    }

    /// The map with node and element numbers shifted, for a part merged into a mesh.
    pub fn offset(&self, nodes: NodeId, elements: ElementId) -> Self {
        Self {
            nodes: (self.nodes.iter())
                .map(|(&k, v)| (k, v.iter().map(|n| n + nodes).collect()))
                .collect(),
            faces: (self.faces.iter())
                .map(|(&k, v)| (k, v.iter().map(|&(e, f)| (e + elements, f)).collect()))
                .collect(),
            segments: (self.segments.iter())
                .map(|(&k, v)| (k, v.iter().map(|s| s.map(|n| n + nodes)).collect()))
                .collect(),
        }
    }

    /// The map without removed elements and nodes; entities left without any go.
    pub fn without(&self, elements: &BTreeSet<ElementId>, nodes: &BTreeSet<NodeId>) -> Self {
        let mut map = Self::default();
        for (&entity, ids) in &self.nodes {
            let kept: Vec<NodeId> = ids.iter().copied().filter(|n| !nodes.contains(n)).collect();
            if !kept.is_empty() {
                map.nodes.insert(entity, kept);
            }
        }
        for (&face, ids) in &self.faces {
            let kept: Vec<_> = (ids.iter().copied())
                .filter(|(e, _)| !elements.contains(e))
                .collect();
            if !kept.is_empty() {
                map.faces.insert(face, kept);
            }
        }
        for (&edge, segments) in &self.segments {
            let kept: Vec<_> = (segments.iter().copied())
                .filter(|s| !s.iter().any(|n| nodes.contains(n)))
                .collect();
            if !kept.is_empty() {
                map.segments.insert(edge, kept);
            }
        }
        map
    }

    /// Adds the entities of another map; an entity in both gets the union.
    pub fn extend(&mut self, other: Self) {
        for (entity, ids) in other.nodes {
            let list = self.nodes.entry(entity).or_default();
            list.extend(ids);
            list.sort_unstable();
            list.dedup();
        }
        for (face, ids) in other.faces {
            self.faces.entry(face).or_default().extend(ids);
        }
        for (edge, segments) in other.segments {
            self.segments.entry(edge).or_default().extend(segments);
        }
    }

    /// The map after Gmsh numbered the CAD entities anew, `tags` giving the new entity by
    /// the old one; entities without a new one are dropped.
    pub fn renumbered(&self, tags: &BTreeMap<CadEntity, CadEntity>) -> Self {
        let mut map = Self::default();
        for (entity, nodes) in &self.nodes {
            if let Some(&new) = tags.get(entity) {
                map.extend(Self {
                    nodes: BTreeMap::from([(new, nodes.clone())]),
                    ..Self::default()
                });
            }
        }
        for (&face, faces) in &self.faces {
            if let Some(&CadEntity::Face(new)) = tags.get(&CadEntity::Face(face)) {
                map.faces.entry(new).or_default().extend(faces);
            }
        }
        for (&edge, segments) in &self.segments {
            if let Some(&CadEntity::Edge(new)) = tags.get(&CadEntity::Edge(edge)) {
                map.segments.entry(new).or_default().extend(segments);
            }
        }
        map
    }

    /// The CAD face of each element face, the reverse of [`Self::faces`].
    pub fn face_of(&self) -> BTreeMap<(ElementId, u8), i32> {
        (self.faces.iter())
            .flat_map(|(&tag, faces)| faces.iter().map(move |&f| (f, tag)))
            .collect()
    }
}

/// The [`CadMap`] of a display of CAD geometry, whose elements each show the CAD face or
/// edge `entities` gives (element ids count from 1): the triangles of a face lie on it, and
/// the sides of the triangles along the border of a face on the nearest of the CAD edges
/// `face_edges` gives for it, so that what a search finds on the display names its CAD
/// faces and edges. The display's line elements of the edges keep nodes of their own.
pub fn display_cad_map(
    mesh: &FeMesh,
    entities: &[CadEntity],
    face_edges: &BTreeMap<i32, Vec<i32>>,
) -> CadMap {
    let sorted = |[a, b]: [NodeId; 2]| [a.min(b), a.max(b)];
    let entity = |id: ElementId| {
        let index = usize::try_from(id).ok()?.checked_sub(1)?;
        entities.get(index).copied()
    };
    let mut map = CadMap::default();
    let mut lines: BTreeMap<i32, Vec<[[f64; 3]; 2]>> = BTreeMap::new();
    // How often each side of a triangle is used on its face: once along the border.
    let mut sides: BTreeMap<i32, BTreeMap<[NodeId; 2], usize>> = BTreeMap::new();
    for element in mesh.elements() {
        match entity(element.id) {
            Some(CadEntity::Face(tag)) if element.is_shell() => {
                map.faces.entry(tag).or_default().push((element.id, 1));
                let nodes = map.nodes.entry(CadEntity::Face(tag)).or_default();
                nodes.extend(&element.nodes);
                let on_face = sides.entry(tag).or_default();
                for edge in element.shape.edges() {
                    let (Some(&a), Some(&b)) = (
                        edge.corners.first().and_then(|&i| element.nodes.get(i)),
                        edge.corners.get(1).and_then(|&i| element.nodes.get(i)),
                    ) else {
                        continue;
                    };
                    *on_face.entry(sorted([a, b])).or_default() += 1;
                }
            }
            Some(CadEntity::Edge(tag)) => {
                if let (Some(a), Some(b)) = (
                    element.nodes.first().and_then(|&n| mesh.node(n)),
                    element.nodes.last().and_then(|&n| mesh.node(n)),
                ) {
                    lines.entry(tag).or_default().push([a, b]);
                }
            }
            _ => {}
        }
    }
    let all: Vec<i32> = lines.keys().copied().collect();
    for (face, on_face) in sides {
        let candidates = face_edges.get(&face).map_or(&all[..], Vec::as_slice);
        for (side, _) in on_face.into_iter().filter(|&(_, uses)| uses == 1) {
            let (Some(a), Some(b)) = (mesh.node(side[0]), mesh.node(side[1])) else {
                continue;
            };
            let middle = [0, 1, 2].map(|k| 0.5 * (a[k] + b[k]));
            let nearest = (candidates.iter())
                .filter_map(|tag| {
                    let distance = (lines.get(tag)?.iter())
                        .map(|&[p, q]| segment_distance(middle, p, q))
                        .reduce(f64::min)?;
                    Some((distance, *tag))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0));
            if let Some((_, edge)) = nearest {
                map.segments.entry(edge).or_default().push(side);
                map.nodes
                    .entry(CadEntity::Edge(edge))
                    .or_default()
                    .extend(side);
            }
        }
    }
    for nodes in map.nodes.values_mut() {
        nodes.sort_unstable();
        nodes.dedup();
    }
    map
}

/// Distance of a point from the segment p-q.
fn segment_distance(point: [f64; 3], p: [f64; 3], q: [f64; 3]) -> f64 {
    let d = [0, 1, 2].map(|k| q[k] - p[k]);
    let v = [0, 1, 2].map(|k| point[k] - p[k]);
    let length = d.iter().map(|x| x * x).sum::<f64>();
    let t = if length > 0.0 {
        ((0..3).map(|k| v[k] * d[k]).sum::<f64>() / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (0..3)
        .map(|k| (v[k] - t * d[k]).powi(2))
        .sum::<f64>()
        .sqrt()
}

impl FeMesh {
    /// The CAD faces that element faces make up, if they are whole CAD faces and nothing
    /// else, so that a region of them can be kept by geometry. The faces of 2D elements and
    /// the edge faces of shells are edges, which make up CAD edges.
    pub fn whole_cad_faces(&self, faces: &[(ElementId, u8)]) -> Option<Vec<CadEntity>> {
        if faces.is_empty() || self.cad.is_empty() {
            return None;
        }
        let edge = |&(e, face): &(ElementId, u8)| {
            self.element(e)
                .is_some_and(|e| e.is_plane() || (e.is_shell() && face >= 3))
        };
        if faces.iter().all(edge) {
            return self.whole_cad_edges(faces);
        }
        if faces.iter().any(edge) {
            return None;
        }
        let face_of = self.cad.face_of();
        let tags: BTreeSet<i32> = (faces.iter())
            .map(|f| face_of.get(f).copied())
            .collect::<Option<_>>()?;
        let given: BTreeSet<(ElementId, u8)> = faces.iter().copied().collect();
        let covered: BTreeSet<(ElementId, u8)> = (tags.iter())
            .flat_map(|t| self.cad.faces[t].iter().copied())
            .collect();
        (covered == given).then(|| tags.into_iter().map(CadEntity::Face).collect())
    }

    /// The CAD edges that edges of 2D elements or shells make up, if they are whole CAD
    /// edges and nothing else.
    fn whole_cad_edges(&self, faces: &[(ElementId, u8)]) -> Option<Vec<CadEntity>> {
        let sorted = |[a, b]: [NodeId; 2]| [a.min(b), a.max(b)];
        let edge_of: BTreeMap<[NodeId; 2], i32> = (self.cad.segments.iter())
            .flat_map(|(&tag, segments)| segments.iter().map(move |&s| (sorted(s), tag)))
            .collect();
        let mut given = BTreeSet::new();
        for &(element, face) in faces {
            let element = self.element(element)?;
            let edge = *element.faces().get(usize::from(face).checked_sub(1)?)?;
            let ends = [edge.corners.first()?, edge.corners.get(1)?];
            given.insert(sorted(ends.map(|&i| element.nodes[i])));
        }
        let tags: BTreeSet<i32> = (given.iter())
            .map(|s| edge_of.get(s).copied())
            .collect::<Option<_>>()?;
        let covered: BTreeSet<[NodeId; 2]> = (tags.iter())
            .flat_map(|t| self.cad.segments[t].iter().map(|&s| sorted(s)))
            .collect();
        (covered == given).then(|| tags.into_iter().map(CadEntity::Edge).collect())
    }

    /// The CAD faces and edges that element faces lie on, each once: the entities a region
    /// touches anywhere, unlike [`Self::whole_cad_faces`]. Faces off the geometry are left out.
    pub fn cad_entities_touched(&self, faces: &[(ElementId, u8)]) -> Vec<CadEntity> {
        let sorted = |[a, b]: [NodeId; 2]| [a.min(b), a.max(b)];
        let face_of = self.cad.face_of();
        let edge_of: BTreeMap<[NodeId; 2], i32> = (self.cad.segments.iter())
            .flat_map(|(&tag, segments)| segments.iter().map(move |&s| (sorted(s), tag)))
            .collect();
        let mut touched = BTreeSet::new();
        for &(id, face) in faces {
            let Some(element) = self.element(id) else {
                continue;
            };
            if element.is_plane() || (element.is_shell() && face >= 3) {
                let Some(edge) = usize::from(face)
                    .checked_sub(1)
                    .and_then(|i| element.faces().get(i))
                else {
                    continue;
                };
                let ends = (edge.corners.first().zip(edge.corners.get(1)))
                    .and_then(|(&a, &b)| Some([*element.nodes.get(a)?, *element.nodes.get(b)?]));
                if let Some(tag) = ends.and_then(|ends| edge_of.get(&sorted(ends))) {
                    touched.insert(CadEntity::Edge(*tag));
                }
            } else {
                // A shell is on its CAD face with face 1, whichever side is meant.
                let key = if element.is_shell() {
                    (id, 1)
                } else {
                    (id, face)
                };
                if let Some(&tag) = face_of.get(&key) {
                    touched.insert(CadEntity::Face(tag));
                }
            }
        }
        touched.into_iter().collect()
    }

    /// Nodes of CAD entities, in ascending order.
    pub fn cad_nodes(&self, entities: &[CadEntity]) -> Vec<NodeId> {
        let ids: BTreeSet<NodeId> = (entities.iter())
            .filter_map(|e| self.cad.nodes.get(e))
            .flatten()
            .copied()
            .collect();
        ids.into_iter().collect()
    }

    /// Line elements (beams, trusses) along CAD edges, e.g. of a line part meshed from a free
    /// edge, by their end nodes.
    pub fn cad_line_elements(&self, entities: &[CadEntity]) -> Vec<ElementId> {
        let segments: BTreeSet<[NodeId; 2]> = (entities.iter())
            .filter_map(|e| match e {
                CadEntity::Edge(tag) => self.cad.segments.get(tag),
                _ => None,
            })
            .flatten()
            .map(|&[a, b]| [a.min(b), a.max(b)])
            .collect();
        if segments.is_empty() {
            return Vec::new();
        }
        (self.elements().iter())
            .filter(|e| e.shape.family() == crate::ElementFamily::Line)
            .filter(|e| match (e.nodes.first(), e.nodes.last()) {
                (Some(&a), Some(&b)) => segments.contains(&[a.min(b), a.max(b)]),
                _ => false,
            })
            .map(|e| e.id)
            .collect()
    }

    /// Element faces on CAD entities: the faces on CAD faces of solids and shells, and the
    /// edges of 2D elements and shells (faces too, see [`crate::Element::faces`]) along CAD
    /// edges.
    pub fn cad_faces(&self, entities: &[CadEntity]) -> Vec<(ElementId, u8)> {
        let mut faces = BTreeSet::new();
        let mut segments: BTreeSet<[NodeId; 2]> = BTreeSet::new();
        for entity in entities {
            match *entity {
                CadEntity::Face(tag) => {
                    let on_face = self.cad.faces.get(&tag).into_iter().flatten();
                    // A 2D element's face numbers name its edges, not the element itself.
                    faces.extend(
                        on_face.filter(|(e, _)| self.element(*e).is_some_and(|e| !e.is_plane())),
                    );
                }
                CadEntity::Edge(tag) => {
                    let along = self.cad.segments.get(&tag).into_iter().flatten();
                    segments.extend(along.map(|&[a, b]| [a.min(b), a.max(b)]));
                }
                CadEntity::Vertex(_) => {}
            }
        }
        if !segments.is_empty() {
            for element in self.elements() {
                for (number, edge) in element.edge_faces() {
                    let (Some(&a), Some(&b)) = (
                        edge.corners.first().and_then(|&i| element.nodes.get(i)),
                        edge.corners.get(1).and_then(|&i| element.nodes.get(i)),
                    ) else {
                        continue;
                    };
                    if segments.contains(&[a.min(b), a.max(b)]) {
                        faces.insert((element.id, number));
                    }
                }
            }
        }
        faces.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, ElementShape};

    /// Two plane stress quadrilaterals side by side, 1-2-5-4 and 2-3-6-5, meshed from CAD
    /// face 1 with the bottom edge 7 (nodes 1, 2, 3).
    fn strip() -> FeMesh {
        strip_of("CPS4")
    }

    /// The strip with elements of the type, e.g. shells.
    fn strip_of(type_name: &str) -> FeMesh {
        let mut mesh = FeMesh::default();
        for (id, x, y) in [
            (1, 0., 0.),
            (2, 1., 0.),
            (3, 2., 0.),
            (4, 0., 1.),
            (5, 1., 1.),
            (6, 2., 1.),
        ] {
            mesh.set_node(id, [x, y, 0.0]);
        }
        for (id, nodes) in [(1, vec![1, 2, 5, 4]), (2, vec![2, 3, 6, 5])] {
            mesh.add_element(Element {
                id,
                type_name: type_name.into(),
                shape: ElementShape::Quad4,
                nodes,
            })
            .unwrap();
        }
        mesh.cad
            .nodes
            .insert(CadEntity::Face(1), vec![1, 2, 3, 4, 5, 6]);
        mesh.cad.nodes.insert(CadEntity::Edge(7), vec![1, 2, 3]);
        mesh.cad.faces.insert(1, vec![(1, 1), (2, 1)]);
        mesh.cad.segments.insert(7, vec![[1, 2], [2, 3]]);
        mesh
    }

    #[test]
    fn edges_of_2d_elements_are_found_after_flipping() {
        let mut mesh = strip();
        let edge = [CadEntity::Edge(7)];
        assert_eq!(mesh.cad_faces(&edge), [(1, 1), (2, 1)]);
        assert_eq!(mesh.cad_nodes(&edge), [1, 2, 3]);
        // A 2D element's face numbers name edges, so a CAD face gives none.
        assert!(mesh.cad_faces(&[CadEntity::Face(1)]).is_empty());
        // Flipped, the bottom edge runs from node 4 to 1: edge 4 of the elements.
        mesh.flip_surface_elements(|_| true);
        assert_eq!(mesh.cad_faces(&edge), [(1, 4), (2, 4)]);
    }

    #[test]
    fn maps_follow_merged_and_removed_parts() {
        let mesh = strip();
        let moved = mesh.cad.offset(10, 100);
        assert_eq!(moved.faces[&1], [(101, 1), (102, 1)]);
        assert_eq!(moved.segments[&7], [[11, 12], [12, 13]]);
        let kept = mesh
            .cad
            .without(&BTreeSet::from([2]), &BTreeSet::from([3, 6]));
        assert_eq!(kept.faces[&1], [(1, 1)]);
        assert_eq!(kept.segments[&7], [[1, 2]]);
        assert_eq!(kept.nodes[&CadEntity::Edge(7)], [1, 2]);
        let gone = mesh
            .cad
            .without(&BTreeSet::from([1, 2]), &BTreeSet::from([1, 2, 3, 4, 5, 6]));
        assert!(gone.is_empty() && gone.faces.is_empty());
        let tags = BTreeMap::from([
            (CadEntity::Face(1), CadEntity::Face(4)),
            (CadEntity::Edge(7), CadEntity::Edge(2)),
        ]);
        let renumbered = mesh.cad.renumbered(&tags);
        assert_eq!(renumbered.faces[&4], mesh.cad.faces[&1]);
        assert_eq!(renumbered.segments[&2], mesh.cad.segments[&7]);
        assert_eq!(renumbered.nodes.len(), 2);
        assert!(mesh.cad.renumbered(&BTreeMap::new()).is_empty());
        let mut both = kept.clone();
        both.extend(moved);
        assert!(both.contains(CadEntity::Face(1)));
        assert_eq!(both.faces[&1].len(), 3);
    }

    #[test]
    fn whole_cad_faces_are_recognised() {
        // Shells, whose face 1 is the element itself.
        let mut mesh = strip_of("S4");
        mesh.cad.faces.insert(2, vec![(5, 3)]);
        assert_eq!(
            mesh.whole_cad_faces(&[(2, 1), (1, 1)]),
            Some(vec![CadEntity::Face(1)])
        );
        assert_eq!(
            mesh.whole_cad_faces(&[(1, 1)]),
            None,
            "only part of the face"
        );
        assert_eq!(mesh.whole_cad_faces(&[(1, 2)]), None, "not on the geometry");
        assert_eq!(mesh.whole_cad_faces(&[]), None);
    }

    #[test]
    fn whole_cad_edges_of_2d_elements_are_recognised() {
        let mut mesh = strip();
        // Edge 1 of the quadrilaterals is the bottom; as faces of the plane elements they
        // must not be taken for the elements on CAD face 1.
        assert_eq!(
            mesh.whole_cad_faces(&[(1, 1), (2, 1)]),
            Some(vec![CadEntity::Edge(7)])
        );
        assert_eq!(
            mesh.whole_cad_faces(&[(1, 1)]),
            None,
            "only part of the edge"
        );
        assert_eq!(mesh.whole_cad_faces(&[(1, 3)]), None, "not on the geometry");
        mesh.flip_surface_elements(|_| true);
        assert_eq!(
            mesh.whole_cad_faces(&[(1, 4), (2, 4)]),
            Some(vec![CadEntity::Edge(7)])
        );
    }
}
