//! FE-Netz: Knoten, Elemente, Sets, Surfaces, Geometrie-Topologie.

mod cad;
mod contact_search;
mod element;
mod fast_map;
mod jacobian;
mod line_joints;
mod mesh;
mod skin;
mod transform;

pub use cad::{CadEntity, CadMap};
pub use contact_search::{
    GeometryFilter, GroupBy, MasterSlaveItem, SearchParameters, SurfaceId, SurfaceKind,
    find_contact_pairs, surface_faces,
};
pub use element::{ElementFamily, ElementShape, FaceTopology};
pub use line_joints::{LineJoint, find_line_joints};
pub use mesh::{Element, ElementId, FeMesh, Fnv, MeshError, NodeId, Part, SurfaceDefinition};
pub use skin::{PartSkin, SkinEdge, SkinFace, extract_part_skin, face_normal};
pub use transform::{FaceRenumbering, MeshTransform};
