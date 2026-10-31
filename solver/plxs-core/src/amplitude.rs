//! `*AMPLITUDE`: a time table that scales loads and boundary values.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Amplitude {
    /// Upper case name.
    pub name: String,
    /// `TIME=TOTAL TIME`: the table is read with the total time, otherwise with the step time.
    pub total_time: bool,
    /// `SHIFTX`, `SHIFTY`: added to every time and value.
    pub shift_x: f64,
    pub shift_y: f64,
    /// (time, value) points in ascending time.
    pub points: Vec<(f64, f64)>,
}

impl Amplitude {
    /// The value at a time: linear between the points, constant before the first and after
    /// the last (CalculiX holds the last value).
    pub fn value(&self, time: f64) -> f64 {
        let t = time - self.shift_x;
        let points = &self.points;
        let Some(&(t0, v0)) = points.first() else {
            return self.shift_y;
        };
        let &(t1, v1) = points.last().unwrap_or(&(t0, v0));
        let value = if t <= t0 {
            v0
        } else if t >= t1 {
            v1
        } else {
            let upper = points
                .iter()
                .position(|&(pt, _)| pt > t)
                .unwrap_or(points.len() - 1);
            let (ta, va) = points[upper - 1];
            let (tb, vb) = points[upper];
            if tb == ta {
                vb
            } else {
                va + (t - ta) / (tb - ta) * (vb - va)
            }
        };
        value + self.shift_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_amplitude_ramps_and_holds() {
        let amplitude = Amplitude {
            name: "A".into(),
            total_time: false,
            shift_x: 0.0,
            shift_y: 0.0,
            points: vec![(0.0, 0.0), (1.0, 2.0), (2.0, 2.0)],
        };
        assert_eq!(amplitude.value(-1.0), 0.0);
        assert_eq!(amplitude.value(0.5), 1.0);
        assert_eq!(amplitude.value(1.5), 2.0);
        assert_eq!(amplitude.value(9.0), 2.0);
    }
}
