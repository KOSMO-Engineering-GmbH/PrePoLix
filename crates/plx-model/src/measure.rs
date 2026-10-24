//! A key figure taken from the results of a step, as a Measure block of the project schematic
//! computes it: the largest von Mises stress in a region, the displacement of a node, a total
//! force, an eigenfrequency. A parameter study reads it.

use serde::{Deserialize, Serialize};

/// Which extreme of a field a measure takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Extreme {
    #[default]
    Max,
    Min,
    /// The value of largest magnitude, with its sign.
    MaxAbs,
}

impl Extreme {
    pub const ALL: [Self; 3] = [Self::Max, Self::Min, Self::MaxAbs];

    pub fn label(self) -> &'static str {
        match self {
            Self::Max => "Maximum",
            Self::Min => "Minimum",
            Self::MaxAbs => "Largest magnitude",
        }
    }

    /// The extreme of `values`, NaN left out.
    pub fn of(self, values: impl IntoIterator<Item = f64>) -> Option<f64> {
        let mut best: Option<f64> = None;
        for value in values.into_iter().filter(|v| v.is_finite()) {
            let better = match (self, best) {
                (_, None) => true,
                (Self::Max, Some(b)) => value > b,
                (Self::Min, Some(b)) => value < b,
                (Self::MaxAbs, Some(b)) => value.abs() > b.abs(),
            };
            if better {
                best = Some(value);
            }
        }
        best
    }
}

/// The nodes a field measure looks at.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeasureRegion {
    /// Every node of the results.
    #[default]
    All,
    /// The nodes of a node set of the analysis' mesh.
    NodeSet(String),
}

/// What a Measure block computes from the results of a step: of its last increment, or of the
/// mode for an eigenfrequency. Field and component names are those of the results workspace
/// (`STRESS` / `MISES`, `DISP` / `ALL`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MeasureKind {
    /// An extreme of a field component over a region.
    FieldExtreme {
        field: String,
        component: String,
        extreme: Extreme,
        region: MeasureRegion,
    },
    /// A field component at one node.
    NodeValue {
        field: String,
        component: String,
        node: u32,
    },
    /// A field component at a point, interpolated with the shape functions of the element
    /// that contains it. The coordinates are in SI units, each a row of the block that a
    /// value link can set.
    PointValue {
        field: String,
        component: String,
        point: [f64; 3],
    },
    /// The first value of a history output (`.dat`), such as the total force of a set
    /// (`TOTAL_FORCE` / `RF3`).
    History {
        set: String,
        field: String,
        component: String,
    },
    /// The eigenfrequency of mode `mode`, counted from 1, of a frequency step.
    Eigenfrequency { mode: u32 },
}

/// The keys of the coordinate rows of a measure at a point, X, Y and Z: a value link sets
/// them at `["Parameters", key]`.
pub const POINT_AXES: [&str; 3] = ["x", "y", "z"];

impl Default for MeasureKind {
    fn default() -> Self {
        Self::FieldExtreme {
            field: "STRESS".into(),
            component: "MISES".into(),
            extreme: Extreme::Max,
            region: MeasureRegion::All,
        }
    }
}

impl MeasureKind {
    /// The kinds a measure editor offers, each with defaults.
    pub fn choices() -> [Self; 5] {
        [
            Self::default(),
            Self::NodeValue {
                field: "DISP".into(),
                component: "ALL".into(),
                node: 1,
            },
            Self::PointValue {
                field: "DISP".into(),
                component: "ALL".into(),
                point: [0.0; 3],
            },
            Self::History {
                set: String::new(),
                field: "TOTAL_FORCE".into(),
                component: "RF3".into(),
            },
            Self::Eigenfrequency { mode: 1 },
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::FieldExtreme { .. } => "Field extreme",
            Self::NodeValue { .. } => "Value at a node",
            Self::PointValue { .. } => "Value at a point",
            Self::History { .. } => "History output",
            Self::Eigenfrequency { .. } => "Eigenfrequency",
        }
    }

    /// The field and component its value has, for its unit; an eigenfrequency is `FREQ`.
    pub fn field(&self) -> (&str, &str) {
        match self {
            Self::FieldExtreme {
                field, component, ..
            }
            | Self::NodeValue {
                field, component, ..
            }
            | Self::PointValue {
                field, component, ..
            }
            | Self::History {
                field, component, ..
            } => (field, component),
            Self::Eigenfrequency { .. } => ("FREQ", ""),
        }
    }

    /// The point of a measure at a point.
    pub fn point(&self) -> Option<[f64; 3]> {
        match self {
            Self::PointValue { point, .. } => Some(*point),
            _ => None,
        }
    }

    /// Sets the coordinate of a measure at a point by its key in [`POINT_AXES`]; whether it
    /// changed.
    pub fn set_coordinate(&mut self, key: &str, value: f64) -> bool {
        let Self::PointValue { point, .. } = self else {
            return false;
        };
        let Some(axis) = POINT_AXES.iter().position(|a| *a == key) else {
            return false;
        };
        if !value.is_finite() || point[axis] == value {
            return false;
        }
        point[axis] = value;
        true
    }

    /// A short text of what it measures: "Maximum of STRESS MISES in TIP".
    pub fn describe(&self) -> String {
        match self {
            Self::FieldExtreme {
                field,
                component,
                extreme,
                region,
            } => {
                let region = match region {
                    MeasureRegion::All => String::new(),
                    MeasureRegion::NodeSet(set) => format!(" in {set}"),
                };
                format!("{} of {field} {component}{region}", extreme.label())
            }
            Self::NodeValue {
                field,
                component,
                node,
            } => format!("{field} {component} at node {node}"),
            Self::PointValue {
                field, component, ..
            } => format!("{field} {component} at a point"),
            Self::History {
                set,
                field,
                component,
            } => format!("{field} {component} of {set}"),
            Self::Eigenfrequency { mode } => format!("Eigenfrequency of mode {mode}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extremes_leave_out_nan_and_keep_the_sign() {
        let values = [1.0, -3.0, f64::NAN, 2.0];
        assert_eq!(Extreme::Max.of(values), Some(2.0));
        assert_eq!(Extreme::Min.of(values), Some(-3.0));
        assert_eq!(Extreme::MaxAbs.of(values), Some(-3.0));
        assert_eq!(Extreme::Max.of([f64::NAN]), None);
    }
}
