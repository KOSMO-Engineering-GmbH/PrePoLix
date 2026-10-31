//! Materials (concept section 1.2): elastic, plastic, creep, thermal expansion, conduction,
//! specific heat, density. Tables depend on temperature where CalculiX allows it: a row per
//! temperature, interpolated linearly, constant beyond the ends.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// Upper case name.
    pub name: String,
    pub elastic: Option<Elastic>,
    pub plastic: Option<Plastic>,
    pub creep: Option<Creep>,
    pub expansion: Option<Expansion>,
    pub conductivity: Option<Conductivity>,
    pub specific_heat: Option<SpecificHeat>,
    pub density: Option<Density>,
}

/// Isotropic linear elasticity (`*ELASTIC`, `TYPE=ISO`), rows of (E, nu, temperature).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Elastic {
    pub rows: Vec<ElasticRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElasticRow {
    pub young: f64,
    pub poisson: f64,
    pub temperature: f64,
}

/// Rate-independent plasticity (`*PLASTIC`) with a stress / plastic strain table per
/// temperature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plastic {
    pub hardening: Hardening,
    pub rows: Vec<PlasticRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hardening {
    Isotropic,
    Kinematic,
    Combined,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlasticRow {
    pub stress: f64,
    pub plastic_strain: f64,
    pub temperature: f64,
}

/// Norton creep (`*CREEP, LAW=NORTON`): rate = A * sigma^n * t^m, rows per temperature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Creep {
    pub rows: Vec<CreepRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreepRow {
    pub a: f64,
    pub n: f64,
    pub m: f64,
    pub temperature: f64,
}

/// Isotropic thermal expansion (`*EXPANSION`), coefficient per temperature, referred to
/// `zero` (the temperature at which the expansion is zero).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expansion {
    pub zero: f64,
    pub rows: Vec<TemperatureValue>,
}

/// Isotropic conductivity (`*CONDUCTIVITY`) per temperature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Conductivity {
    pub rows: Vec<TemperatureValue>,
}

/// `*SPECIFIC HEAT` per temperature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpecificHeat {
    pub rows: Vec<TemperatureValue>,
}

/// `*DENSITY` per temperature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Density {
    pub rows: Vec<TemperatureValue>,
}

/// A value at a temperature; a table with a single row at temperature 0 is constant.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemperatureValue {
    pub value: f64,
    pub temperature: f64,
}

impl TemperatureValue {
    pub const fn constant(value: f64) -> Self {
        Self {
            value,
            temperature: 0.0,
        }
    }
}

/// Linear interpolation in a temperature table, constant beyond its ends (as CalculiX does).
pub fn interpolate(rows: &[TemperatureValue], temperature: f64) -> Option<f64> {
    let (first, last) = (rows.first()?, rows.last()?);
    if rows.len() == 1 || temperature <= first.temperature {
        return Some(first.value);
    }
    if temperature >= last.temperature {
        return Some(last.value);
    }
    let upper = rows.iter().position(|r| r.temperature > temperature)?;
    let (a, b) = (rows[upper - 1], rows[upper]);
    let t = (temperature - a.temperature) / (b.temperature - a.temperature);
    Some(a.value + t * (b.value - a.value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_interpolate_and_hold_their_ends() {
        let rows = [
            TemperatureValue {
                value: 10.0,
                temperature: 0.0,
            },
            TemperatureValue {
                value: 20.0,
                temperature: 100.0,
            },
        ];
        assert_eq!(interpolate(&rows, -5.0), Some(10.0));
        assert_eq!(interpolate(&rows, 50.0), Some(15.0));
        assert_eq!(interpolate(&rows, 500.0), Some(20.0));
        assert_eq!(interpolate(&rows[..1], 500.0), Some(10.0));
        assert_eq!(interpolate(&[], 0.0), None);
    }
}
