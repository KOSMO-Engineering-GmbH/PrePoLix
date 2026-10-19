//! Meshes a mesh block of the project schematic makes from a few parameters: a box of
//! hexahedra for 3D models and a rectangle of quadrilaterals for 2D ones, see
//! [`plx_mesh::structured`].

use plx_mesh::{FeMesh, Fnv};
use serde::{Deserialize, Serialize};

use crate::{ModelSpace, ValueKind};

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

/// A number a mesh block is made from: a length, stored in SI units, or a count of elements.
/// Each is a row of the block in the project schematic, which a numeric input can later feed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamValue {
    Length(f64),
    Count(u32),
}

/// A parameter of a [`MeshPrimitive`]: its key, which stays the same for the project file and
/// links, and its label in the GUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Param {
    pub key: &'static str,
    pub label: &'static str,
    pub tip: &'static str,
}

const fn param(key: &'static str, label: &'static str, tip: &'static str) -> Param {
    Param { key, label, tip }
}

const BOX_PARAMS: [Param; 9] = [
    param("x", "X", "X of the corner the box starts at"),
    param("y", "Y", "Y of the corner the box starts at"),
    param("z", "Z", "Z of the corner the box starts at"),
    param("lx", "Lx", "Edge length along X"),
    param("ly", "Ly", "Edge length along Y"),
    param("lz", "Lz", "Edge length along Z"),
    param("nx", "nx", "Elements along X"),
    param("ny", "ny", "Elements along Y"),
    param("nz", "nz", "Elements along Z"),
];

const RECTANGLE_PARAMS: [Param; 6] = [
    param("x", "X", "X of the corner the rectangle starts at"),
    param("y", "Y", "Y of the corner the rectangle starts at"),
    param("lx", "Lx", "Edge length along X"),
    param("ly", "Ly", "Edge length along Y"),
    param("nx", "nx", "Elements along X"),
    param("ny", "ny", "Elements along Y"),
];

/// Where a parameter's number is held, by its kind: a length in SI units or a count.
pub enum ParamSlot<'a> {
    Length(&'a mut f64),
    Count(&'a mut u32),
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

    /// Its numeric parameters, in the order its block shows them.
    pub fn params(&self) -> &'static [Param] {
        match self {
            Self::Box { .. } => &BOX_PARAMS,
            Self::Rectangle { .. } => &RECTANGLE_PARAMS,
        }
    }

    /// The parameter of the key and the number it holds.
    pub fn slot(&mut self, key: &str) -> Option<ParamSlot<'_>> {
        let (origin, size, divisions): (&mut [f64], &mut [f64], &mut [u32]) = match self {
            Self::Box {
                origin,
                size,
                divisions,
                ..
            } => (origin, size, divisions),
            Self::Rectangle {
                origin,
                size,
                divisions,
                ..
            } => (origin, size, divisions),
        };
        let axis = |name: &str| ["x", "y", "z"].iter().position(|a| *a == name);
        if let Some(rest) = key.strip_prefix('l') {
            return size.get_mut(axis(rest)?).map(ParamSlot::Length);
        }
        if let Some(rest) = key.strip_prefix('n') {
            return divisions.get_mut(axis(rest)?).map(ParamSlot::Count);
        }
        origin.get_mut(axis(key)?).map(ParamSlot::Length)
    }

    /// The value of the parameter of the key.
    pub fn get(&self, key: &str) -> Option<ParamValue> {
        self.clone().slot(key).map(|slot| match slot {
            ParamSlot::Length(value) => ParamValue::Length(*value),
            ParamSlot::Count(value) => ParamValue::Count(*value),
        })
    }

    /// Sets the parameter of the key; returns whether it has one of that kind.
    pub fn set(&mut self, key: &str, value: ParamValue) -> bool {
        match (self.slot(key), value) {
            (Some(ParamSlot::Length(slot)), ParamValue::Length(value)) => *slot = value,
            (Some(ParamSlot::Count(slot)), ParamValue::Count(value)) => *slot = value,
            _ => return false,
        }
        true
    }

    /// What the parameter of the key is, for a value link to it.
    pub fn value_kind(&self, key: &str) -> Option<ValueKind> {
        match self.get(key)? {
            ParamValue::Length(_) => Some(ValueKind::LENGTH),
            ParamValue::Count(_) => Some(ValueKind::Count),
        }
    }

    /// The parameter of the key as a number, a length in SI units.
    pub fn value(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            ParamValue::Length(value) => Some(value),
            ParamValue::Count(value) => Some(f64::from(value)),
        }
    }

    /// Sets the parameter of the key to a number from a value link: a count is rounded to a
    /// whole number of at least one. Returns whether it changed.
    pub fn set_value(&mut self, key: &str, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        let value = match self.get(key) {
            Some(ParamValue::Length(_)) => ParamValue::Length(value),
            Some(ParamValue::Count(_)) => {
                ParamValue::Count(ValueKind::Count.clean(value).min(f64::from(u32::MAX)) as u32)
            }
            None => return false,
        };
        let before = self.get(key);
        self.set(key, value) && before != Some(value)
    }

    /// The same with `factor` times the elements along each edge, rounded, at least one.
    pub fn refined(&self, factor: f64) -> Self {
        let mut refined = self.clone();
        if factor.is_finite() && factor > 0.0 && factor != 1.0 {
            let divisions: &mut [u32] = match &mut refined {
                Self::Box { divisions, .. } => divisions,
                Self::Rectangle { divisions, .. } => divisions,
            };
            for n in divisions {
                *n = (f64::from(*n) * factor)
                    .round()
                    .clamp(1.0, f64::from(u32::MAX)) as u32;
            }
        }
        refined
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

    #[test]
    fn parameters_are_read_and_set_by_key() {
        let mut block = MeshPrimitive::default_box();
        let keys: Vec<&str> = block.params().iter().map(|p| p.key).collect();
        assert_eq!(keys, ["x", "y", "z", "lx", "ly", "lz", "nx", "ny", "nz"]);
        // Every key reaches its own number.
        for param in block.params() {
            assert!(block.get(param.key).is_some(), "{}", param.key);
        }
        assert_eq!(block.get("ly"), Some(ParamValue::Length(0.02)));
        assert_eq!(block.get("nx"), Some(ParamValue::Count(10)));
        assert!(block.set("lx", ParamValue::Length(0.5)));
        assert!(block.set("nz", ParamValue::Count(4)));
        assert!(block.set("y", ParamValue::Length(-1.0)));
        // A count is no length, and a rectangle has no Z.
        assert!(!block.set("nx", ParamValue::Length(3.0)));
        assert!(block.get("w").is_none());
        let MeshPrimitive::Box {
            origin,
            size,
            divisions,
            ..
        } = &block
        else {
            panic!()
        };
        assert_eq!((origin[1], size[0], divisions[2]), (-1.0, 0.5, 4));
        let rectangle = MeshPrimitive::default_rectangle();
        assert_eq!(rectangle.params().len(), 6);
        assert!(rectangle.get("z").is_none());
        assert!(rectangle.get("nz").is_none());
    }

    #[test]
    fn value_links_set_lengths_and_whole_counts() {
        let mut block = MeshPrimitive::default_box();
        assert_eq!(block.value_kind("lx"), Some(ValueKind::LENGTH));
        assert_eq!(block.value_kind("nz"), Some(ValueKind::Count));
        assert_eq!(block.value_kind("element"), None);
        assert!(block.set_value("lx", 0.25));
        assert!(!block.set_value("lx", 0.25));
        assert_eq!(block.value("lx"), Some(0.25));
        assert!(block.set_value("nx", 7.4));
        assert_eq!(block.value("nx"), Some(7.0));
        assert!(block.set_value("ny", 0.0));
        assert_eq!(block.value("ny"), Some(1.0));
        let refined = block.refined(1.5);
        assert_eq!(refined.value("nx"), Some(11.0));
        assert_eq!(refined.value("ny"), Some(2.0));
        assert_eq!(refined.value("nz"), Some(3.0));
        assert_eq!(block.refined(1.0), block);
    }
}
