//! Material laws of the PrePoLix solver. M1: isotropic linear elasticity (Hooke). Plastic,
//! creep and thermal laws follow in M2 (concept section 4.2).
//!
//! Sources: any textbook on elasticity (e.g. Timoshenko & Goodier) for Hooke's law in the
//! engineering-strain convention. No CalculiX code was consulted.

use plxs_core::material::{Material, interpolate};
use plxs_core::{Error, Result};

/// The 6 × 6 matrix of isotropic linear elasticity acting on engineering strains
/// `[xx, yy, zz, xy, yz, zx]` (γ for the shear components).
pub fn isotropic_matrix(young: f64, poisson: f64) -> [[f64; 6]; 6] {
    let lambda = young * poisson / ((1.0 + poisson) * (1.0 - 2.0 * poisson));
    let mu = young / (2.0 * (1.0 + poisson));
    let mut d = [[0.0; 6]; 6];
    for i in 0..3 {
        d[i][..3].fill(lambda);
        d[i][i] = lambda + 2.0 * mu;
        d[i + 3][i + 3] = mu;
    }
    d
}

/// Isotropic linear elasticity of a material at a temperature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearElastic {
    pub young: f64,
    pub poisson: f64,
}

impl LinearElastic {
    /// The elastic constants of a material at `temperature`, interpolated in its
    /// `*ELASTIC` table.
    pub fn of(material: &Material, temperature: f64) -> Result<Self> {
        let elastic = material.elastic.as_ref().ok_or_else(|| {
            Error::input(
                "*MATERIAL",
                format!("material {} has no *ELASTIC definition", material.name),
            )
        })?;
        let young: Vec<_> = elastic
            .rows
            .iter()
            .map(|r| plxs_core::material::TemperatureValue {
                value: r.young,
                temperature: r.temperature,
            })
            .collect();
        let poisson: Vec<_> = elastic
            .rows
            .iter()
            .map(|r| plxs_core::material::TemperatureValue {
                value: r.poisson,
                temperature: r.temperature,
            })
            .collect();
        let (young, poisson) = match (
            interpolate(&young, temperature),
            interpolate(&poisson, temperature),
        ) {
            (Some(e), Some(nu)) => (e, nu),
            _ => {
                return Err(Error::input(
                    "*ELASTIC",
                    format!("material {} has an empty *ELASTIC table", material.name),
                ));
            }
        };
        Ok(Self { young, poisson })
    }

    pub fn matrix(&self) -> [[f64; 6]; 6] {
        isotropic_matrix(self.young, self.poisson)
    }

    pub fn shear_modulus(&self) -> f64 {
        self.young / (2.0 * (1.0 + self.poisson))
    }
}

/// Density of a material at a temperature, if it has one.
pub fn density(material: &Material, temperature: f64) -> Option<f64> {
    material
        .density
        .as_ref()
        .and_then(|d| interpolate(&d.rows, temperature))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooke_matrix_matches_the_textbook() {
        let d = isotropic_matrix(210_000.0, 0.3);
        let e = 210_000.0;
        let nu = 0.3;
        let factor = e / ((1.0 + nu) * (1.0 - 2.0 * nu));
        assert!((d[0][0] - factor * (1.0 - nu)).abs() < 1e-6);
        assert!((d[0][1] - factor * nu).abs() < 1e-6);
        assert!((d[3][3] - e / (2.0 * (1.0 + nu))).abs() < 1e-6);
        assert_eq!(d[0][3], 0.0);
    }
}
