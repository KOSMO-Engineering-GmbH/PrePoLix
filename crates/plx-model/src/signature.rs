//! Finding the faces, edges and vertices of a geometry again after it changed.
//!
//! Gmsh numbers the entities of a geometry each time it reads one; a changed geometry, such
//! as a reloaded STEP file or the result of a fillet, gets new numbers, while selections and
//! local mesh sizes name the old ones. Each entity is therefore recorded with what it looks
//! like, and the entities of the changed geometry are matched to the old ones by that: the
//! same entity where nothing changed, the nearest one where it moved or grew a little, none
//! where the match is unclear.

use std::collections::BTreeMap;

use plx_mesh::CadEntity;
use serde::{Deserialize, Serialize};

/// The new entity of each old one that was found in a changed geometry.
pub type TagMap = BTreeMap<CadEntity, CadEntity>;

/// What a face, edge or vertex of the geometry looks like, recorded with the geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntitySignature {
    pub entity: CadEntity,
    /// Gmsh's type of the surface or curve, e.g. "Plane", "Cylinder", "Line", "Circle".
    pub kind: String,
    pub centroid: [f64; 3],
    pub min: [f64; 3],
    pub max: [f64; 3],
    /// Area of a face, length of an edge, zero for a vertex.
    pub size: f64,
}

/// How far an entity may have changed, relative to the size of the model, and still be
/// found: a fillet radius or a moved face changes the faces next to it by a few percent.
const TOLERANCE: f64 = 0.3;

/// How much nearer the best match must be than the second, so that two similar entities,
/// like both edges a filleted edge becomes, are not taken for each other.
const CLEAR: f64 = 2.0;

/// How much the relative change of area or length counts against the change of place: a
/// thicker plate has side faces of much more area in the same place.
const SIZE_WEIGHT: f64 = 0.25;

/// Below this the entity is taken as unchanged.
const SAME: f64 = 1e-6;

impl EntitySignature {
    /// How different two entities look: distances of centroid and bounding box relative to
    /// the size of the model, plus the relative difference of size, weighted less. Zero for the same one;
    /// infinite for entities of another dimension or type.
    fn distance(&self, other: &Self, scale: f64) -> f64 {
        if self.entity.dim() != other.entity.dim() || self.kind != other.kind {
            return f64::INFINITY;
        }
        let gap =
            |a: [f64; 3], b: [f64; 3]| ((0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>()).sqrt();
        let place = gap(self.centroid, other.centroid)
            + gap(self.min, other.min)
            + gap(self.max, other.max);
        let larger = self.size.abs().max(other.size.abs());
        let size = if larger > 0.0 {
            (self.size - other.size).abs() / larger
        } else {
            0.0
        };
        place / scale + SIZE_WEIGHT * size
    }
}

/// The diagonal of the bounding box around the entities, at least a tiny length.
fn extent(signatures: &[EntitySignature]) -> f64 {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for signature in signatures {
        for k in 0..3 {
            min[k] = min[k].min(signature.min[k]);
            max[k] = max[k].max(signature.max[k]);
        }
    }
    let diagonal = ((0..3).map(|k| (max[k] - min[k]).powi(2)).sum::<f64>()).sqrt();
    if diagonal.is_finite() && diagonal > 0.0 {
        diagonal
    } else {
        1e-9
    }
}

/// The entities of `new` that the entities of `old` became. An unchanged entity keeps its
/// tag if it has it, else takes the unchanged one of another tag; the others take the
/// nearest of the same dimension and type within [`TOLERANCE`], if it is clearly the
/// nearest. Each new entity is taken once; old ones without a match are left out.
pub fn match_entities(old: &[EntitySignature], new: &[EntitySignature]) -> TagMap {
    let scale = extent(old).max(extent(new));
    let mut map = TagMap::new();
    let mut taken = vec![false; new.len()];
    let by_entity: BTreeMap<CadEntity, usize> = (new.iter().enumerate())
        .map(|(i, s)| (s.entity, i))
        .collect();
    let mut open = Vec::new();
    for signature in old {
        let same_tag = (by_entity.get(&signature.entity))
            .filter(|&&i| !taken[i] && signature.distance(&new[i], scale) < SAME);
        let unchanged = same_tag.copied().or_else(|| {
            (0..new.len()).find(|&i| !taken[i] && signature.distance(&new[i], scale) < SAME)
        });
        match unchanged {
            Some(i) => {
                taken[i] = true;
                map.insert(signature.entity, new[i].entity);
            }
            None => open.push(signature),
        }
    }
    // The candidates of each changed entity, nearest first; the entities nearest to one
    // of theirs choose first.
    let mut candidates: Vec<(&EntitySignature, Vec<(f64, usize)>)> = (open.into_iter())
        .map(|signature| {
            let mut near: Vec<(f64, usize)> = (new.iter().enumerate())
                .filter(|&(i, _)| !taken[i])
                .map(|(i, n)| (signature.distance(n, scale), i))
                .filter(|&(d, _)| d < TOLERANCE)
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            (signature, near)
        })
        .collect();
    candidates.sort_by(|a, b| {
        let first = |near: &[(f64, usize)]| near.first().map_or(f64::INFINITY, |c| c.0);
        first(&a.1).total_cmp(&first(&b.1))
    });
    for (signature, near) in candidates {
        let mut free = near.iter().filter(|&&(_, i)| !taken[i]);
        let Some(&(best, i)) = free.next() else {
            continue;
        };
        if free
            .next()
            .is_some_and(|&(second, _)| second < CLEAR * best)
        {
            continue;
        }
        taken[i] = true;
        map.insert(signature.entity, new[i].entity);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(entity: CadEntity, kind: &str, min: [f64; 3], max: [f64; 3]) -> EntitySignature {
        let centroid = [0, 1, 2].map(|k| 0.5 * (min[k] + max[k]));
        let size = (0..3)
            .map(|k| max[k] - min[k])
            .filter(|d| *d > 0.0)
            .product();
        EntitySignature {
            entity,
            kind: kind.into(),
            centroid,
            min,
            max,
            size,
        }
    }

    /// The six faces of a box 0..x by 0..1 by 0..1, tags 1 to 6 from -x to +z.
    fn box_faces(x: f64) -> Vec<EntitySignature> {
        let face = |tag, min, max| signature(CadEntity::Face(tag), "Plane", min, max);
        vec![
            face(1, [0.0, 0.0, 0.0], [0.0, 1.0, 1.0]),
            face(2, [x, 0.0, 0.0], [x, 1.0, 1.0]),
            face(3, [0.0, 0.0, 0.0], [x, 0.0, 1.0]),
            face(4, [0.0, 1.0, 0.0], [x, 1.0, 1.0]),
            face(5, [0.0, 0.0, 0.0], [x, 1.0, 0.0]),
            face(6, [0.0, 0.0, 1.0], [x, 1.0, 1.0]),
        ]
    }

    #[test]
    fn unchanged_entities_are_found_under_other_tags() {
        let old = box_faces(2.0);
        let mut new = old.clone();
        new.reverse();
        for (i, signature) in new.iter_mut().enumerate() {
            signature.entity = CadEntity::Face(i as i32 + 1);
        }
        let map = match_entities(&old, &new);
        assert_eq!(map.len(), 6);
        for (tag, found) in (1..=6).zip([6, 5, 4, 3, 2, 1]) {
            assert_eq!(map[&CadEntity::Face(tag)], CadEntity::Face(found));
        }
    }

    #[test]
    fn a_longer_box_keeps_its_faces() {
        // The +x face moved by a tenth of the length, the side faces grew with it.
        let map = match_entities(&box_faces(2.0), &box_faces(2.2));
        assert_eq!(map.len(), 6);
        assert!(map.iter().all(|(old, new)| old == new));
        // So with the tags shuffled.
        let mut new = box_faces(2.2);
        new.rotate_left(2);
        for (i, signature) in new.iter_mut().enumerate() {
            signature.entity = CadEntity::Face(10 + i as i32);
        }
        let map = match_entities(&box_faces(2.0), &new);
        assert_eq!(map.len(), 6);
        assert_eq!(map[&CadEntity::Face(3)], CadEntity::Face(10));
        assert_eq!(map[&CadEntity::Face(1)], CadEntity::Face(14));
        assert_eq!(map[&CadEntity::Face(2)], CadEntity::Face(15));
    }

    #[test]
    fn unclear_and_far_entities_are_not_found() {
        // An edge filleted becomes two edges, as near to it as each other.
        let edge = |tag, min, max| signature(CadEntity::Edge(tag), "Line", min, max);
        let old = vec![edge(1, [1.0, 1.0, 0.0], [1.0, 1.0, 1.0])];
        let new = vec![
            edge(1, [0.9, 1.0, 0.0], [0.9, 1.0, 1.0]),
            edge(2, [1.0, 0.9, 0.0], [1.0, 0.9, 1.0]),
        ];
        assert!(match_entities(&old, &new).is_empty());
        // Of another type, or far away.
        let curve = vec![signature(
            CadEntity::Edge(1),
            "Circle",
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
        )];
        assert!(match_entities(&old, &curve).is_empty());
        let far = vec![edge(1, [9.0, 1.0, 0.0], [9.0, 1.0, 1.0])];
        assert!(match_entities(&old, &far).is_empty());
        // A new entity is taken once, by the nearer old one.
        let old = vec![
            edge(1, [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
            edge(2, [1.0, 0.2, 0.0], [1.0, 0.2, 1.0]),
        ];
        let new = vec![edge(7, [1.0, 0.05, 0.0], [1.0, 0.05, 1.0])];
        let map = match_entities(&old, &new);
        assert_eq!(map.len(), 1);
        assert_eq!(map[&CadEntity::Edge(1)], CadEntity::Edge(7));
    }
}
