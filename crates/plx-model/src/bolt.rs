//! Bolts preloaded across a cut through their shank, Ansys' bolt pretension: the user picks
//! the two faces of the cut and the preload, PrePoLix derives the rest when the input file is
//! written ([`FeModel::lower_bolts`]): which side the mesh is cut on, the tie between the
//! halves of a bolt split into two bodies, the pre-tension load in every step (the preload
//! in the first step that takes loads, locked at its length after it), and the contact
//! regions on the cut faces, which the bolt replaces.

use std::collections::{BTreeMap, BTreeSet};

use plx_mesh::{ElementId, FeMesh, NodeId};
use serde::{Deserialize, Serialize};

use crate::{FeModel, Load, LoadKind, ModelItem, Region, Step, Tie};

/// A bolt cut through its shank: the faces on the two sides of the cut and its preload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bolt {
    pub name: String,
    #[serde(default = "crate::active")]
    pub active: bool,
    /// The faces of the cut: of the two bodies of a split bolt, or both on the face inside a
    /// bolt meshed in one piece (side B may then stay empty).
    pub side_a: Region,
    pub side_b: Region,
    /// The preload in the first step that takes loads.
    pub preload: Preload,
    /// Direction of the preload; `None` takes the normal of the cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    pub color: [u8; 3],
}

impl Bolt {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            active: true,
            side_a: Region::Faces(Vec::new()),
            side_b: Region::Faces(Vec::new()),
            preload: Preload::Force(0.0),
            axis: None,
            color: [230, 140, 0],
        }
    }

    pub fn regions(&self) -> [&Region; 2] {
        [&self.side_a, &self.side_b]
    }

    pub fn regions_mut(&mut self) -> [&mut Region; 2] {
        [&mut self.side_a, &mut self.side_b]
    }

    /// The nodes of both sides of the cut, of their faces too.
    pub fn nodes(&self, mesh: &FeMesh) -> BTreeSet<NodeId> {
        self.regions()
            .into_iter()
            .flat_map(|region| side_nodes(region, mesh))
            .collect()
    }
}

/// How a bolt is tightened.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Preload {
    /// The force pulling the two sides of the cut together.
    Force(f64),
    /// The length the bolt is shortened by.
    Shortening(f64),
}

impl Preload {
    pub fn value(self) -> f64 {
        match self {
            Preload::Force(v) | Preload::Shortening(v) => v,
        }
    }
}

/// What a bolt does in a step, Ansys' "Define By" of a bolt pretension.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BoltAction {
    /// Pulled by a force.
    Load(f64),
    /// Shortened by a length since the start of the analysis.
    Adjust(f64),
    /// Kept at the length it had at the end of the previous step.
    Lock,
    /// Loose: no preload.
    Open,
}

impl BoltAction {
    pub fn name(self) -> &'static str {
        match self {
            BoltAction::Load(_) => "Load",
            BoltAction::Adjust(_) => "Adjustment",
            BoltAction::Lock => "Lock",
            BoltAction::Open => "Open",
        }
    }
}

impl From<Preload> for BoltAction {
    fn from(preload: Preload) -> Self {
        match preload {
            Preload::Force(f) => BoltAction::Load(f),
            Preload::Shortening(u) => BoltAction::Adjust(u),
        }
    }
}

/// What a bolt does in one step instead of the automatic action ([`FeModel::bolt_action`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoltChange {
    pub bolt: String,
    pub action: BoltAction,
}

/// How the two sides of a bolt's cut are joined in the input file.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedBolt {
    /// The element faces of the side the mesh is cut on, all on one side of the cut.
    pub cut_faces: Vec<(ElementId, u8)>,
    /// The other side, tied to the cut side; `None` when the sides share their nodes.
    pub tied_to: Option<Region>,
    /// The cut side is side A.
    pub cut_on_a: bool,
    /// Distance the tie looks for the other side within.
    pub tie_tolerance: Option<f64>,
    /// Why, in words, for the dialog.
    pub why: String,
}

/// What [`FeModel::lower_bolts`] added, by the bolt each item stands for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoweredBolts {
    pub first_tie: usize,
    pub tie_bolts: Vec<usize>,
    /// Per step: the index of the first added load and the bolt of each added load.
    pub first_loads: Vec<usize>,
    pub load_bolts: Vec<Vec<usize>>,
    /// The contact regions the bolts replace, with the bolt.
    pub replaced: BTreeMap<usize, usize>,
}

impl LoweredBolts {
    /// The bolt a tie or load of the lowered model stands for.
    pub fn bolt_of(&self, item: ModelItem) -> Option<usize> {
        match item {
            ModelItem::Tie(i) => self.tie_bolts.get(i.checked_sub(self.first_tie)?).copied(),
            ModelItem::Load(s, i) => {
                let first = *self.first_loads.get(s)?;
                self.load_bolts.get(s)?.get(i.checked_sub(first)?).copied()
            }
            _ => None,
        }
    }
}

/// Share of the nodes of both sides of a contact region on a bolt's cut that makes the bolt
/// replace the region.
const REPLACED_SHARE: f64 = 0.5;

impl FeModel {
    /// What the bolt does in the step `index`: the change of the step, else
    /// [`FeModel::auto_bolt_action`]. `None` when the step takes no pre-tension or the bolt
    /// is open.
    pub fn bolt_action(&self, bolt: &Bolt, index: usize) -> Option<BoltAction> {
        let auto = self.auto_bolt_action(bolt, index)?;
        let action = (self.steps[index].bolt_changes.iter())
            .find(|c| c.bolt == bolt.name)
            .map_or(auto, |c| c.action);
        (action != BoltAction::Open).then_some(action)
    }

    /// What the bolt does in the step `index` without a change: the preload in the first
    /// active step that takes it, the lock after it and nothing before it. `None` when the
    /// step takes no pre-tension.
    pub fn auto_bolt_action(&self, bolt: &Bolt, index: usize) -> Option<BoltAction> {
        let step = self.steps.get(index)?;
        if !step.active || !takes_pre_tension(step) {
            return None;
        }
        let first = self
            .steps
            .iter()
            .position(|s| s.active && takes_pre_tension(s))?;
        Some(match index.cmp(&first) {
            std::cmp::Ordering::Less => BoltAction::Open,
            std::cmp::Ordering::Equal => bolt.preload.into(),
            std::cmp::Ordering::Greater => BoltAction::Lock,
        })
    }

    /// How the sides of the bolt are joined: the side with the finer mesh is cut, the other
    /// one tied to it; a cut inside a bolt meshed in one piece needs no tie.
    pub fn resolve_bolt(&self, bolt: &Bolt, mesh: &FeMesh) -> Result<ResolvedBolt, String> {
        let a = solid_faces(&bolt.side_a, mesh);
        let b = solid_faces(&bolt.side_b, mesh);
        if a.is_empty() && b.is_empty() {
            return Err("no faces of solid elements on the cut".into());
        }
        let nodes = |faces: &[(ElementId, u8)]| -> BTreeSet<NodeId> {
            faces.iter().flat_map(|&f| face_nodes(mesh, f)).collect()
        };
        let (nodes_a, nodes_b) = (nodes(&a), nodes(&b));
        let shared = b.is_empty() || nodes_a.intersection(&nodes_b).next().is_some();
        if shared {
            // One mesh through the cut: the faces of the elements on one side of it.
            let faces = if a.is_empty() { &b } else { &a };
            return Ok(ResolvedBolt {
                cut_faces: one_side(faces, mesh),
                tied_to: None,
                cut_on_a: !a.is_empty(),
                tie_tolerance: None,
                why: "The sides share their nodes: the mesh is cut between them.".into(),
            });
        }
        if a.is_empty() {
            return Err("side A has no faces of solid elements".into());
        }
        let (size_a, size_b) = (mean_size(&a, mesh), mean_size(&b, mesh));
        // The finer side is cut, so the tie's slave nodes are the denser ones.
        let cut_on_a = size_a <= size_b;
        let (cut, other, region, size) = if cut_on_a {
            (a, &bolt.side_b, "A", size_a)
        } else {
            (b, &bolt.side_a, "B", size_b)
        };
        Ok(ResolvedBolt {
            cut_faces: one_side(&cut, mesh),
            tied_to: Some(other.clone()),
            cut_on_a,
            tie_tolerance: (size > 0.0).then_some(0.25 * size),
            why: format!(
                "Two bodies: the mesh is cut on side {region}{} and tied to the other side.",
                if size_a == size_b {
                    ""
                } else {
                    " (finer mesh)"
                }
            ),
        })
    }

    /// The contact regions the active bolts replace, with the bolt: those with both sides
    /// mostly on the faces of a bolt's cut.
    pub fn replaced_contact_regions(&self, mesh: &FeMesh) -> BTreeMap<usize, usize> {
        let mut replaced = BTreeMap::new();
        if self.contact_regions.is_empty() {
            return replaced;
        }
        let bolts: Vec<(usize, BTreeSet<NodeId>)> = (self.bolts.iter().enumerate())
            .filter(|(_, b)| b.active)
            .map(|(i, b)| (i, b.nodes(mesh)))
            .collect();
        for (r, region) in self.contact_regions.iter().enumerate() {
            let sides = region.regions().map(|side| side_nodes(side, mesh));
            if let Some(&(b, _)) = bolts.iter().find(|(_, cut)| {
                sides.iter().all(|nodes| {
                    let on = nodes.iter().filter(|n| cut.contains(n)).count();
                    !nodes.is_empty() && on as f64 >= REPLACED_SHARE * nodes.len() as f64
                })
            }) {
                replaced.insert(r, b);
            }
        }
        replaced
    }

    /// The model with its bolts replaced by the pre-tension loads and ties they stand for,
    /// and the contact regions on their cuts switched off. Bolts whose cut cannot be
    /// resolved are left out; the model check reports them.
    pub fn lower_bolts(&self, mesh: &FeMesh) -> (FeModel, LoweredBolts) {
        let mut lowered = self.clone();
        let mut origin = LoweredBolts {
            first_tie: self.ties.len(),
            first_loads: self.steps.iter().map(|s| s.loads.len()).collect(),
            load_bolts: vec![Vec::new(); self.steps.len()],
            ..LoweredBolts::default()
        };
        if self.bolts.is_empty() {
            return (lowered, origin);
        }
        lowered.bolts.clear();
        for step in &mut lowered.steps {
            step.bolt_changes.clear();
        }
        origin.replaced = self.replaced_contact_regions(mesh);
        for &r in origin.replaced.keys() {
            let region = &mut lowered.contact_regions[r];
            region.active = false;
            let name = region.name.clone();
            lowered.remove_contact_changes(&name);
        }
        let mut taken: BTreeSet<String> = (self.steps.iter())
            .flat_map(|s| s.loads.iter().map(|l| l.name.clone()))
            .chain(self.ties.iter().map(|t| t.name.clone()))
            .collect();
        for (b, bolt) in self.bolts.iter().enumerate().filter(|(_, b)| b.active) {
            let Ok(resolved) = self.resolve_bolt(bolt, mesh) else {
                continue;
            };
            let name = unique(&bolt.name, &mut taken);
            if let Some(other) = &resolved.tied_to {
                let mut tie = Tie::new(unique(&format!("{name}_Cut"), &mut taken));
                tie.master = other.clone();
                tie.slave = Region::Faces(resolved.cut_faces.clone());
                tie.position_tolerance = resolved.tie_tolerance;
                lowered.ties.push(tie);
                origin.tie_bolts.push(b);
            }
            for s in 0..self.steps.len() {
                let Some(action) = self.bolt_action(bolt, s) else {
                    continue;
                };
                let (value, by_displacement, lock) = match action {
                    BoltAction::Load(f) => (f, false, false),
                    BoltAction::Adjust(u) => (u, true, false),
                    BoltAction::Lock => (0.0, false, true),
                    BoltAction::Open => continue,
                };
                lowered.steps[s].loads.push(Load {
                    name: name.clone(),
                    active: true,
                    visible: true,
                    region: Region::Faces(resolved.cut_faces.clone()),
                    kind: LoadKind::PreTension {
                        value,
                        by_displacement,
                        lock,
                        direction: bolt.axis,
                    },
                    amplitude: None,
                    factor_amplitude: None,
                    distribution: None,
                });
                origin.load_bolts[s].push(b);
            }
        }
        (lowered, origin)
    }

    /// Drops the step changes of a bolt that is deleted.
    pub fn remove_bolt_changes(&mut self, name: &str) {
        for step in &mut self.steps {
            step.bolt_changes.retain(|c| c.bolt != name);
        }
    }

    /// Follows a renamed bolt: the step changes keep referring to it.
    pub fn rename_bolt(&mut self, old: &str, new: &str) {
        for change in self.steps.iter_mut().flat_map(|s| &mut s.bolt_changes) {
            if change.bolt == old {
                change.bolt = new.to_string();
            }
        }
    }
}

/// Whether a step takes a pre-tension: one that takes forces.
fn takes_pre_tension(step: &Step) -> bool {
    step.kind.supports_load(&LoadKind::PreTension {
        value: 0.0,
        by_displacement: false,
        lock: false,
        direction: None,
    })
}

fn unique(name: &str, taken: &mut BTreeSet<String>) -> String {
    let mut candidate = name.to_string();
    let mut n = 2;
    while taken.contains(&candidate) {
        candidate = format!("{name}-{n}");
        n += 1;
    }
    taken.insert(candidate.clone());
    candidate
}

/// The nodes of a region and of its faces: a side picked on the geometry may list either.
pub(crate) fn side_nodes(region: &Region, mesh: &FeMesh) -> BTreeSet<NodeId> {
    (region.nodes(mesh).into_iter())
        .chain(
            region
                .faces(mesh)
                .into_iter()
                .flat_map(|f| face_nodes(mesh, f)),
        )
        .collect()
}

/// The faces of the region that belong to solid elements.
fn solid_faces(region: &Region, mesh: &FeMesh) -> Vec<(ElementId, u8)> {
    (region.faces(mesh).into_iter())
        .filter(|&(e, _)| {
            mesh.element(e)
                .is_some_and(|e| e.shape.family() == plx_mesh::ElementFamily::Solid)
        })
        .collect()
}

/// All nodes of an element face, corners and midside nodes.
fn face_nodes(mesh: &FeMesh, (element, face): (ElementId, u8)) -> Vec<NodeId> {
    let Some(element) = mesh.element(element) else {
        return Vec::new();
    };
    let Some(topology) = element.faces().get(usize::from(face).saturating_sub(1)) else {
        return Vec::new();
    };
    (topology.corners.iter().chain(topology.mids))
        .filter_map(|&i| element.nodes.get(i).copied())
        .collect()
}

/// Outward normal (not unit) and area of an element face.
fn face_normal(mesh: &FeMesh, (element, face): (ElementId, u8)) -> Option<[f64; 3]> {
    let element = mesh.element(element)?;
    let topology = element.faces().get(usize::from(face).checked_sub(1)?)?;
    let corners: Vec<[f64; 3]> = (topology.corners.iter())
        .map(|&i| mesh.node(*element.nodes.get(i)?))
        .collect::<Option<_>>()?;
    // Newell's method, times the area.
    let mut n = [0.0; 3];
    for i in 0..corners.len() {
        let (p, q) = (corners[i], corners[(i + 1) % corners.len()]);
        n[0] += (p[1] - q[1]) * (p[2] + q[2]);
        n[1] += (p[2] - q[2]) * (p[0] + q[0]);
        n[2] += (p[0] - q[0]) * (p[1] + q[1]);
    }
    Some(n.map(|c| c / 2.0))
}

/// The faces whose outward normal points the way of the first one: on a face inside a mesh
/// both elements at it have a face there, and the cut takes those of one side.
fn one_side(faces: &[(ElementId, u8)], mesh: &FeMesh) -> Vec<(ElementId, u8)> {
    let Some(reference) = faces.iter().find_map(|&f| face_normal(mesh, f)) else {
        return faces.to_vec();
    };
    (faces.iter().copied())
        .filter(|&f| {
            face_normal(mesh, f).is_some_and(|n| {
                n[0] * reference[0] + n[1] * reference[1] + n[2] * reference[2] > 0.0
            })
        })
        .collect()
}

/// Square root of the mean face area: the size of the mesh on the faces.
fn mean_size(faces: &[(ElementId, u8)], mesh: &FeMesh) -> f64 {
    let areas: Vec<f64> = (faces.iter())
        .filter_map(|&f| face_normal(mesh, f))
        .map(|n| (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt())
        .collect();
    if areas.is_empty() {
        0.0
    } else {
        (areas.iter().sum::<f64>() / areas.len() as f64).sqrt()
    }
}
