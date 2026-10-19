//! CAD operations on the geometry with OpenCASCADE through Gmsh: fillets and chamfers of
//! edges, see [`plx_model::CadOperation`].

use std::collections::BTreeSet;

use plx_mesh::CadEntity;
use plx_model::{CadOperationKind, Geometry};

use crate::gmsh::{Gmsh, GmshError, with_gmsh};
use crate::{Entity, TempFile, default_parts, parts, signatures};

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
    if !(value.is_finite() && value > 0.0) {
        return Err(fail(format!(
            "The {} must be greater than zero",
            kind.value_label().to_lowercase()
        )));
    }
    let file = TempFile::with_contents("brep", &input.brep)?;
    let result = TempFile::new("brep");
    let names = with_gmsh(|gmsh| {
        gmsh.import_shapes(&file.0)?;
        let before = named_places(gmsh, &parts(gmsh, input)?)?;
        let volumes_before = gmsh.entities(3)?.len();
        let volume_before = total_volume(gmsh)?;
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
            CadOperationKind::Fillet => gmsh.fillet(&volumes, &edges, value),
            CadOperationKind::Chamfer => gmsh.chamfer(&volumes, &edges, &faces, value),
        };
        let made = made.map_err(|e| {
            fail(format!(
                "OpenCASCADE could not make the {}: {e}",
                kind.label().to_lowercase()
            ))
        })?;
        if made.is_empty() {
            return Err(fail(format!(
                "OpenCASCADE made no {}: the {} may be too large for the faces next to the \
                 edges",
                kind.label().to_lowercase(),
                kind.value_label().to_lowercase()
            )));
        }
        // OpenCASCADE may give a broken solid instead of failing: the result must have as
        // many solids, and a volume a little different from before.
        let volumes_after = gmsh.entities(3)?.len();
        let volume_after = total_volume(gmsh)?;
        if volumes_after != volumes_before
            || !(volume_after.is_finite() && volume_after > 0.0)
            || (volume_after - volume_before).abs() > 0.5 * volume_before
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
        for ((dim, tag), name) in after {
            let place = place_of(gmsh, dim, tag)?;
            let nearest = (before.iter())
                .filter(|(d, _, _)| *d == dim)
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

/// Each part as (dimension, name, centre).
fn named_places(
    gmsh: &Gmsh,
    parts: &[(Entity, String)],
) -> Result<Vec<(i32, String, [f64; 3])>, GmshError> {
    (parts.iter())
        .map(|&((dim, tag), ref name)| Ok((dim, name.clone(), place_of(gmsh, dim, tag)?)))
        .collect()
}

fn place_of(gmsh: &Gmsh, dim: i32, tag: i32) -> Result<[f64; 3], GmshError> {
    let (min, max) = gmsh.entity_bounding_box(dim, tag)?;
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
