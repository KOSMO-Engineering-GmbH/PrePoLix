//! Remote displacements: prescribed displacements and rotations of a point, by default at
//! the centre of the selection, that the selection follows as a rigid body, on average
//! (deformable) or with its nodes coupled to the point, as Ansys' remote displacement does.
//! The point exists only in the input file.

use std::collections::BTreeMap;

use plx_mesh::{ElementId, FeMesh, NodeId};
use serde::{Deserialize, Serialize};

use crate::features::GLOBAL;
use crate::{FeModel, ModelSpace, Region};

/// How the selection follows the remote point, Ansys' "Behavior".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteBehavior {
    /// The selection keeps its shape (`*RIGID BODY`).
    #[default]
    Rigid,
    /// The point moves with the weighted mean of the selection, which may deform: Nastran's
    /// RBE3, written as `*EQUATION`s.
    Deformable,
    /// Every node of the selection moves like the point in the prescribed directions, and
    /// freely in the others; rotations are not coupled.
    Coupled,
}

impl RemoteBehavior {
    pub const ALL: [RemoteBehavior; 3] = [Self::Rigid, Self::Deformable, Self::Coupled];

    pub fn label(self) -> &'static str {
        match self {
            Self::Rigid => "Rigid",
            Self::Deformable => "Deformable",
            Self::Coupled => "Coupled",
        }
    }
}

/// A remote displacement, Ansys' remote displacement as far as CalculiX can do it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteDisplacement {
    /// Displacements (U1..U3) and rotations (UR1..UR3) of the point in the directions of
    /// `system`; `None` leaves one free.
    pub values: [Option<f64>; 6],
    pub behavior: RemoteBehavior,
    /// Coordinate system of the location and of the directions, by name ([`GLOBAL`]).
    pub system: String,
    /// Coordinates of the point in `system`; `None` is the centre of the selection.
    pub location: Option<[f64; 3]>,
    /// Only the nodes within this distance of the point take part, Ansys' pinball region.
    pub pinball: Option<f64>,
}

/// A remote displacement resolved on a mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct RemotePoint {
    pub position: [f64; 3],
    /// The local x, y and z directions at the point; `None` for the global ones.
    pub axes: Option<[[f64; 3]; 3]>,
    /// The nodes that follow the point.
    pub nodes: Vec<NodeId>,
    /// Weight of each node in a deformable coupling: its share of the face areas.
    pub weights: Vec<f64>,
}

/// A degree of freedom of an equation: of a node of the mesh, or the translation or rotation
/// of the remote point in a global direction 0..3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RemoteDof {
    Node(NodeId, usize),
    Translation(usize),
    Rotation(usize),
}

impl FeModel {
    /// The first remote displacement boundary condition of the name, of any step, with
    /// the index of its step.
    pub fn remote_displacement(
        &self,
        name: &str,
    ) -> Option<(usize, &crate::BoundaryCondition, &RemoteDisplacement)> {
        self.steps.iter().enumerate().find_map(|(s, step)| {
            step.boundary_conditions
                .iter()
                .find_map(|bc| match &bc.kind {
                    crate::BoundaryKind::RemoteDisplacement(remote) if bc.name == name => {
                        Some((s, bc, remote))
                    }
                    _ => None,
                })
        })
    }

    /// Follows a renamed remote displacement: history outputs of its point keep referring
    /// to it.
    pub fn rename_remote_point(&mut self, old: &str, new: &str) {
        for output in self.steps.iter_mut().flat_map(|s| &mut s.history_outputs) {
            if let Some(Region::RemotePoint(name)) = output.kind.region_mut()
                && name == old
            {
                *name = new.to_string();
            }
        }
    }
}

impl RemoteDisplacement {
    pub fn new(values: [Option<f64>; 6]) -> Self {
        Self {
            values,
            behavior: RemoteBehavior::default(),
            system: GLOBAL.into(),
            location: None,
            pinball: None,
        }
    }

    /// The same point and coupling, whatever the values: remote displacements of several
    /// steps share it. A coupling depends on which directions are held.
    pub fn same_point(&self, other: &Self) -> bool {
        let held = |r: &Self| r.values.map(|v| v.is_some());
        self.behavior == other.behavior
            && self.system == other.system
            && self.location == other.location
            && self.pinball == other.pinball
            && (self.behavior != RemoteBehavior::Coupled || held(self) == held(other))
    }

    /// The degrees of freedom (0..6) of the point the model space and the behavior allow.
    pub fn dofs(&self, space: ModelSpace) -> Vec<usize> {
        let coupled = self.behavior == RemoteBehavior::Coupled;
        (space.remote_dofs().iter().copied())
            .filter(|&d| !coupled || d < 3)
            .collect()
    }

    /// The values of the degrees of freedom [`Self::dofs`] allows, the others free.
    pub fn held(&self, space: ModelSpace) -> [Option<f64>; 6] {
        let mut held = [None; 6];
        for d in self.dofs(space) {
            held[d] = self.values[d];
        }
        held
    }

    /// The point, its directions and the nodes following it.
    pub fn resolve(
        &self,
        model: &FeModel,
        mesh: &FeMesh,
        region: &Region,
    ) -> Result<RemotePoint, String> {
        let system = (model.coordinate_system_or_global(&self.system))
            .ok_or_else(|| format!("Coordinate System {} does not exist", self.system))?;
        let position = match self.location {
            Some(local) => system.global(local)?,
            None => (region.centroid(mesh))
                .ok_or_else(|| "The selection contains no nodes.".to_string())?,
        };
        let axes = system.directions_at(position)?;
        let global = (0..3).all(|i| (0..3).all(|k| (axes[i][k] - f64::from(i == k)).abs() < 1e-12));
        let within = |node: NodeId| match (self.pinball, mesh.node(node)) {
            (Some(radius), Some(x)) => distance(x, position) <= radius,
            (None, Some(_)) => true,
            (_, None) => false,
        };
        let nodes: Vec<NodeId> = region
            .nodes(mesh)
            .into_iter()
            .filter(|&n| within(n))
            .collect();
        if nodes.is_empty() {
            return Err(match self.pinball {
                Some(_) => "No node of the selection lies within the pinball region.".into(),
                None => "The selection contains no nodes.".into(),
            });
        }
        let areas = face_areas(mesh, &region.faces(mesh));
        let weights = (nodes.iter())
            .map(|n| areas.get(n).copied().unwrap_or(0.0))
            .collect::<Vec<_>>();
        // Without faces, or nodes beside them, every node counts alike.
        let weights = if weights.iter().all(|&w| w > 0.0) {
            weights
        } else {
            vec![1.0; nodes.len()]
        };
        Ok(RemotePoint {
            position,
            axes: (!global).then_some(axes),
            nodes,
            weights,
        })
    }
}

impl RemotePoint {
    /// The `*EQUATION`s of a deformable or coupled behavior, each as its terms with the
    /// dependent node first: a node degree of freedom that no other equation has, so
    /// CalculiX needs no cascading. `held` are the point's prescribed degrees of freedom
    /// (of a coupling, in the point's directions).
    pub fn equations(
        &self,
        mesh: &FeMesh,
        behavior: RemoteBehavior,
        held: &[Option<f64>; 6],
        space: ModelSpace,
    ) -> Vec<Vec<(RemoteDof, f64)>> {
        let rows = match behavior {
            RemoteBehavior::Rigid => return Vec::new(),
            RemoteBehavior::Deformable => self.averaging_rows(mesh, space),
            RemoteBehavior::Coupled => self.coupling_rows(held, space),
        };
        reduce(rows)
    }

    /// Nastran's RBE3: the point moves with the weighted mean translation of the nodes and
    /// turns with their weighted mean rotation about their centre c,
    /// J theta = sum w r x u with J = sum w (|r|^2 I - r r^T), r = x - c, and
    /// u_point = sum w u + theta x (point - c).
    fn averaging_rows(&self, mesh: &FeMesh, space: ModelSpace) -> Vec<Vec<(RemoteDof, f64)>> {
        let total: f64 = self.weights.iter().sum();
        let points: Vec<(NodeId, f64, [f64; 3])> = (self.nodes.iter().zip(&self.weights))
            .filter_map(|(&n, &w)| Some((n, w / total, mesh.node(n)?)))
            .collect();
        let c = points
            .iter()
            .fold([0.0; 3], |s, (_, w, x)| [0, 1, 2].map(|k| s[k] + w * x[k]));
        let d: [f64; 3] = std::array::from_fn(|k| self.position[k] - c[k]);
        let mut rows = Vec::new();
        let translations = if space.is_2d() { 2 } else { 3 };
        let turns: &[usize] = match space {
            ModelSpace::ThreeD => &[0, 1, 2],
            ModelSpace::PlaneStress | ModelSpace::PlaneStrain => &[2],
            ModelSpace::Axisymmetric => &[],
        };
        for k in 0..translations {
            let mut row = vec![(RemoteDof::Translation(k), 1.0)];
            row.extend(points.iter().map(|&(n, w, _)| (RemoteDof::Node(n, k), -w)));
            // - (theta x d)_k
            let (a, b) = ((k + 1) % 3, (k + 2) % 3);
            if turns.contains(&a) {
                row.push((RemoteDof::Rotation(a), -d[b]));
            }
            if turns.contains(&b) {
                row.push((RemoteDof::Rotation(b), d[a]));
            }
            rows.push(row);
        }
        let r: Vec<[f64; 3]> = points
            .iter()
            .map(|(_, _, x)| [0, 1, 2].map(|k| x[k] - c[k]))
            .collect();
        for &k in turns {
            let mut row = Vec::new();
            for &l in turns {
                let j: f64 = (points.iter().zip(&r))
                    .map(|((_, w, _), r)| {
                        let squared = if k == l {
                            r.iter().map(|v| v * v).sum()
                        } else {
                            0.0
                        };
                        w * (squared - r[k] * r[l])
                    })
                    .sum();
                row.push((RemoteDof::Rotation(l), j));
            }
            // - (r x u)_k = -(r_a u_b - r_b u_a)
            let (a, b) = ((k + 1) % 3, (k + 2) % 3);
            for ((n, w, _), r) in points.iter().zip(&r) {
                if b < translations {
                    row.push((RemoteDof::Node(*n, b), -w * r[a]));
                }
                if a < translations {
                    row.push((RemoteDof::Node(*n, a), w * r[b]));
                }
            }
            rows.push(row);
        }
        rows
    }

    /// Each node moves like the point in each held direction e: e . u_node = e . u_point.
    fn coupling_rows(
        &self,
        held: &[Option<f64>; 6],
        space: ModelSpace,
    ) -> Vec<Vec<(RemoteDof, f64)>> {
        let translations = if space.is_2d() { 2 } else { 3 };
        let axes = self
            .axes
            .unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        let mut rows = Vec::new();
        for &node in &self.nodes {
            for (k, e) in axes.iter().enumerate().take(translations) {
                if held[k].is_none() {
                    continue;
                }
                let mut row: Vec<(RemoteDof, f64)> = (0..translations)
                    .map(|g| (RemoteDof::Node(node, g), e[g]))
                    .collect();
                row.extend((0..translations).map(|g| (RemoteDof::Translation(g), -e[g])));
                rows.push(row);
            }
        }
        rows
    }
}

/// Reduced row echelon form of the equations in the node degrees of freedom: each keeps
/// the node degree of freedom with the largest factor as its dependent one, scaled to 1 and
/// removed from all other equations. Equations without node terms are left out.
fn reduce(rows: Vec<Vec<(RemoteDof, f64)>>) -> Vec<Vec<(RemoteDof, f64)>> {
    let mut rows: Vec<BTreeMap<RemoteDof, f64>> = rows
        .into_iter()
        .map(|row| {
            let mut map = BTreeMap::new();
            for (dof, value) in row {
                *map.entry(dof).or_insert(0.0) += value;
            }
            map
        })
        .collect();
    // Rows each node degree of freedom appears in, so elimination only visits those.
    let mut users: BTreeMap<RemoteDof, Vec<usize>> = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        for dof in row.keys() {
            users.entry(*dof).or_default().push(i);
        }
    }
    let largest = (rows.iter().flat_map(|r| r.values())).fold(0.0_f64, |m, v| m.max(v.abs()));
    let tolerance = 1e-10 * largest;
    let mut pivots = Vec::new();
    for i in 0..rows.len() {
        let pivot = (rows[i].iter())
            .filter(|(dof, v)| matches!(dof, RemoteDof::Node(..)) && v.abs() > tolerance)
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(dof, v)| (*dof, *v));
        let Some((pivot, value)) = pivot else {
            continue;
        };
        for v in rows[i].values_mut() {
            *v /= value;
        }
        let source = rows[i].clone();
        for j in users.get(&pivot).cloned().unwrap_or_default() {
            if j == i {
                continue;
            }
            let Some(f) = rows[j].remove(&pivot) else {
                continue;
            };
            for (dof, v) in source.iter().filter(|(dof, _)| **dof != pivot) {
                let entry = rows[j].entry(*dof).or_insert_with(|| {
                    users.entry(*dof).or_default().push(j);
                    0.0
                });
                *entry -= f * v;
            }
        }
        pivots.push((i, pivot));
    }
    (pivots.into_iter())
        .map(|(i, pivot)| {
            let mut terms = vec![(pivot, 1.0)];
            terms.extend(
                (rows[i].iter())
                    .filter(|(dof, v)| **dof != pivot && v.abs() > tolerance)
                    .map(|(dof, v)| (*dof, *v)),
            );
            terms
        })
        .collect()
}

/// Share of the face areas (edge lengths of 2D faces) of each node of the faces.
fn face_areas(mesh: &FeMesh, faces: &[(ElementId, u8)]) -> BTreeMap<NodeId, f64> {
    let mut areas = BTreeMap::new();
    for &(element, face) in faces {
        let Some(element) = mesh.element(element) else {
            continue;
        };
        let Some(topology) = element.faces().get(usize::from(face).saturating_sub(1)) else {
            continue;
        };
        let corners: Vec<[f64; 3]> = (topology.corners.iter())
            .filter_map(|&local| mesh.node(*element.nodes.get(local)?))
            .collect();
        let area = match corners[..] {
            [a, b] => distance(a, b),
            [a, ..] if corners.len() >= 3 => (corners[1..].windows(2))
                .map(|pair| {
                    let u: [f64; 3] = std::array::from_fn(|k| pair[0][k] - a[k]);
                    let v: [f64; 3] = std::array::from_fn(|k| pair[1][k] - a[k]);
                    let c = [
                        u[1] * v[2] - u[2] * v[1],
                        u[2] * v[0] - u[0] * v[2],
                        u[0] * v[1] - u[1] * v[0],
                    ];
                    0.5 * c.iter().map(|x| x * x).sum::<f64>().sqrt()
                })
                .sum(),
            _ => 0.0,
        };
        let quadratic = element.shape.is_quadratic();
        let nodes: Vec<NodeId> = (topology.corners.iter())
            .chain(if quadratic { topology.mids } else { &[] })
            .filter_map(|&local| element.nodes.get(local).copied())
            .collect();
        for node in &nodes {
            *areas.entry(*node).or_insert(0.0) += area / nodes.len() as f64;
        }
    }
    areas
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}
