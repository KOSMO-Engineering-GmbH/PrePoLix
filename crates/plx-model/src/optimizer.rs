//! What an Optimizer block of the project schematic does: it gives a value to a parameter of
//! the blocks before a measure, has them solved, reads the measure and decides on the next
//! values, until it is done. The loop is a pure function of the history, so that the project
//! window only runs what it asks for; each round is a batch of values, one for a mesh
//! convergence, all that are left for a parameter study.

use serde::{Deserialize, Serialize};

use crate::ValueKind;

/// The kind of an optimizer and its settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OptimizerKind {
    /// Refines a mesh by a factor until the measure changes less than `tolerance` from one
    /// iteration to the next. Its value is the factor of a refine mesh block.
    MeshConvergence {
        /// Refinement of the first iteration.
        start: f64,
        /// Factor on the refinement from one iteration to the next.
        factor: f64,
        /// Relative change of the measure below which it has converged, as a fraction.
        tolerance: f64,
        max_iterations: u32,
        /// No iteration with more elements than these.
        max_elements: u64,
    },
    /// Solves each of `values` and lists the measure of each.
    ParameterStudy { kind: ValueKind, values: Vec<f64> },
}

impl Default for OptimizerKind {
    fn default() -> Self {
        Self::MeshConvergence {
            start: 1.0,
            factor: 1.5,
            tolerance: 0.02,
            max_iterations: 6,
            max_elements: 500_000,
        }
    }
}

/// One solve of an optimizer: the value given, the measure read, the elements of the mesh
/// it was refined to and how long it took. `measure` is `None` when it failed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Iteration {
    pub value: f64,
    pub measure: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elements: Option<u64>,
    pub seconds: f64,
}

/// What an optimizer asks for after the iterations so far.
#[derive(Clone, Debug, PartialEq)]
pub enum Next {
    /// Solve these values, in this order.
    Run(Vec<f64>),
    /// Done: the value to leave the parameter at, if it is to change, and why it is done.
    Finished {
        value: Option<f64>,
        outcome: Outcome,
    },
}

/// Why an optimizer is done.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The measure changed by `change` (a fraction) in the last iteration, below the
    /// tolerance.
    Converged { change: f64 },
    /// The last allowed iteration did not converge; `change` is its last change.
    NotConverged { change: Option<f64> },
    /// The next refinement would have more elements than allowed.
    ElementLimit { elements: u64 },
    /// An iteration gave no measure.
    Failed { iteration: usize },
    /// Every value of a study was solved.
    StudyDone,
}

impl OptimizerKind {
    pub const LABELS: [&str; 2] = ["Mesh convergence", "Parameter study"];

    pub fn label(&self) -> &'static str {
        match self {
            Self::MeshConvergence { .. } => Self::LABELS[0],
            Self::ParameterStudy { .. } => Self::LABELS[1],
        }
    }

    /// A new optimizer of the kind with that label, with default settings.
    pub fn of_label(label: &str) -> Self {
        if label == Self::LABELS[1] {
            Self::ParameterStudy {
                kind: ValueKind::Factor,
                values: vec![1.0, 1.5, 2.0],
            }
        } else {
            Self::default()
        }
    }

    /// The kind of number it gives: a refinement factor, or what the study varies.
    pub fn value_kind(&self) -> ValueKind {
        match self {
            Self::MeshConvergence { .. } => ValueKind::Factor,
            Self::ParameterStudy { kind, .. } => *kind,
        }
    }

    /// The value it gives before it ran.
    pub fn start(&self) -> f64 {
        match self {
            Self::MeshConvergence { start, .. } => *start,
            Self::ParameterStudy { values, .. } => values.first().copied().unwrap_or(1.0),
        }
    }

    /// How many iterations it runs at most.
    pub fn planned(&self) -> usize {
        match self {
            Self::MeshConvergence { max_iterations, .. } => *max_iterations as usize,
            Self::ParameterStudy { values, .. } => values.len(),
        }
    }

    /// What to solve next after `history`; `elements_at` tells how many elements a value
    /// would mesh, where that is known.
    pub fn next_values(
        &self,
        history: &[Iteration],
        elements_at: impl Fn(f64) -> Option<u64>,
    ) -> Next {
        match self {
            Self::MeshConvergence {
                start,
                factor,
                tolerance,
                max_iterations,
                max_elements,
            } => {
                let last_good = || history.iter().rev().find(|i| i.measure.is_some());
                let Some(last) = history.last() else {
                    return match elements_at(*start) {
                        Some(elements) if elements > *max_elements => Next::Finished {
                            value: None,
                            outcome: Outcome::ElementLimit { elements },
                        },
                        _ => Next::Run(vec![*start]),
                    };
                };
                if last.measure.is_none() {
                    return Next::Finished {
                        value: last_good().map(|i| i.value),
                        outcome: Outcome::Failed {
                            iteration: history.len(),
                        },
                    };
                }
                let change = change(history);
                if let Some(change) = change
                    && change < *tolerance
                {
                    return Next::Finished {
                        value: None,
                        outcome: Outcome::Converged { change },
                    };
                }
                if history.len() >= *max_iterations as usize {
                    return Next::Finished {
                        value: None,
                        outcome: Outcome::NotConverged { change },
                    };
                }
                let next = last.value * factor;
                match elements_at(next) {
                    Some(elements) if elements > *max_elements => Next::Finished {
                        value: None,
                        outcome: Outcome::ElementLimit { elements },
                    },
                    _ => Next::Run(vec![next]),
                }
            }
            Self::ParameterStudy { values, .. } => match values.get(history.len()..) {
                Some(left) if !left.is_empty() => Next::Run(left.to_vec()),
                _ => Next::Finished {
                    value: None,
                    outcome: Outcome::StudyDone,
                },
            },
        }
    }
}

/// The relative change of the measure in the last iteration, against the one before; `None`
/// before there are two measures.
pub fn change(history: &[Iteration]) -> Option<f64> {
    let [.., before, last] = history else {
        return None;
    };
    let (a, b) = (before.measure?, last.measure?);
    let scale = a.abs().max(b.abs());
    Some(if scale == 0.0 {
        0.0
    } else {
        (b - a).abs() / scale
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iteration(value: f64, measure: Option<f64>) -> Iteration {
        Iteration {
            value,
            measure,
            elements: None,
            seconds: 1.0,
        }
    }

    #[test]
    fn mesh_convergence_refines_until_the_measure_settles() {
        let kind = OptimizerKind::default();
        let none = |_| None;
        assert_eq!(kind.next_values(&[], none), Next::Run(vec![1.0]));
        let mut history = vec![iteration(1.0, Some(100.0))];
        assert_eq!(kind.next_values(&history, none), Next::Run(vec![1.5]));
        history.push(iteration(1.5, Some(110.0)));
        assert_eq!(kind.next_values(&history, none), Next::Run(vec![2.25]));
        history.push(iteration(2.25, Some(111.0)));
        match kind.next_values(&history, none) {
            Next::Finished {
                value: None,
                outcome: Outcome::Converged { change },
            } => assert!((change - 1.0 / 111.0).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
        // Too many elements next: it stops on the finest mesh solved.
        let history = &history[..2];
        let limit = kind.next_values(history, |v| Some((v * 400_000.0) as u64));
        assert_eq!(
            limit,
            Next::Finished {
                value: None,
                outcome: Outcome::ElementLimit { elements: 900_000 }
            }
        );
    }

    #[test]
    fn a_failed_iteration_goes_back_to_the_last_good_value() {
        let kind = OptimizerKind::default();
        let history = [iteration(1.0, Some(5.0)), iteration(1.5, None)];
        assert_eq!(
            kind.next_values(&history, |_| None),
            Next::Finished {
                value: Some(1.0),
                outcome: Outcome::Failed { iteration: 2 }
            }
        );
    }

    #[test]
    fn a_study_runs_the_values_that_are_left() {
        let kind = OptimizerKind::ParameterStudy {
            kind: ValueKind::LENGTH,
            values: vec![0.1, 0.2, 0.3],
        };
        assert_eq!(
            kind.next_values(&[iteration(0.1, None)], |_| None),
            Next::Run(vec![0.2, 0.3])
        );
        let all = [0.1, 0.2, 0.3].map(|v| iteration(v, Some(v)));
        assert!(matches!(
            kind.next_values(&all, |_| None),
            Next::Finished {
                outcome: Outcome::StudyDone,
                ..
            }
        ));
    }
}
