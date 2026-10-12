//! Links between the analyses of a project, like the links between the cells of Ansys
//! Workbench's project schematic, but down to single items of the model tree: an analysis
//! takes the geometry, the mesh, its setup, a category of the setup or one item of it from
//! another analysis, which owns them. The results of a step of one analysis can drive a step
//! of another, the submodel.
//!
//! A path names what is linked, as the block of the analysis lists it: `["Geometry"]`,
//! `["Mesh"]`, `["Setup"]`, `["Setup", "Materials"]`, `["Setup", "Materials", "Steel"]` or
//! `["Results", "Step-1"]`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use plx_io::project::Link;
use plx_mesh::{FeMesh, Fnv};
use plx_model::{BoundaryKind, FeModel, Geometry, ModelKind};

use crate::tree::TreeItem;

pub const GEOMETRY: &str = "Geometry";
pub const MESH: &str = "Mesh";
pub const SETUP: &str = "Setup";
pub const RESULTS: &str = "Results";

/// The categories of the setup in the order of the FE Model tree; each can be linked as a
/// whole or item by item.
pub const CATEGORIES: [&str; 8] = [
    "Features",
    "Materials",
    "Sections",
    "Constraints",
    "Contacts",
    "Amplitudes",
    "Initial Conditions",
    "Steps",
];

/// What an analysis has that links read: the data of a closed analysis or of an open window.
#[derive(Clone, Copy)]
pub struct Data<'a> {
    pub geometry: Option<&'a Geometry>,
    pub mesh: &'a FeMesh,
    pub fe: &'a FeModel,
    pub mesh_inputs: &'a BTreeMap<String, u64>,
}

/// A mesh with what belongs to it, taken over by a link of the mesh.
pub struct MeshCopy {
    /// The geometry it was meshed from.
    pub geometry: Option<Geometry>,
    pub mesh: FeMesh,
    pub mesh_inputs: BTreeMap<String, u64>,
    pub mesh_sets: Vec<plx_model::MeshSet>,
    pub element_types: Vec<plx_model::PartElementType>,
}

impl MeshCopy {
    pub fn of(data: Data) -> Self {
        Self {
            geometry: data.geometry.cloned(),
            mesh: data.mesh.clone(),
            mesh_inputs: data.mesh_inputs.clone(),
            mesh_sets: data.fe.mesh_sets.clone(),
            element_types: data.fe.element_types.clone(),
        }
    }
}

/// The path as text, as the output pane names it: "Setup > Materials > Steel".
pub fn label(path: &[String]) -> String {
    path.join(" > ")
}

/// Whether `path` is `prefix` or lies inside it.
pub fn covers(prefix: &[String], path: &[String]) -> bool {
    path.len() >= prefix.len() && path[..prefix.len()] == *prefix
}

/// Where a link puts what it takes in the target.
pub fn slot(link: &Link) -> &[String] {
    if link.slot.is_empty() {
        &link.item
    } else {
        &link.slot
    }
}

/// Whether the link carries the results of a step, for a submodel.
pub fn is_results(link: &Link) -> bool {
    link.item.first().is_some_and(|c| c == RESULTS)
}

/// The names of the items of a category of the setup, as the tree lists them.
pub fn entries(fe: &FeModel, category: &str) -> Vec<String> {
    let names = |names: &mut dyn Iterator<Item = &str>| names.map(str::to_owned).collect();
    match category {
        "Features" => names(
            &mut (fe.reference_points.iter().map(|p| p.name.as_str()))
                .chain(fe.coordinate_systems.iter().map(|c| c.name.as_str()))
                .chain(fe.planes.iter().map(|p| p.name.as_str())),
        ),
        "Materials" => names(&mut fe.materials.iter().map(|m| m.name.as_str())),
        "Sections" => names(&mut fe.sections.iter().map(|s| s.name.as_str())),
        "Constraints" => names(&mut fe.constraints.iter().map(|c| c.name())),
        "Contacts" => names(
            &mut (fe.surface_interactions.iter().map(|s| s.name.as_str()))
                .chain(fe.contact_pairs.iter().map(|c| c.name.as_str()))
                .chain(fe.ties.iter().map(|t| t.name.as_str()))
                .chain(fe.node_ties.iter().map(|t| t.name.as_str())),
        ),
        "Amplitudes" => names(&mut fe.amplitudes.iter().map(|a| a.name.as_str())),
        "Initial Conditions" => names(&mut fe.initial_conditions.iter().map(|c| c.name.as_str())),
        "Steps" => names(&mut fe.steps.iter().map(|s| s.name.as_str())),
        _ => Vec::new(),
    }
}

/// Whether the analysis has what the path names, so that a link can take it.
pub fn exists(data: Data, path: &[String]) -> bool {
    match path {
        [cell] if cell == GEOMETRY => data.geometry.is_some(),
        [cell] if cell == MESH => data.mesh.element_count() > 0,
        [cell] if cell == SETUP => true,
        [cell, category] if cell == SETUP => CATEGORIES.contains(&category.as_str()),
        [cell, category, name] if cell == SETUP => entries(data.fe, category).contains(name),
        [cell, step] if cell == RESULTS => data.fe.steps.iter().any(|s| &s.name == step),
        _ => false,
    }
}

/// Fingerprint of what the path takes from the analysis: a link copies it anew when the
/// source's differs from what the target has, or had last time for the mesh and geometry.
pub fn fingerprint(data: Data, path: &[String]) -> u64 {
    let mut hash = Fnv::new();
    match path {
        [cell] if cell == GEOMETRY => {
            if let Some(geometry) = data.geometry {
                hash.add_str(&geometry.source);
                hash.add_str(&geometry.brep);
                hash.add_debug(&geometry.meshing);
                hash.add_debug(&geometry.mesh_items);
                hash.add_debug(&geometry.part_names);
            }
        }
        [cell] if cell == MESH => {
            hash.add(&data.mesh.input_fingerprint().to_le_bytes());
            hash.add_debug(data.mesh_inputs);
            hash.add_debug(&data.fe.mesh_sets);
            hash.add_debug(&data.fe.element_types);
        }
        [cell, rest @ ..] if cell == SETUP => setup_fingerprint(&mut hash, data.fe, rest),
        _ => {}
    }
    hash.finish()
}

fn setup_fingerprint(hash: &mut Fnv, fe: &FeModel, path: &[String]) {
    match path {
        [] => {
            for category in CATEGORIES {
                category_fingerprint(hash, fe, category, None);
            }
            hash.add_debug(&fe.user_keywords);
        }
        [category] => category_fingerprint(hash, fe, category, None),
        [category, name] => category_fingerprint(hash, fe, category, Some(name)),
        _ => {}
    }
}

/// Hashes the items of a category, or only the one named. The global step of a submodel's
/// boundary conditions is left out: a results link sets it in the target.
fn category_fingerprint(hash: &mut Fnv, fe: &FeModel, category: &str, name: Option<&str>) {
    fn add<T: std::fmt::Debug>(
        hash: &mut Fnv,
        items: &[T],
        key: fn(&T) -> &str,
        name: Option<&str>,
    ) {
        for item in items.iter().filter(|i| name.is_none_or(|n| key(i) == n)) {
            hash.add_debug(item);
        }
    }
    hash.add_str(category);
    match category {
        "Features" => {
            add(hash, &fe.reference_points, |p| &p.name, name);
            add(hash, &fe.coordinate_systems, |c| &c.name, name);
            add(hash, &fe.planes, |p| &p.name, name);
        }
        "Materials" => add(hash, &fe.materials, |m| &m.name, name),
        "Sections" => add(hash, &fe.sections, |s| &s.name, name),
        "Constraints" => add(hash, &fe.constraints, |c| c.name(), name),
        "Contacts" => {
            add(hash, &fe.surface_interactions, |s| &s.name, name);
            add(hash, &fe.contact_pairs, |c| &c.name, name);
            add(hash, &fe.ties, |t| &t.name, name);
            add(hash, &fe.node_ties, |t| &t.name, name);
        }
        "Amplitudes" => add(hash, &fe.amplitudes, |a| &a.name, name),
        "Initial Conditions" => add(hash, &fe.initial_conditions, |c| &c.name, name),
        "Steps" => {
            let mut steps: Vec<_> = (fe.steps.iter())
                .filter(|s| name.is_none_or(|n| s.name == n))
                .cloned()
                .collect();
            for bc in steps.iter_mut().flat_map(|s| &mut s.boundary_conditions) {
                if let BoundaryKind::Submodel { step, .. } = &mut bc.kind {
                    *step = 0;
                }
            }
            hash.add_debug(&steps);
        }
        _ => {}
    }
}

/// Copies what the path names from `source` into `target`: a category replaces the
/// target's, an item replaces the target's item of the same name or is added.
pub fn apply_setup(target: &mut FeModel, source: &FeModel, path: &[String]) {
    match path {
        [cell] if cell == SETUP => {
            for category in CATEGORIES {
                copy_category(target, source, category, None);
            }
            target.user_keywords = source.user_keywords.clone();
        }
        [cell, category] if cell == SETUP => copy_category(target, source, category, None),
        [cell, category, name] if cell == SETUP => {
            copy_category(target, source, category, Some(name))
        }
        _ => {}
    }
}

fn copy_category(target: &mut FeModel, source: &FeModel, category: &str, name: Option<&str>) {
    fn copy<T: Clone>(target: &mut Vec<T>, source: &[T], key: fn(&T) -> &str, name: Option<&str>) {
        let Some(name) = name else {
            *target = source.to_vec();
            return;
        };
        let Some(item) = source.iter().find(|i| key(i) == name) else {
            return;
        };
        match target.iter_mut().find(|i| key(i) == name) {
            Some(own) => *own = item.clone(),
            None => target.push(item.clone()),
        }
    }
    // The global steps of the target's submodel boundary conditions stay as a results link
    // set them.
    let global_steps = submodel_steps(target);
    match category {
        "Features" => {
            copy(
                &mut target.reference_points,
                &source.reference_points,
                |p| &p.name,
                name,
            );
            copy(
                &mut target.coordinate_systems,
                &source.coordinate_systems,
                |c| &c.name,
                name,
            );
            copy(&mut target.planes, &source.planes, |p| &p.name, name);
        }
        "Materials" => copy(&mut target.materials, &source.materials, |m| &m.name, name),
        "Sections" => copy(&mut target.sections, &source.sections, |s| &s.name, name),
        "Constraints" => copy(
            &mut target.constraints,
            &source.constraints,
            |c| c.name(),
            name,
        ),
        "Contacts" => {
            copy(
                &mut target.surface_interactions,
                &source.surface_interactions,
                |s| &s.name,
                name,
            );
            copy(
                &mut target.contact_pairs,
                &source.contact_pairs,
                |c| &c.name,
                name,
            );
            copy(&mut target.ties, &source.ties, |t| &t.name, name);
            copy(&mut target.node_ties, &source.node_ties, |t| &t.name, name);
        }
        "Amplitudes" => copy(
            &mut target.amplitudes,
            &source.amplitudes,
            |a| &a.name,
            name,
        ),
        "Initial Conditions" => copy(
            &mut target.initial_conditions,
            &source.initial_conditions,
            |c| &c.name,
            name,
        ),
        "Steps" => {
            copy(&mut target.steps, &source.steps, |s| &s.name, name);
            for (step, global) in global_steps {
                set_global_step(target, &step, global);
            }
        }
        _ => {}
    }
}

/// The global step each step's submodel boundary conditions read, by step name.
fn submodel_steps(fe: &FeModel) -> Vec<(String, u32)> {
    (fe.steps.iter())
        .filter_map(|step| {
            (step.boundary_conditions.iter()).find_map(|bc| match bc.kind {
                BoundaryKind::Submodel { step: global, .. } => Some((step.name.clone(), global)),
                _ => None,
            })
        })
        .collect()
}

fn set_global_step(fe: &mut FeModel, step: &str, global: u32) -> bool {
    let mut changed = false;
    let Some(step) = fe.steps.iter_mut().find(|s| s.name == step) else {
        return false;
    };
    for bc in &mut step.boundary_conditions {
        if let BoundaryKind::Submodel { step, .. } = &mut bc.kind
            && *step != global
        {
            *step = global;
            changed = true;
        }
    }
    changed
}

/// The number CalculiX gives a step of the global model in its results: the active steps
/// counted from 1.
pub fn global_step_number(fe: &FeModel, step: &str) -> Option<u32> {
    let index = (fe.steps.iter().filter(|s| s.active)).position(|s| s.name == step)?;
    u32::try_from(index + 1).ok()
}

/// Makes `fe` a submodel whose step `step` reads the results of global step `global` from
/// `results`; returns whether anything changed.
pub fn apply_submodel(fe: &mut FeModel, step: &str, global: u32, results: PathBuf) -> bool {
    let mut changed = false;
    if fe.properties.kind != ModelKind::Submodel {
        fe.properties.kind = ModelKind::Submodel;
        changed = true;
    }
    if fe.properties.global_results.as_ref() != Some(&results) {
        fe.properties.global_results = Some(results);
        changed = true;
    }
    set_global_step(fe, step, global) || changed
}

/// Makes `fe` an ordinary model again, once its last results link is gone.
pub fn clear_submodel(fe: &mut FeModel) {
    fe.properties.kind = ModelKind::General;
    fe.properties.global_results = None;
}

/// Whether a link from `item` of one analysis may go to `slot` of another: like with like,
/// and the results of a step only to a step.
pub fn compatible(item: &[String], slot: &[String]) -> bool {
    match (item, slot) {
        ([a], [b]) => a == b && a != RESULTS,
        ([a, _], [b, _]) if a == RESULTS && b == RESULTS => true,
        ([a, ..], [b, ..]) if a == SETUP && b == SETUP => {
            // A category or an item goes to the target's category, or into its setup.
            match (item.get(1), slot.get(1)) {
                (None, None) => true,
                (Some(_), None) => false,
                (None, Some(_)) => false,
                (Some(c), Some(d)) => c == d,
            }
        }
        _ => false,
    }
}

/// The analyses that come before `target` through links, nearest first.
pub fn upstream(links: &[Link], target: u64) -> Vec<u64> {
    let mut found = Vec::new();
    let mut queue = vec![target];
    while let Some(id) = queue.pop() {
        for link in links.iter().filter(|l| l.target == id) {
            if !found.contains(&link.source) && link.source != target {
                found.push(link.source);
                queue.push(link.source);
            }
        }
    }
    found
}

/// Whether a link from `source` to `target` would make a loop.
pub fn makes_cycle(links: &[Link], source: u64, target: u64) -> bool {
    source == target || upstream(links, source).contains(&target)
}

/// The analyses in an order in which every source comes before its targets.
pub fn order(ids: &[u64], links: &[Link]) -> Vec<u64> {
    let mut done: Vec<u64> = Vec::with_capacity(ids.len());
    let mut left: Vec<u64> = ids.to_vec();
    while !left.is_empty() {
        let before = left.len();
        left.retain(|&id| {
            let ready = (links.iter())
                .filter(|l| l.target == id && ids.contains(&l.source))
                .all(|l| done.contains(&l.source));
            if ready {
                done.push(id);
            }
            !ready
        });
        if left.len() == before {
            // A loop, which links never make; take the rest as they are.
            done.append(&mut left);
        }
    }
    done
}

/// What of a model the tree shows for a path: the category and its items, or the item.
pub fn tree_items(fe: &FeModel, path: &[String]) -> Vec<TreeItem> {
    let all = |category: &str| -> Vec<TreeItem> {
        let mut items = category_items(fe, category, None);
        items.extend(category_groups(category));
        items
    };
    match path {
        [cell] if cell == MESH => vec![TreeItem::Mesh],
        [cell] if cell == SETUP => CATEGORIES.iter().flat_map(|c| all(c)).collect(),
        [cell, category] if cell == SETUP => all(category),
        [cell, category, name] if cell == SETUP => category_items(fe, category, Some(name)),
        _ => Vec::new(),
    }
}

fn category_groups(category: &str) -> Vec<TreeItem> {
    match category {
        "Contacts" => vec![
            TreeItem::Group("Contacts"),
            TreeItem::Group("Surface Interactions"),
            TreeItem::Group("Contact Pairs"),
        ],
        "Features" => vec![TreeItem::Group("Features")],
        other => (CATEGORIES.iter())
            .find(|c| **c == other)
            .map(|c| vec![TreeItem::Group(c)])
            .unwrap_or_default(),
    }
}

fn category_items(fe: &FeModel, category: &str, name: Option<&str>) -> Vec<TreeItem> {
    let wanted = |n: &str| name.is_none_or(|m| m == n);
    let mut items = Vec::new();
    match category {
        "Features" => {
            for (i, p) in fe.reference_points.iter().enumerate() {
                if wanted(&p.name) {
                    items.push(TreeItem::ReferencePoint(i));
                }
            }
            for (i, c) in fe.coordinate_systems.iter().enumerate() {
                if wanted(&c.name) {
                    items.push(TreeItem::CoordinateSystem(i));
                }
            }
            for (i, p) in fe.planes.iter().enumerate() {
                if wanted(&p.name) {
                    items.push(TreeItem::Plane(i));
                }
            }
        }
        "Materials" => items.extend(
            (fe.materials.iter().enumerate())
                .filter(|(_, m)| wanted(&m.name))
                .map(|(i, _)| TreeItem::Material(i)),
        ),
        "Sections" => items.extend(
            (fe.sections.iter().enumerate())
                .filter(|(_, s)| wanted(&s.name))
                .map(|(i, _)| TreeItem::Section(i)),
        ),
        "Constraints" => items.extend(
            (fe.constraints.iter().enumerate())
                .filter(|(_, c)| wanted(c.name()))
                .map(|(i, _)| TreeItem::Constraint(i)),
        ),
        "Contacts" => {
            items.extend(
                (fe.surface_interactions.iter().enumerate())
                    .filter(|(_, s)| wanted(&s.name))
                    .map(|(i, _)| TreeItem::SurfaceInteraction(i)),
            );
            items.extend(
                (fe.contact_pairs.iter().enumerate())
                    .filter(|(_, c)| wanted(&c.name))
                    .map(|(i, _)| TreeItem::ContactPair(i)),
            );
            items.extend(
                (fe.ties.iter().enumerate())
                    .filter(|(_, t)| wanted(&t.name))
                    .map(|(i, _)| TreeItem::Tie(i)),
            );
            items.extend(
                (fe.node_ties.iter().enumerate())
                    .filter(|(_, t)| wanted(&t.name))
                    .map(|(i, _)| TreeItem::NodeTie(i)),
            );
        }
        "Amplitudes" => items.extend(
            (fe.amplitudes.iter().enumerate())
                .filter(|(_, a)| wanted(&a.name))
                .map(|(i, _)| TreeItem::Amplitude(i)),
        ),
        "Initial Conditions" => items.extend(
            (fe.initial_conditions.iter().enumerate())
                .filter(|(_, c)| wanted(&c.name))
                .map(|(i, _)| TreeItem::InitialCondition(i)),
        ),
        "Steps" => {
            for (s, step) in fe.steps.iter().enumerate().filter(|(_, s)| wanted(&s.name)) {
                items.push(TreeItem::Step(s));
                for group in [
                    "BCs",
                    "Loads",
                    "Field Outputs",
                    "History Outputs",
                    "Defined Fields",
                ] {
                    items.push(TreeItem::StepGroup(s, group));
                }
                let n = step.boundary_conditions.len();
                items.extend((0..n).map(|i| TreeItem::BoundaryCondition(s, i)));
                items.extend((0..step.loads.len()).map(|i| TreeItem::Load(s, i)));
                items.extend((0..step.field_outputs.len()).map(|i| TreeItem::FieldOutput(s, i)));
                let n = step.history_outputs.len();
                items.extend((0..n).map(|i| TreeItem::HistoryOutput(s, i)));
                let n = step.defined_fields.len();
                items.extend((0..n).map(|i| TreeItem::DefinedField(s, i)));
            }
        }
        _ => {}
    }
    items
}

/// The path a tree item belongs to, to find whether a link covers it.
pub fn path_of(fe: &FeModel, item: &TreeItem) -> Option<Vec<String>> {
    let entry = |category: &str, name: &str| {
        Some(vec![SETUP.to_owned(), category.to_owned(), name.to_owned()])
    };
    let category = |category: &str| Some(vec![SETUP.to_owned(), category.to_owned()]);
    match item {
        TreeItem::Mesh
        | TreeItem::Part(_)
        | TreeItem::NodeSet(_)
        | TreeItem::ElementSet(_)
        | TreeItem::Surface(_) => Some(vec![MESH.to_owned()]),
        TreeItem::MeshItem(_) => Some(vec![GEOMETRY.to_owned()]),
        TreeItem::Material(i) => entry("Materials", &fe.materials.get(*i)?.name),
        TreeItem::Section(i) => entry("Sections", &fe.sections.get(*i)?.name),
        TreeItem::Constraint(i) => entry("Constraints", fe.constraints.get(*i)?.name()),
        TreeItem::SurfaceInteraction(i) => {
            entry("Contacts", &fe.surface_interactions.get(*i)?.name)
        }
        TreeItem::ContactPair(i) => entry("Contacts", &fe.contact_pairs.get(*i)?.name),
        TreeItem::Tie(i) => entry("Contacts", &fe.ties.get(*i)?.name),
        TreeItem::NodeTie(i) => entry("Contacts", &fe.node_ties.get(*i)?.name),
        TreeItem::Amplitude(i) => entry("Amplitudes", &fe.amplitudes.get(*i)?.name),
        TreeItem::InitialCondition(i) => {
            entry("Initial Conditions", &fe.initial_conditions.get(*i)?.name)
        }
        TreeItem::ReferencePoint(i) => entry("Features", &fe.reference_points.get(*i)?.name),
        TreeItem::CoordinateSystem(i) => entry("Features", &fe.coordinate_systems.get(*i)?.name),
        TreeItem::Plane(i) => entry("Features", &fe.planes.get(*i)?.name),
        TreeItem::Step(s)
        | TreeItem::StepGroup(s, _)
        | TreeItem::BoundaryCondition(s, _)
        | TreeItem::Load(s, _)
        | TreeItem::FieldOutput(s, _)
        | TreeItem::HistoryOutput(s, _)
        | TreeItem::DefinedField(s, _) => entry("Steps", &fe.steps.get(*s)?.name),
        TreeItem::Group("Surface Interactions" | "Contact Pairs" | "Contacts") => {
            category("Contacts")
        }
        TreeItem::Group(name) if CATEGORIES.contains(name) => category(name),
        _ => None,
    }
}

/// The paths of a model's tree that links of other analyses fill, with the name of the
/// analysis each comes from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Incoming {
    pub paths: Vec<(Vec<String>, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use plx_model::{BoundaryCondition, Material, Region, Step};

    fn path(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn fp(fe: &FeModel, path: &[String]) -> u64 {
        let (mesh, inputs) = (FeMesh::default(), BTreeMap::new());
        let data = Data {
            geometry: None,
            mesh: &mesh,
            fe,
            mesh_inputs: &inputs,
        };
        fingerprint(data, path)
    }

    fn material(name: &str, young: f64) -> Material {
        Material {
            name: name.into(),
            elastic: Some(plx_model::Elastic {
                young,
                poisson: 0.3,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn an_item_replaces_the_one_of_the_same_name_and_leaves_the_others() {
        let source = FeModel {
            materials: vec![
                material("Steel", 210_000.0),
                material("Aluminium", 70_000.0),
            ],
            ..Default::default()
        };
        let mut target = FeModel {
            materials: vec![material("Steel", 1.0), material("Rubber", 5.0)],
            ..Default::default()
        };
        let steel = path(&["Setup", "Materials", "Steel"]);
        assert_ne!(fp(&source, &steel), fp(&target, &steel));
        apply_setup(&mut target, &source, &steel);
        let names: Vec<&str> = target.materials.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Steel", "Rubber"]);
        assert_eq!(fp(&source, &steel), fp(&target, &steel));
        apply_setup(&mut target, &source, &path(&["Setup", "Materials"]));
        assert_eq!(target.materials, source.materials);
    }

    #[test]
    fn steps_keep_the_global_step_a_results_link_set() {
        let mut step = Step::new_static("Step-1");
        step.boundary_conditions.push(BoundaryCondition {
            name: "Submodel-1".into(),
            active: true,
            visible: true,
            region: Region::NodeSet("CUT".into()),
            kind: BoundaryKind::Submodel {
                step: 1,
                dofs: [true; 6],
            },
            amplitude: None,
        });
        let mut source = FeModel::default();
        source.steps.push(step);
        let mut target = source.clone();
        assert!(apply_submodel(
            &mut target,
            "Step-1",
            2,
            "global.frd".into()
        ));
        assert_eq!(target.properties.kind, ModelKind::Submodel);
        // The steps still count as the same, and taking them anew keeps global step 2.
        let steps = path(&["Setup", "Steps"]);
        assert_eq!(fp(&source, &steps), fp(&target, &steps));
        apply_setup(&mut target, &source, &steps);
        assert_eq!(submodel_steps(&target), [("Step-1".to_string(), 2)]);
        assert!(!apply_submodel(
            &mut target,
            "Step-1",
            2,
            "global.frd".into()
        ));
    }

    #[test]
    fn global_steps_count_the_active_steps() {
        let mut fe = FeModel::default();
        for name in ["Preload", "Off", "Service"] {
            fe.steps.push(Step::new_static(name));
        }
        fe.steps[1].active = false;
        assert_eq!(global_step_number(&fe, "Preload"), Some(1));
        assert_eq!(global_step_number(&fe, "Service"), Some(2));
        assert_eq!(global_step_number(&fe, "Off"), None);
    }

    #[test]
    fn links_never_make_a_loop_and_sources_come_first() {
        let link = |source, target| Link {
            source,
            target,
            item: path(&["Mesh"]),
            slot: Vec::new(),
        };
        let links = vec![link(1, 2), link(2, 3)];
        assert!(makes_cycle(&links, 3, 1));
        assert!(makes_cycle(&links, 2, 2));
        assert!(!makes_cycle(&links, 1, 3));
        assert_eq!(order(&[3, 2, 1], &links), [1, 2, 3]);
        assert_eq!(upstream(&links, 3), [2, 1]);
    }

    #[test]
    fn like_goes_with_like() {
        assert!(compatible(&path(&["Mesh"]), &path(&["Mesh"])));
        assert!(!compatible(&path(&["Mesh"]), &path(&["Geometry"])));
        let steel = path(&["Setup", "Materials", "Steel"]);
        assert!(compatible(&steel, &path(&["Setup", "Materials"])));
        assert!(compatible(&steel, &path(&["Setup", "Materials", "Rubber"])));
        assert!(!compatible(&steel, &path(&["Setup", "Sections"])));
        assert!(!compatible(&steel, &path(&["Setup"])));
        assert!(compatible(&path(&["Setup"]), &path(&["Setup"])));
        let results = path(&["Results", "Step-1"]);
        assert!(compatible(&results, &path(&["Results", "Step-2"])));
        assert!(!compatible(&results, &path(&["Setup", "Steps"])));
    }
}
