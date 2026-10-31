//! Nodes, sets and surfaces. Elements are in [`crate::element`].

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// A node number as the user wrote it.
pub type NodeId = u32;
/// An element number as the user wrote it.
pub type ElementId = u32;

/// The nodes of the model: user ids and coordinates, densely packed in the order of their
/// definition, with an index from id to position.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Nodes {
    pub ids: Vec<NodeId>,
    pub coordinates: Vec<[f64; 3]>,
    #[serde(skip)]
    index: HashMap<NodeId, u32>,
}

impl Nodes {
    /// Adds a node, or replaces the coordinates of one defined already (CalculiX takes the
    /// last definition). Returns its dense index.
    pub fn insert(&mut self, id: NodeId, coordinates: [f64; 3]) -> u32 {
        if let Some(&i) = self.index.get(&id) {
            self.coordinates[i as usize] = coordinates;
            return i;
        }
        let i = self.ids.len() as u32;
        self.ids.push(id);
        self.coordinates.push(coordinates);
        self.index.insert(id, i);
        i
    }

    /// Dense index of a node id.
    pub fn index(&self, id: NodeId) -> Option<u32> {
        self.index.get(&id).copied()
    }

    pub fn coordinates(&self, id: NodeId) -> Option<[f64; 3]> {
        self.index(id).map(|i| self.coordinates[i as usize])
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Rebuilds the id index, needed after deserializing.
    pub fn rebuild_index(&mut self) {
        self.index = (self.ids.iter().enumerate())
            .map(|(i, &id)| (id, i as u32))
            .collect();
    }

    /// Ids and coordinates together.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, [f64; 3])> + '_ {
        self.ids
            .iter()
            .copied()
            .zip(self.coordinates.iter().copied())
    }
}

/// Node sets, element sets and surfaces by their (upper case) names. Sets keep the order of
/// definition without duplicates; CalculiX's `GENERATE` ranges are expanded when read.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Sets {
    pub node_sets: BTreeMap<String, Vec<NodeId>>,
    pub element_sets: BTreeMap<String, Vec<ElementId>>,
    pub surfaces: BTreeMap<String, Surface>,
}

impl Sets {
    /// Appends ids to a node set, creating it when new; ids already in the set are skipped.
    pub fn add_nodes(&mut self, name: &str, ids: impl IntoIterator<Item = NodeId>) {
        let set = self.node_sets.entry(name.to_string()).or_default();
        append_unique(set, ids);
    }

    pub fn add_elements(&mut self, name: &str, ids: impl IntoIterator<Item = ElementId>) {
        let set = self.element_sets.entry(name.to_string()).or_default();
        append_unique(set, ids);
    }
}

fn append_unique<T: Copy + Ord + std::hash::Hash>(
    set: &mut Vec<T>,
    ids: impl IntoIterator<Item = T>,
) {
    let mut known: std::collections::HashSet<T> = set.iter().copied().collect();
    for id in ids {
        if known.insert(id) {
            set.push(id);
        }
    }
}

/// A surface: element faces (for loads, contact, ties) or nodes (for node-to-surface contact
/// slaves and ties).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Surface {
    Element(Vec<SurfaceFace>),
    Node(Vec<NodeId>),
}

/// One face of an element in a surface: the element and the CalculiX face number (`S1`..`S6`
/// of solids, `S1`/`S2`/`S3`.. of shells and 2D elements, `SPOS`/`SNEG` as `1`/`2`), kept as
/// the number after the `S`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceFace {
    pub element: ElementId,
    pub face: u8,
}

/// Where a boundary condition, load or output applies: a single node or element, or a set
/// by name. Names are upper case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRef {
    Node(NodeId),
    Set(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementRef {
    Element(ElementId),
    Set(String),
}

impl std::fmt::Display for NodeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeRef::Node(id) => write!(f, "{id}"),
            NodeRef::Set(name) => f.write_str(name),
        }
    }
}

impl std::fmt::Display for ElementRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElementRef::Element(id) => write!(f, "{id}"),
            ElementRef::Set(name) => f.write_str(name),
        }
    }
}
