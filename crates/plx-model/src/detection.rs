//! Contact detection, Ansys' connection groups: a search for touching surfaces with its
//! parameters kept in the model, so that it can run again after the geometry or the mesh
//! changed. Each pair of touching surfaces found becomes a [`ContactRegion`] of the
//! detection's type. Detecting again replaces the regions found before, except those the
//! user changed since.
//!
//! A detection runs on the display of the CAD geometry when the model has one, Ansys'
//! detection on the geometry: the regions take the whole CAD faces that touch, so they exist
//! before the parts are meshed and stay the same when they are meshed again.

use std::collections::{BTreeSet, HashSet};

use std::collections::BTreeMap;

use plx_mesh::{
    CadEntity, FeMesh, GeometryFilter, GroupBy, PartSkin, SearchParameters, SurfaceId, SurfaceKind,
    display_cad_map, find_contact_pairs, surface_faces,
};
use serde::{Deserialize, Serialize};

use crate::connection::parts_of;
use crate::{ContactBehavior, ContactRegion, FeModel, Region};

/// The parts of the two sides of a region, the smaller set first.
type PartPair = (BTreeSet<usize>, BTreeSet<usize>);

/// A search for contact regions and the parameters it runs with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContactDetection {
    pub name: String,
    /// Names of the parts searched; empty searches all visible parts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<String>,
    /// Largest gap between surfaces that touch; `None` derives it from the model size.
    #[serde(default)]
    pub tolerance: Option<f64>,
    /// Largest angle between the surfaces facing each other, in degrees.
    pub angle_deg: f64,
    pub filter: DetectionFilter,
    pub grouping: DetectionGrouping,
    /// The type of the regions found.
    pub behavior: ContactBehavior,
}

impl ContactDetection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parts: Vec::new(),
            tolerance: None,
            angle_deg: 35.0,
            filter: DetectionFilter::default(),
            grouping: DetectionGrouping::default(),
            behavior: ContactBehavior::Bonded,
        }
    }
}

/// Which surfaces a detection searches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionFilter {
    /// Outer faces of solids.
    pub solid: bool,
    /// Top and bottom of shells and membranes.
    pub shell: bool,
    /// Free edges of shells and membranes.
    pub shell_edge: bool,
}

impl Default for DetectionFilter {
    fn default() -> Self {
        Self {
            solid: true,
            shell: true,
            shell_edge: true,
        }
    }
}

/// How touching surfaces are gathered into regions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionGrouping {
    /// One region per pair of touching parts.
    #[default]
    Parts,
    /// One region per pair of touching surfaces (CAD faces).
    Faces,
    /// Surfaces sharing nodes are merged, so that every node is on the slave side once.
    Graph,
}

impl DetectionGrouping {
    pub const ALL: [Self; 3] = [Self::Parts, Self::Faces, Self::Graph];

    pub fn name(self) -> &'static str {
        match self {
            Self::Parts => "Parts",
            Self::Faces => "Faces",
            Self::Graph => "Graph",
        }
    }
}

/// What a detection did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetectionReport {
    /// The tolerance it searched with.
    pub tolerance: f64,
    /// Regions found and added.
    pub added: usize,
    /// Regions of the previous detection that were replaced.
    pub replaced: usize,
    /// Regions the user changed, kept although found again.
    pub kept: Vec<String>,
    /// Regions the user changed that the detection no longer finds.
    pub lost: Vec<String>,
}

impl DetectionReport {
    /// One line for the output window.
    pub fn summary(&self, name: &str) -> String {
        let mut text = format!(
            "{name}: {} contact region{} found",
            self.added + self.kept.len(),
            if self.added + self.kept.len() == 1 {
                ""
            } else {
                "s"
            }
        );
        if !self.kept.is_empty() {
            text += &format!(", {} changed by you kept", self.kept.len());
        }
        if !self.lost.is_empty() {
            text += &format!(
                "; no longer found but kept because you changed them: {}",
                self.lost.join(", ")
            );
        }
        text
    }
}

/// Default tolerance of a detection: 0.5 % of the diagonal of the parts searched, but at
/// least half their mean element size, so that the faces of different meshes on a curved
/// surface still count as touching.
pub fn auto_detection_tolerance(mesh: &FeMesh, skins: &[PartSkin], searched: &[bool]) -> f64 {
    tolerance_of(mesh, skins, searched, true)
}

/// Default tolerance of a detection on the display of the CAD geometry: 0.5 % of the
/// diagonal of the parts searched. Its triangles say nothing about the mesh to come, and
/// the long thin ones along curved faces would make the tolerance far too large.
pub fn auto_geometry_detection_tolerance(
    display: &FeMesh,
    skins: &[PartSkin],
    searched: &[bool],
) -> f64 {
    tolerance_of(display, skins, searched, false)
}

fn tolerance_of(mesh: &FeMesh, skins: &[PartSkin], searched: &[bool], by_elements: bool) -> f64 {
    let coords = mesh.coords();
    let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let (mut length, mut edges) = (0.0, 0usize);
    let skins = skins.iter().zip(searched).filter(|(_, s)| **s);
    for face in skins.flat_map(|(skin, _)| &skin.faces) {
        for (k, &corner) in face.corners.iter().enumerate() {
            let p = coords[corner];
            for d in 0..3 {
                min[d] = min[d].min(p[d]);
                max[d] = max[d].max(p[d]);
            }
            let q = coords[face.corners[(k + 1) % face.corners.len()]];
            length += (0..3).map(|d| (q[d] - p[d]).powi(2)).sum::<f64>().sqrt();
            edges += 1;
        }
    }
    if edges == 0 {
        return 0.0;
    }
    let diagonal = (0..3)
        .map(|d| (max[d] - min[d]).powi(2))
        .sum::<f64>()
        .sqrt();
    if by_elements {
        (0.005 * diagonal).max(0.5 * length / edges as f64)
    } else {
        0.005 * diagonal
    }
}

/// The display of CAD geometry a detection runs on, see [`FeModel::detect_contacts_on_geometry`].
pub struct DetectionGeometry<'a> {
    /// The display mesh: triangles on the CAD faces, lines along the CAD edges.
    pub mesh: &'a FeMesh,
    pub skins: &'a [PartSkin],
    /// The CAD face or edge each element shows; element ids count from 1.
    pub entities: &'a [CadEntity],
    /// The CAD edges bounding each CAD face.
    pub face_edges: &'a BTreeMap<i32, Vec<i32>>,
    /// Of each part, the dimension of its CAD shape: 3 a solid, 2 faces, 1 edges.
    pub part_dimensions: &'a [u8],
}

impl FeModel {
    /// Runs the detection `index`: finds the touching surfaces of the parts it searches
    /// (`visible` says which parts are shown, for a detection of all parts) and replaces its
    /// regions with the ones found. Regions the user changed since stay as they are.
    pub fn detect_contacts(
        &mut self,
        index: usize,
        mesh: &FeMesh,
        skins: &[PartSkin],
        visible: &[bool],
    ) -> Option<DetectionReport> {
        self.detect(index, mesh, skins, visible, None)
    }

    /// [`Self::detect_contacts`] on the display of the CAD geometry: the sides of the
    /// regions are the CAD faces, or the CAD edges of shells, that touch anywhere, so that
    /// the regions are found before the parts are meshed and keep to the geometry when they
    /// are meshed again. The tie tolerance is left to the mesh unless the detection sets one.
    pub fn detect_contacts_on_geometry(
        &mut self,
        index: usize,
        geometry: &DetectionGeometry,
        visible: &[bool],
    ) -> Option<DetectionReport> {
        let mut mesh = geometry.mesh.clone();
        mesh.cad = display_cad_map(&mesh, geometry.entities, geometry.face_edges);
        let skins = geometry.skins;
        self.detect(index, &mesh, skins, visible, Some(geometry.part_dimensions))
    }

    /// The detection on a mesh, or on a display of CAD geometry with the dimensions of its
    /// parts.
    fn detect(
        &mut self,
        index: usize,
        mesh: &FeMesh,
        skins: &[PartSkin],
        visible: &[bool],
        display: Option<&[u8]>,
    ) -> Option<DetectionReport> {
        let detection = self.contact_detections.get(index)?.clone();
        let searched: Vec<bool> = (mesh.parts.iter().enumerate())
            .map(|(i, part)| {
                if detection.parts.is_empty() {
                    visible.get(i).copied().unwrap_or(true)
                } else {
                    detection.parts.contains(&part.name)
                }
            })
            .collect();
        let tolerance = (detection.tolerance)
            .unwrap_or_else(|| tolerance_of(mesh, skins, &searched, display.is_none()));
        let young_of = self.young_of_elements(mesh);
        let stiffness = (mesh.parts.iter())
            .map(|part| {
                (part.elements.iter())
                    .filter_map(|e| young_of.get(e).copied())
                    .reduce(f64::max)
            })
            .collect();
        let filter = if self.properties.space.is_2d() {
            GeometryFilter {
                solid: false,
                shell: false,
                shell_edge: true,
            }
        } else if display.is_some() {
            // The display shows the faces of solids as triangles, searched like shells.
            GeometryFilter {
                solid: false,
                shell: detection.filter.solid || detection.filter.shell,
                shell_edge: detection.filter.shell_edge,
            }
        } else {
            GeometryFilter {
                solid: detection.filter.solid,
                shell: detection.filter.shell,
                shell_edge: detection.filter.shell_edge,
            }
        };
        let parameters = SearchParameters {
            distance: tolerance,
            angle_deg: detection.angle_deg,
            group_by: match detection.grouping {
                DetectionGrouping::Parts => GroupBy::Parts,
                DetectionGrouping::Faces => GroupBy::None,
                DetectionGrouping::Graph => GroupBy::Graph,
            },
            stiffness,
            filter,
        };
        let mut items = find_contact_pairs(mesh, skins, &searched, &parameters);
        if let Some(dimensions) = display {
            // Of solids only their faces count, which the filter asks for as solid faces;
            // the borders of their faces are no edges of shells.
            let wanted = |id: &SurfaceId| match (dimensions.get(id.0), id.1) {
                (Some(3), SurfaceKind::ShellEdge) => false,
                (Some(3), _) => detection.filter.solid,
                (_, SurfaceKind::ShellEdge) => true,
                _ => detection.filter.shell || self.properties.space.is_2d(),
            };
            for item in &mut items {
                item.master.retain(wanted);
                item.slave.retain(wanted);
            }
        }
        // Surfaces of whole CAD faces are kept by geometry, so that they survive remeshing;
        // on the geometry every face touched is taken whole.
        let region = |surfaces: &BTreeSet<SurfaceId>| {
            let faces = surface_faces(mesh, skins, surfaces);
            if display.is_some() {
                let entities = mesh.cad_entities_touched(&faces);
                return if entities.is_empty() {
                    Region::Faces(Vec::new())
                } else {
                    Region::Geometry(entities)
                };
            }
            match mesh.whole_cad_faces(&faces) {
                Some(entities) => Region::Geometry(entities),
                None => Region::Faces(faces),
            }
        };
        let mut report = DetectionReport {
            tolerance,
            ..DetectionReport::default()
        };
        let ours = |r: &ContactRegion| r.detected_by.as_deref() == Some(detection.name.as_str());
        let before = self.contact_regions.len();
        self.contact_regions.retain(|r| !ours(r) || r.edited);
        report.replaced = before - self.contact_regions.len();
        // The parts each kept region connects, to recognise it among the regions found.
        let key = |a: &Region, b: &Region| {
            let (a, b) = (parts_of(a, mesh), parts_of(b, mesh));
            if a <= b { (a, b) } else { (b, a) }
        };
        let edited: Vec<(String, PartPair)> = (self.contact_regions.iter().filter(|r| ours(r)))
            .map(|r| (r.name.clone(), key(&r.side_a, &r.side_b)))
            .collect();
        let mut found_again = HashSet::new();
        for item in &items {
            let mut found = ContactRegion::new("", detection.behavior);
            found.side_a = region(&item.master);
            found.side_b = region(&item.slave);
            let empty = |r: &Region| matches!(r, Region::Faces(f) if f.is_empty());
            if empty(&found.side_a) || empty(&found.side_b) {
                continue;
            }
            let parts = key(&found.side_a, &found.side_b);
            if let Some((name, _)) = edited.iter().find(|(_, k)| *k == parts) {
                found_again.insert(name.clone());
                continue;
            }
            // The side found as master is side A; its tolerance is the one it was found
            // with, so that a tie catches every node the detection saw touching. Found on
            // the geometry, the mesh to come sets it unless the detection does.
            found.settings.tolerance = match display {
                Some(_) => detection.tolerance,
                None => Some(tolerance),
            };
            found.detected_by = Some(detection.name.clone());
            let name = found.name_from_definition(mesh);
            let taken = self.contact_pair_names();
            found.name = if taken.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
                crate::next_name(&name, taken)
            } else {
                name
            };
            self.contact_regions.push(found);
            report.added += 1;
        }
        for (name, _) in edited {
            if found_again.contains(&name) {
                report.kept.push(name);
            } else {
                report.lost.push(name);
            }
        }
        Some(report)
    }
}
