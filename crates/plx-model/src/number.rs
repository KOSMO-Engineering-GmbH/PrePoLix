//! The arithmetic of the number blocks of the project schematic: a Math block computes a
//! number from two or three others, a Switch block picks one of several by its index.

use serde::{Deserialize, Serialize};

/// What a Math block computes, like the operations of Blender's Math node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MathOp {
    #[default]
    Add,
    Subtract,
    Multiply,
    Divide,
    Minimum,
    Maximum,
    /// The value kept between a lower and an upper limit.
    Clamp,
}

impl MathOp {
    pub const ALL: [Self; 7] = [
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
        Self::Minimum,
        Self::Maximum,
        Self::Clamp,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Multiply => "Multiply",
            Self::Divide => "Divide",
            Self::Minimum => "Minimum",
            Self::Maximum => "Maximum",
            Self::Clamp => "Clamp",
        }
    }

    /// What it computes, for its tooltip.
    pub fn formula(self) -> &'static str {
        match self {
            Self::Add => "A + B",
            Self::Subtract => "A - B",
            Self::Multiply => "A x B, B a factor",
            Self::Divide => "A / B, B a factor",
            Self::Minimum => "the smaller of A and B",
            Self::Maximum => "the larger of A and B",
            Self::Clamp => "Value, but at least Min and at most Max",
        }
    }

    /// The names of its inputs, in order.
    pub fn inputs(self) -> &'static [&'static str] {
        match self {
            Self::Clamp => &["Value", "Min", "Max"],
            _ => &["A", "B"],
        }
    }

    /// Whether its second input is a factor without unit, which scales the first: the
    /// others are numbers of the kind of the result.
    pub fn scales(self) -> bool {
        matches!(self, Self::Multiply | Self::Divide)
    }

    /// The result of `values`, its inputs in order; `None` for a division by zero or too
    /// few values.
    pub fn apply(self, values: &[f64]) -> Option<f64> {
        let (a, b) = (*values.first()?, *values.get(1)?);
        let result = match self {
            Self::Add => a + b,
            Self::Subtract => a - b,
            Self::Multiply => a * b,
            Self::Divide if b == 0.0 => return None,
            Self::Divide => a / b,
            Self::Minimum => a.min(b),
            Self::Maximum => a.max(b),
            // Like Blender: the upper limit wins when the limits cross.
            Self::Clamp => a.max(b).min(*values.get(2)?),
        };
        result.is_finite().then_some(result)
    }
}

/// The value a Switch block gives: the choice of number `index`, counted from 1; `None` when
/// there is no such choice.
pub fn switch(index: f64, choices: &[f64]) -> Option<f64> {
    let index = index.round();
    if !(1.0..=choices.len() as f64).contains(&index) {
        return None;
    }
    choices.get(index as usize - 1).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_computes_its_operation() {
        assert_eq!(MathOp::Add.apply(&[2.0, 3.0]), Some(5.0));
        assert_eq!(MathOp::Subtract.apply(&[2.0, 3.0]), Some(-1.0));
        assert_eq!(MathOp::Multiply.apply(&[0.1, 2.0]), Some(0.2));
        assert_eq!(MathOp::Divide.apply(&[1.0, 4.0]), Some(0.25));
        assert_eq!(MathOp::Divide.apply(&[1.0, 0.0]), None);
        assert_eq!(MathOp::Minimum.apply(&[2.0, 3.0]), Some(2.0));
        assert_eq!(MathOp::Maximum.apply(&[2.0, 3.0]), Some(3.0));
        assert_eq!(MathOp::Clamp.apply(&[5.0, 0.0, 3.0]), Some(3.0));
        assert_eq!(MathOp::Clamp.apply(&[-1.0, 0.0, 3.0]), Some(0.0));
        assert_eq!(MathOp::Clamp.apply(&[1.0, 0.0]), None);
        assert_eq!(MathOp::Clamp.inputs().len(), 3);
        assert!(MathOp::Multiply.scales() && !MathOp::Add.scales());
    }

    #[test]
    fn switch_picks_a_choice_by_its_index_from_one() {
        let choices = [10.0, 20.0, 30.0];
        assert_eq!(switch(1.0, &choices), Some(10.0));
        assert_eq!(switch(2.6, &choices), Some(30.0));
        assert_eq!(switch(0.0, &choices), None);
        assert_eq!(switch(4.0, &choices), None);
        assert_eq!(switch(f64::NAN, &choices), None);
    }
}
