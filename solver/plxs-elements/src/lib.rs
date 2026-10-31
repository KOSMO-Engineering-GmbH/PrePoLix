//! Element formulations of the PrePoLix solver: shape functions and integration rules of
//! all families at one place ([`shape`]), the solid elements ([`solid`]) and the expansion of
//! shells, membranes and beams into solids ([`expand`]). Plane elements, springs and gaps
//! follow in later milestones (concept section 4.1).
//!
//! Sources: the CalculiX manual for node, face and integration-point conventions; textbooks
//! (Zienkiewicz & Taylor; Hughes; Bathe) for the formulations. No CalculiX code was
//! consulted.

pub mod expand;
pub mod shape;
pub mod solid;

pub use shape::{Face, Family, IntegrationRule};
pub use solid::{Formulation, SolidElement, solid};
