//! Meshes a mesh block of the project schematic makes from a few parameters: a box of
//! hexahedra for 3D models and a rectangle of quadrilaterals for 2D ones, see
//! [`plx_mesh::structured`].

use plx_mesh::{FeMesh, Fnv};
use serde::{Deserialize, Serialize};

use crate::ModelSpace;

/// The shape and division of a structured mesh, in SI units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MeshPrimitive {
    /// A box from `origin` with edge lengths `size`, `divisions` elements along each axis.
    Box {
        origin: [f64; 3],
        size: [f64; 3],
        divisions: [u32; 3],
        /// One of [`MeshPrimitive::BOX_ELEMENTS`].
        element: String,
    },
    /// A rectangle in the X-Y plane; its element type follows from the model space.
    Rectangle {
        origin: [f64; 2],
        size: [f64; 2],
        divisions: [u32; 2],
        /// 8-node rather than 4-node quadrilaterals.
        quadratic: bool,
        /// Reduced integration.
        reduced: bool,
    },
}

/// Most elements a mesh block makes, so that a typing slip does not freeze the program.
pub const MAX_PRIMITIVE_ELEMENTS: u64 = 2_000_000;

impl MeshPrimitive {
    /// The element types of a box: linear and quadratic hexahedra.
    pub const BOX_ELEMENTS: [&'static str; 5] = ["C3D8", "C3D8R", "C3D8I", "C3D20", "C3D20R"];

    /// A box of 100 x 20 x 20 mm, like a cantilever.
    pub fn default_box() -> Self {
        Self::Box {
            origin: [0.0; 3],
            size: [0.1, 0.02, 0.02],
            divisions: [10, 2, 2],
            element: "C3D20R".into(),
        }
    }

    /// A rectangle of 100 x 20 mm.
    pub fn default_rectangle() -> Self {
        Self::Rectangle {
            origin: [0.0; 2],
            size: [0.1, 0.02],
            divisions: [10, 2],
            quadratic: true,
            reduced: true,
        }
    }

    /// Its kind in the GUI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Box { .. } => "Box",
            Self::Rectangle { .. } => "Rectangle",
        }
    }

    /// The model spaces it is made for.
    pub fn fits(&self, space: ModelSpace) -> bool {
        matches!(self, Self::Box { .. }) != space.is_2d()
    }

    /// The CalculiX element type of its elements in the model space.
    pub fn element_type(&self, space: ModelSpace) -> String {
        match self {
            Self::Box { element, .. } => element.clone(),
            Self::Rectangle {
                quadratic, reduced, ..
            } => {
                let family = match space {
                    ModelSpace::PlaneStrain => "CPE",
                    ModelSpace::Axisymmetric => "CAX",
                    _ => "CPS",
                };
                let nodes = if *quadratic { 8 } else { 4 };
                let reduced = if *reduced { "R" } else { "" };
                format!("{family}{nodes}{reduced}")
            }
        }
    }

    /// How many elements it makes.
    pub fn element_count(&self) -> u64 {
        match self {
            Self::Box { divisions, .. } => divisions.iter().map(|&n| u64::from(n.max(1))).product(),
            Self::Rectangle { divisions, .. } => {
                divisions.iter().map(|&n| u64::from(n.max(1))).product()
            }
        }
    }

    /// Why it cannot be made in the model space, if it cannot.
    pub fn problem(&self, space: ModelSpace) -> Option<String> {
        let (origin, size, divisions): (&[f64], &[f64], &[u32]) = match self {
            Self::Box {
                origin,
                size,
                divisions,
                element,
            } => {
                if !Self::BOX_ELEMENTS.contains(&element.as_str()) {
                    return Some(format!("{element} is no element type of a box."));
                }
                (origin, size, divisions)
            }
            Self::Rectangle {
                origin,
                size,
                divisions,
                ..
            } => (origin, size, divisions),
        };
        if !self.fits(space) {
            return Some(format!("A {} needs another model space.", self.label()));
        }
        if size.iter().any(|&s| !(s.is_finite() && s > 0.0)) {
            return Some("The edge lengths must be greater than 0.".into());
        }
        if origin.iter().any(|o| !o.is_finite()) {
            return Some("The origin must be a number.".into());
        }
        if divisions.contains(&0) {
            return Some("Each edge needs at least one element.".into());
        }
        if self.element_count() > MAX_PRIMITIVE_ELEMENTS {
            return Some(format!(
                "{} elements are too many; at most {MAX_PRIMITIVE_ELEMENTS} are made.",
                self.element_count()
            ));
        }
        if space == ModelSpace::Axisymmetric && origin[0] < 0.0 {
            return Some("An axisymmetric model lies at X >= 0: X is the radius.".into());
        }
        None
    }

    /// The mesh, its elements in one part named `part`.
    pub fn generate(&self, space: ModelSpace, part: &str) -> FeMesh {
        let element = self.element_type(space);
        match self {
            Self::Box {
                origin,
                size,
                divisions,
                ..
            } => {
                let quadratic = element.starts_with("C3D20");
                plx_mesh::structured::box_mesh(
                    *origin, *size, *divisions, &element, quadratic, part,
                )
            }
            Self::Rectangle {
                origin,
                size,
                divisions,
                quadratic,
                ..
            } => plx_mesh::structured::rectangle_mesh(
                *origin, *size, *divisions, &element, *quadratic, part,
            ),
        }
    }

    /// Fingerprint of what the mesh is made from: it is out of date once this differs from
    /// the one it was made with.
    pub fn fingerprint(&self, space: ModelSpace) -> u64 {
        let mut hash = Fnv::new();
        hash.add_debug(self);
        hash.add_debug(&space);
        hash.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_take_the_element_family_of_the_model_space() {
        let rectangle = MeshPrimitive::default_rectangle();
        assert_eq!(rectangle.element_type(ModelSpace::PlaneStress), "CPS8R");
        assert_eq!(rectangle.element_type(ModelSpace::Axisymmetric), "CAX8R");
        let linear = MeshPrimitive::Rectangle {
            origin: [0.0; 2],
            size: [1.0; 2],
            divisions: [1, 1],
            quadratic: false,
            reduced: false,
        };
        assert_eq!(linear.element_type(ModelSpace::PlaneStrain), "CPE4");
        assert!(rectangle.problem(ModelSpace::ThreeD).is_some());
        assert!(rectangle.problem(ModelSpace::PlaneStrain).is_none());
        let mesh = rectangle.generate(ModelSpace::PlaneStrain, "Rectangle");
        assert_eq!(mesh.element_count(), 20);
        assert_eq!(mesh.elements()[0].type_name, "CPE8R");
    }

    #[test]
    fn boxes_are_checked_before_they_are_made() {
        let good = MeshPrimitive::default_box();
        assert_eq!(good.problem(ModelSpace::ThreeD), None);
        assert!(good.problem(ModelSpace::PlaneStress).is_some());
        let mesh = good.generate(ModelSpace::ThreeD, "Box");
        assert_eq!(mesh.element_count(), 40);
        assert_eq!(mesh.elements()[0].type_name, "C3D20R");
        assert_eq!(mesh.elements()[0].nodes.len(), 20);
        let block = |size: [f64; 3], divisions: [u32; 3]| MeshPrimitive::Box {
            origin: [0.0; 3],
            size,
            divisions,
            element: "C3D8".into(),
        };
        assert!(
            block([1.0, 0.0, 1.0], [1; 3])
                .problem(ModelSpace::ThreeD)
                .is_some()
        );
        let huge = block([1.0; 3], [1000; 3]);
        assert!(huge.problem(ModelSpace::ThreeD).is_some());
        assert!(
            block([1.0; 3], [1; 3])
                .problem(ModelSpace::ThreeD)
                .is_none()
        );
        let changed = block([0.1, 0.02, 0.02], [10, 2, 2]);
        let space = ModelSpace::ThreeD;
        assert_ne!(good.fingerprint(space), changed.fingerprint(space));
        assert_eq!(good.fingerprint(space), good.clone().fingerprint(space));
    }
}
