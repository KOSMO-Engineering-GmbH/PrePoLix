//! Cross-sections of beams, kept apart from the sections that use them: a named profile a
//! beam section picks by name, so that several beam sections, and analyses linked in the
//! project, share one definition.
//!
//! Coordinates in the plane of a profile are those of CalculiX's `*BEAM SECTION`: `u` along
//! the 1-direction (the section's normal) and `v` along the 2-direction (the beam axis crossed
//! with the 1-direction), about the centre of the profile.

use serde::{Deserialize, Serialize};

/// Points on the outline of a round profile, in [`BeamProfile::outlines`].
pub const ROUND_POINTS: usize = 24;

/// A named beam profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossSection {
    pub name: String,
    pub profile: BeamProfile,
}

impl CrossSection {
    /// Prefix of new cross-sections' names, like `Cross_Section-1`.
    pub const PREFIX: &'static str = "Cross_Section";
}

/// Cross-section of a beam, with CalculiX's dimensions: the 1-direction is the normal of
/// the section, the 2-direction the beam axis crossed with it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum BeamProfile {
    /// Rectangle with thickness `a` in the 1-direction and `b` in the 2-direction.
    Rect { a: f64, b: f64 },
    /// Circle; CalculiX takes the diameter, see [`BeamProfile::data_line`].
    Circ { radius: f64 },
    /// Hollow circle with outer radius and wall thickness (`B32R` only).
    Pipe { radius: f64, thickness: f64 },
    /// Hollow rectangle `a` by `b` with four wall thicknesses, in the order CalculiX reads
    /// them: at +1, +2, -1 and -2 (`B32R` only).
    Box { a: f64, b: f64, t: [f64; 4] },
}

impl BeamProfile {
    /// The profiles with dimensions of a millimetre, in SI units.
    pub const ALL: [BeamProfile; 4] = [
        BeamProfile::Rect { a: 1e-3, b: 1e-3 },
        BeamProfile::Circ { radius: 1e-3 },
        BeamProfile::Pipe {
            radius: 1e-3,
            thickness: 1e-4,
        },
        BeamProfile::Box {
            a: 1e-3,
            b: 1e-3,
            t: [1e-4; 4],
        },
    ];

    /// Value of `SECTION=` in the input file.
    pub fn keyword(&self) -> &'static str {
        match self {
            BeamProfile::Rect { .. } => "RECT",
            BeamProfile::Circ { .. } => "CIRC",
            BeamProfile::Pipe { .. } => "PIPE",
            BeamProfile::Box { .. } => "BOX",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            BeamProfile::Rect { .. } => "Rectangular",
            BeamProfile::Circ { .. } => "Circular",
            BeamProfile::Pipe { .. } => "Pipe",
            BeamProfile::Box { .. } => "Box",
        }
    }

    /// Pipes and boxes exist for `B32R` only in CalculiX.
    pub fn needs_reduced_integration(&self) -> bool {
        matches!(self, BeamProfile::Pipe { .. } | BeamProfile::Box { .. })
    }

    /// The dimensions as CalculiX 2.21 reads them on the first data line of `*BEAM
    /// SECTION`. A circle is given by its thickness in both directions, the diameter: a
    /// value of 4 puts the expanded nodes at radius 2. A pipe takes the radius itself.
    pub fn data_line(&self) -> Vec<f64> {
        match *self {
            BeamProfile::Rect { a, b } => vec![a, b],
            BeamProfile::Circ { radius } => vec![2.0 * radius, 2.0 * radius],
            BeamProfile::Pipe { radius, thickness } => vec![radius, thickness],
            BeamProfile::Box { a, b, t } => vec![a, b, t[0], t[1], t[2], t[3]],
        }
    }

    /// Whether the dimensions make a section CalculiX accepts.
    pub fn is_valid(&self) -> bool {
        let positive = |v: f64| v.is_finite() && v > 0.0;
        match *self {
            BeamProfile::Rect { a, b } => positive(a) && positive(b),
            BeamProfile::Circ { radius } => positive(radius),
            BeamProfile::Pipe { radius, thickness } => {
                positive(radius) && positive(thickness) && thickness < radius
            }
            BeamProfile::Box { a, b, t } => {
                positive(a)
                    && positive(b)
                    && t.iter().all(|&t| positive(t))
                    && t[0] + t[2] < a
                    && t[1] + t[3] < b
            }
        }
    }

    /// Every dimension of the profile; all are lengths.
    pub fn lengths_mut(&mut self) -> Vec<&mut f64> {
        match self {
            BeamProfile::Rect { a, b } => vec![a, b],
            BeamProfile::Circ { radius } => vec![radius],
            BeamProfile::Pipe { radius, thickness } => vec![radius, thickness],
            BeamProfile::Box { a, b, t } => {
                let [t1, t2, t3, t4] = t;
                vec![a, b, t1, t2, t3, t4]
            }
        }
    }

    pub fn convert_units(&mut self, c: &crate::convert::Conversion) {
        for length in self.lengths_mut() {
            c.value(length, crate::Quantity::Length);
        }
    }

    /// The thickness of the profile in the 1- and 2-direction, which the offsets of a beam
    /// section refer to.
    pub fn size(&self) -> [f64; 2] {
        match *self {
            BeamProfile::Rect { a, b } | BeamProfile::Box { a, b, .. } => [a, b],
            BeamProfile::Circ { radius } | BeamProfile::Pipe { radius, .. } => {
                [2.0 * radius, 2.0 * radius]
            }
        }
    }

    /// The outlines of the profile about its centre in the 1-2 plane, counter-clockwise: the
    /// outer one first, then the hole of a hollow profile.
    pub fn outlines(&self) -> Vec<Outline> {
        let circle = |r: f64| Outline {
            points: (0..ROUND_POINTS)
                .map(|k| {
                    let angle = std::f64::consts::TAU * k as f64 / ROUND_POINTS as f64;
                    [r * angle.cos(), r * angle.sin()]
                })
                .collect(),
            round: true,
        };
        let rect = |[u0, v0]: [f64; 2], [u1, v1]: [f64; 2]| Outline {
            points: vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            round: false,
        };
        match *self {
            BeamProfile::Rect { a, b } => vec![rect([-a / 2.0, -b / 2.0], [a / 2.0, b / 2.0])],
            BeamProfile::Circ { radius } => vec![circle(radius)],
            BeamProfile::Pipe { radius, thickness } => {
                vec![circle(radius), circle(radius - thickness)]
            }
            // Walls at +1, +2, -1 and -2.
            BeamProfile::Box { a, b, t } => vec![
                rect([-a / 2.0, -b / 2.0], [a / 2.0, b / 2.0]),
                rect(
                    [-a / 2.0 + t[2], -b / 2.0 + t[3]],
                    [a / 2.0 - t[0], b / 2.0 - t[1]],
                ),
            ],
        }
    }

    /// Area, centroid and second moments of area of the profile, exact for round ones too.
    pub fn properties(&self) -> SectionProperties {
        match *self {
            BeamProfile::Circ { radius } => SectionProperties::disc(radius, 0.0),
            BeamProfile::Pipe { radius, thickness } => {
                SectionProperties::disc(radius, radius - thickness)
            }
            BeamProfile::Rect { .. } | BeamProfile::Box { .. } => {
                let loops: Vec<Vec<[f64; 2]>> =
                    self.outlines().into_iter().map(|o| o.points).collect();
                SectionProperties::of_loops(&loops)
            }
        }
    }
}

/// An outline of a profile, closed; a round one is shaded smooth around.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub points: Vec<[f64; 2]>,
    pub round: bool,
}

/// Area and second moments of area of a profile, in the 1-2 plane: `centroid` about the
/// centre of the profile, the moments about axes through the centroid parallel to the 1- and
/// 2-direction. `i11` is the moment about the 1-axis (the integral of `v^2`), `i22` about the
/// 2-axis (`u^2`), `i12` the product moment (`u v`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SectionProperties {
    pub area: f64,
    pub centroid: [f64; 2],
    pub i11: f64,
    pub i22: f64,
    pub i12: f64,
}

impl SectionProperties {
    /// A full or hollow circle about its centre; `inner` 0 for a full one.
    fn disc(outer: f64, inner: f64) -> Self {
        let pi = std::f64::consts::PI;
        let i = pi / 4.0 * (outer.powi(4) - inner.powi(4));
        Self {
            area: pi * (outer * outer - inner * inner),
            centroid: [0.0; 2],
            i11: i,
            i22: i,
            i12: 0.0,
        }
    }

    /// A region bounded by closed polygons: the first counts as the outer boundary and the
    /// others as holes, whichever way they run.
    pub fn of_loops(loops: &[Vec<[f64; 2]>]) -> Self {
        // Integrals over the region about the origin, by Green's theorem per polygon.
        let mut sums = [0.0; 6];
        for (k, points) in loops.iter().enumerate() {
            let mut own = [0.0; 6];
            for (i, &[x0, y0]) in points.iter().enumerate() {
                let [x1, y1] = points[(i + 1) % points.len()];
                let cross = x0 * y1 - x1 * y0;
                own[0] += cross / 2.0;
                own[1] += cross * (x0 + x1) / 6.0;
                own[2] += cross * (y0 + y1) / 6.0;
                own[3] += cross * (y0 * y0 + y0 * y1 + y1 * y1) / 12.0;
                own[4] += cross * (x0 * x0 + x0 * x1 + x1 * x1) / 12.0;
                own[5] += cross * (x0 * y1 + 2.0 * x0 * y0 + 2.0 * x1 * y1 + x1 * y0) / 24.0;
            }
            // Counter-clockwise for the outer boundary, clockwise for holes.
            let sign = own[0].signum() * if k == 0 { 1.0 } else { -1.0 };
            for (sum, value) in sums.iter_mut().zip(own) {
                *sum += sign * value;
            }
        }
        let [area, su, sv, iuu_v, iuu_u, iuv] = sums;
        if area.abs() < f64::MIN_POSITIVE {
            return Self::default();
        }
        let centroid = [su / area, sv / area];
        Self {
            area,
            centroid,
            i11: iuu_v - area * centroid[1] * centroid[1],
            i22: iuu_u - area * centroid[0] * centroid[0],
            i12: iuv - area * centroid[0] * centroid[1],
        }
    }

    /// The principal moments of area, largest first, and the angle in radians from the
    /// 1-axis to the principal axis of the largest one, counter-clockwise.
    pub fn principal(&self) -> ([f64; 2], f64) {
        let mean = (self.i11 + self.i22) / 2.0;
        let radius = (((self.i11 - self.i22) / 2.0).powi(2) + self.i12 * self.i12).sqrt();
        let mut angle = if radius > 1e-12 * mean.abs().max(f64::MIN_POSITIVE) {
            0.5 * (-2.0 * self.i12).atan2(self.i11 - self.i22)
        } else {
            0.0
        };
        // An axis, not a direction: from above -90 up to 90 degrees.
        if angle <= -std::f64::consts::FRAC_PI_2 + 1e-12 {
            angle += std::f64::consts::PI;
        }
        ([mean + radius, mean - radius], angle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    #[test]
    fn rectangles_have_the_textbook_moments() {
        let p = BeamProfile::Rect { a: 20.0, b: 40.0 }.properties();
        assert!(close(p.area, 800.0));
        assert!(close(p.i11, 20.0 * 40.0_f64.powi(3) / 12.0));
        assert!(close(p.i22, 40.0 * 20.0_f64.powi(3) / 12.0));
        assert!(close(p.i12, 0.0));
        assert_eq!(p.centroid, [0.0, 0.0]);
        let ([big, small], angle) = p.principal();
        assert!(close(big, p.i11) && close(small, p.i22));
        assert!(close(angle, 0.0));
        // Turned a quarter, axis I runs along the 2-direction.
        let (_, angle) = BeamProfile::Rect { a: 40.0, b: 20.0 }
            .properties()
            .principal();
        assert!(close(angle, std::f64::consts::FRAC_PI_2));
    }

    #[test]
    fn an_unequal_box_moves_its_centroid_towards_the_thicker_wall() {
        // A 10 by 10 box, wall 3 at +1 and 1 elsewhere: the hole lies towards -1.
        let p = BeamProfile::Box {
            a: 10.0,
            b: 10.0,
            t: [3.0, 1.0, 1.0, 1.0],
        }
        .properties();
        // Outer 100 minus a hole of 6 by 8 centred at u = -1.
        assert!(close(p.area, 100.0 - 48.0));
        assert!(close(p.centroid[0], 48.0 / 52.0));
        assert!(close(p.centroid[1], 0.0));
        let i22_origin = 10.0 * 1000.0 / 12.0 - (8.0 * 216.0 / 12.0 + 48.0);
        assert!(close(p.i22, i22_origin - 52.0 * p.centroid[0].powi(2)));
        assert!(close(p.i11, 10.0 * 1000.0 / 12.0 - 6.0 * 512.0 / 12.0));
    }

    #[test]
    fn pipes_subtract_the_hole() {
        let p = BeamProfile::Pipe {
            radius: 2.0,
            thickness: 1.0,
        }
        .properties();
        let pi = std::f64::consts::PI;
        assert!(close(p.area, 3.0 * pi));
        assert!(close(p.i11, pi / 4.0 * 15.0));
    }

    #[test]
    fn a_tilted_angle_has_its_principal_axis_on_the_diagonal() {
        // An equal-legged angle of 10 by 10 with legs 1 thick: principal axes at 45 degrees.
        let outline = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 1.0],
            [1.0, 1.0],
            [1.0, 10.0],
            [0.0, 10.0],
        ];
        let p = SectionProperties::of_loops(&[outline]);
        assert!(close(p.area, 19.0));
        assert!(p.i12 != 0.0);
        let ([big, small], angle) = p.principal();
        assert!(big > small);
        assert!(close(angle.abs(), std::f64::consts::FRAC_PI_4));
        // The sum of the moments does not depend on the axes.
        assert!(close(big + small, p.i11 + p.i22));
    }
}
