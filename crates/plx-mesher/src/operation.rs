//! CAD operations on the geometry with OpenCASCADE through Gmsh: fillets and chamfers of
//! edges and planar faces moved, see [`plx_model::CadOperation`].

use std::collections::BTreeSet;

use plx_mesh::CadEntity;
use plx_model::{CadOperationKind, EntitySignature, Geometry, match_entities};

use crate::gmsh::{Boolean, Gmsh, GmshError, with_gmsh};
use crate::{Shape, TempFile, default_parts, parts, signatures};

/// The geometry with the operation applied to the entities, which are those of `input` as
/// Gmsh reads its BREP; its length is in metres like the geometry. The parts keep their
/// names and the mesh setup stays; the faces and edges get Gmsh's new tags, with their
/// signatures to find them again. Fails with OpenCASCADE's reason, or when the result is
/// not a valid change of the solids.
pub fn apply_cad_operation(
    input: &Geometry,
    kind: CadOperationKind,
    entities: &[CadEntity],
    value: f64,
) -> Result<Geometry, GmshError> {
    let fail = |message: String| GmshError::Other(message);
    let operation = plx_model::CadOperation {
        kind,
        entities: Vec::new(),
        value,
    };
    if let Some(problem) = operation.problem() {
        return Err(fail(capitalised(&problem)));
    }
    let file = TempFile::with_contents("brep", &input.brep)?;
    let result = TempFile::new("brep");
    let names = with_gmsh(|gmsh| {
        gmsh.import_shapes(&file.0)?;
        let before = named_places(gmsh, &parts(gmsh, input)?)?;
        let volumes_before = gmsh.entities(3)?.len();
        let volume_before = total_volume(gmsh)?;
        let made = match kind {
            CadOperationKind::Fillet | CadOperationKind::Chamfer => {
                round_edges(gmsh, kind, entities, value)?
            }
            CadOperationKind::PushPull => push_pull(gmsh, entities, value)?,
        };
        if made.is_empty() {
            return Err(fail(format!(
                "OpenCASCADE made no {}: the {} may be too large for the faces next to the \
                 {}",
                kind.label().to_lowercase(),
                kind.value_label().to_lowercase(),
                kind.picked_noun()
            )));
        }
        // OpenCASCADE may give a broken solid instead of failing: the result must have as
        // many solids, and a volume a little different from before.
        let volumes_after = gmsh.entities(3)?.len();
        let volume_after = total_volume(gmsh)?;
        // A face moved may change the volume much; a fillet or a chamfer may not.
        let changed_much = kind != CadOperationKind::PushPull
            && (volume_after - volume_before).abs() > 0.5 * volume_before;
        if volumes_after != volumes_before
            || !(volume_after.is_finite() && volume_after > 0.0)
            || changed_much
        {
            return Err(fail(format!(
                "The {} gave no valid solid: the {} may be too large",
                kind.label().to_lowercase(),
                kind.value_label().to_lowercase()
            )));
        }
        gmsh.write(&result.0)?;
        gmsh.clear()?;
        gmsh.import_shapes(&result.0)?;
        // Each part keeps the name of the one it was made from: the nearest of the same
        // dimension.
        let after = default_parts(gmsh)?;
        let mut names = Vec::with_capacity(after.len());
        for (shape, name) in after {
            let place = place_of(gmsh, &shape)?;
            let nearest = (before.iter())
                .filter(|(d, _, _)| *d == shape.dim)
                .min_by(|a, b| distance(a.2, place).total_cmp(&distance(b.2, place)))
                .map(|(_, n, _)| n.clone());
            names.push(nearest.unwrap_or(name));
        }
        Ok((names, signatures(gmsh)?))
    })?;
    let brep = std::fs::read_to_string(&result.0)
        .map_err(|e| fail(format!("{}: {e}", result.0.display())))?;
    let (part_names, signatures) = names;
    Ok(Geometry {
        brep,
        part_names,
        signatures,
        ..input.clone()
    })
}

/// Rounds or bevels the edges among `entities`; returns the new solids.
fn round_edges(
    gmsh: &Gmsh,
    kind: CadOperationKind,
    entities: &[CadEntity],
    value: f64,
) -> Result<Vec<(i32, i32)>, GmshError> {
    let fail = |message: String| GmshError::Other(message);
    let edges: Vec<i32> = (entities.iter())
        .filter_map(|e| match e {
            CadEntity::Edge(tag) => Some(*tag),
            _ => None,
        })
        .collect();
    let known: BTreeSet<i32> = gmsh.entities(1)?.into_iter().collect();
    if let Some(edge) = edges.iter().find(|e| !known.contains(e)) {
        return Err(fail(format!("The geometry has no edge {edge}")));
    }
    if edges.is_empty() {
        return Err(fail("No edges picked".into()));
    }
    // The solids the edges bound, through their faces.
    let mut volumes = BTreeSet::new();
    let mut faces = Vec::with_capacity(edges.len());
    for &edge in &edges {
        let (up, _) = gmsh.adjacencies(1, edge)?;
        faces.push(up.first().copied().unwrap_or(0));
        for face in up {
            volumes.extend(gmsh.adjacencies(2, face)?.0);
        }
    }
    if volumes.is_empty() {
        return Err(fail(format!(
            "A {} needs edges of solids",
            kind.label().to_lowercase()
        )));
    }
    let volumes: Vec<i32> = volumes.into_iter().collect();
    let made = match kind {
        CadOperationKind::Chamfer => gmsh.chamfer(&volumes, &edges, &faces, value),
        _ => gmsh.fillet(&volumes, &edges, value),
    };
    made.map_err(|e| {
        fail(format!(
            "OpenCASCADE could not make the {}: {e}",
            kind.label().to_lowercase()
        ))
    })
}

/// Moves the planar faces among `entities` along their outward normal by `offset`, inwards
/// when it is negative: a prism on each face is fused with its solid or cut from it, see
/// [`CadOperationKind::PushPull`]. Returns the solids made.
fn push_pull(
    gmsh: &Gmsh,
    entities: &[CadEntity],
    offset: f64,
) -> Result<Vec<(i32, i32)>, GmshError> {
    let fail = |message: String| GmshError::Other(message);
    let faces: Vec<i32> = (entities.iter())
        .filter_map(|e| match e {
            CadEntity::Face(tag) => Some(*tag),
            _ => None,
        })
        .collect();
    if faces.is_empty() {
        return Err(fail("No faces picked".into()));
    }
    let known: BTreeSet<i32> = gmsh.entities(2)?.into_iter().collect();
    if let Some(face) = faces.iter().find(|f| !known.contains(f)) {
        return Err(fail(format!("The geometry has no face {face}")));
    }
    // One face after the other, so that faces next to each other both move as a whole:
    // the faces still to move are found again in each result by their signatures.
    let mut pending: Vec<EntitySignature> = (signatures(gmsh)?.into_iter())
        .filter(|s| faces.iter().any(|&f| s.entity == CadEntity::Face(f)))
        .collect();
    let mut made = Vec::new();
    while !pending.is_empty() {
        let CadEntity::Face(face) = pending.remove(0).entity else {
            continue;
        };
        made = move_face(gmsh, face, offset)?;
        if pending.is_empty() {
            break;
        }
        let now = signatures(gmsh)?;
        let map = match_entities(&pending, &now);
        pending = (pending.iter())
            .map(|s| {
                (map.get(&s.entity))
                    .and_then(|new| now.iter().find(|n| n.entity == *new))
                    .cloned()
                    .ok_or_else(|| fail("A picked face was lost by moving another".into()))
            })
            .collect::<Result<_, _>>()?;
    }
    Ok(made)
}

/// Moves one planar face, see [`push_pull`].
fn move_face(gmsh: &Gmsh, face: i32, offset: f64) -> Result<Vec<(i32, i32)>, GmshError> {
    let fail = |message: String| GmshError::Other(message);
    if gmsh.entity_type(2, face)? != "Plane" {
        return Err(fail("Push/Pull moves planar faces only".into()));
    }
    let (solids, _) = gmsh.adjacencies(2, face)?;
    let [volume] = solids[..] else {
        return Err(fail(
            "Push/Pull needs faces on the outside of one solid".into(),
        ));
    };
    let normal = outward_normal(gmsh, face, volume)?;
    let prisms: Vec<(i32, i32)> = (gmsh.extrude(&[(2, face)], normal.map(|n| n * offset))?)
        .into_iter()
        .filter(|&(dim, _)| dim == 3)
        .collect();
    let operation = if offset > 0.0 {
        Boolean::Fuse
    } else {
        Boolean::Cut
    };
    gmsh.boolean(operation, &[(3, volume)], &prisms, true)
        .map_err(|e| fail(format!("OpenCASCADE could not move the face: {e}")))
}

/// The unit normal of a planar face pointing out of the solid it bounds.
fn outward_normal(gmsh: &Gmsh, face: i32, volume: i32) -> Result<[f64; 3], GmshError> {
    let (min, max) = gmsh.parameter_bounds(2, face)?;
    let [u0, v0, u1, v1] = [min[0], min[1], max[0], max[1]];
    let corners = gmsh.values(2, face, &[u0, v0, u1, v0, u0, v1])?;
    let [a, b, c] = [corners[0], corners[1], corners[2]];
    let (ab, ac) = (
        [0, 1, 2].map(|k| b[k] - a[k]),
        [0, 1, 2].map(|k| c[k] - a[k]),
    );
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let length = (n.iter().map(|x| x * x).sum::<f64>()).sqrt();
    if length.is_nan() || length <= 0.0 {
        return Err(GmshError::Other(format!("Face {face} has no normal")));
    }
    let n = n.map(|x| x / length);
    // Which side the solid is on, a little off a point of the face.
    let (lo, hi) = gmsh.entity_bounding_box(2, face)?;
    let step = 1e-4 * distance(lo, hi).max(1e-9);
    let centre = gmsh.center_of_mass(2, face)?;
    let point = gmsh.closest_point(2, face, centre)?;
    let probe = |sign: f64| [0, 1, 2].map(|k| point[k] + sign * step * n[k]);
    if gmsh.contains_point(3, volume, probe(1.0))? {
        Ok(n.map(|x| -x))
    } else if gmsh.contains_point(3, volume, probe(-1.0))? {
        Ok(n)
    } else {
        Err(GmshError::Other(format!(
            "The outside of face {face} could not be told"
        )))
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// Each part as (dimension, name, centre).
fn named_places(
    gmsh: &Gmsh,
    parts: &[(Shape, String)],
) -> Result<Vec<(i32, String, [f64; 3])>, GmshError> {
    (parts.iter())
        .map(|(shape, name)| Ok((shape.dim, name.clone(), place_of(gmsh, shape)?)))
        .collect()
}

fn place_of(gmsh: &Gmsh, shape: &Shape) -> Result<[f64; 3], GmshError> {
    let (min, max) = shape.bounding_box(gmsh)?;
    Ok([0, 1, 2].map(|k| 0.5 * (min[k] + max[k])))
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}

fn total_volume(gmsh: &Gmsh) -> Result<f64, GmshError> {
    let mut volume = 0.0;
    for tag in gmsh.entities(3)? {
        volume += gmsh.mass(3, tag)?;
    }
    Ok(volume)
}
