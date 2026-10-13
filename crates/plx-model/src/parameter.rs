//! Numbers of the project schematic that flow along value links: the parameters of a
//! Parameters block, and the inputs of other blocks they feed, such as the size and division
//! of a mesh block.

use serde::{Deserialize, Serialize};

use crate::Quantity;

/// What a number is, so that a value link joins like with like: a length goes to a length, a
/// count of elements to a count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueKind {
    /// A length, stored in SI units (m).
    #[default]
    Length,
    /// A whole number, such as the elements along an edge.
    Count,
    /// A factor without unit, such as the refinement of a mesh.
    Factor,
}

impl ValueKind {
    pub const ALL: [Self; 3] = [Self::Length, Self::Count, Self::Factor];

    pub fn label(self) -> &'static str {
        match self {
            Self::Length => "Length",
            Self::Count => "Count",
            Self::Factor => "Factor",
        }
    }

    /// The quantity its values are shown in, for a length.
    pub fn quantity(self) -> Option<Quantity> {
        (self == Self::Length).then_some(Quantity::Length)
    }

    /// A value of this kind as it is kept: a count is a whole number of at least one.
    pub fn clean(self, value: f64) -> f64 {
        match self {
            Self::Count if value.is_finite() => value.round().max(1.0),
            _ => value,
        }
    }
}

/// A named number of a Parameters block, which value links give to other blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub kind: ValueKind,
    /// In SI units for a length.
    pub value: f64,
}

impl Parameter {
    pub fn new(name: impl Into<String>, kind: ValueKind, value: f64) -> Self {
        Self {
            name: name.into(),
            kind,
            value: kind.clean(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_whole_numbers_of_at_least_one() {
        assert_eq!(Parameter::new("n", ValueKind::Count, 3.6).value, 4.0);
        assert_eq!(ValueKind::Count.clean(0.2), 1.0);
        assert_eq!(ValueKind::Length.clean(0.0125), 0.0125);
        assert_eq!(ValueKind::Factor.clean(1.5), 1.5);
    }
}
