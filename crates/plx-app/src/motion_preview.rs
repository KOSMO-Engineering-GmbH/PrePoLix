//! Preview of the motion a displacement boundary condition prescribes with an amplitude: the
//! nodes of its region move through the amplitude's time range in the 3D view, exaggerated so
//! that the motion is visible, while the rest of the mesh stays put.

use plx_model::{Amplitude, AmplitudeTime, Region};

use crate::model::Model;
use crate::results::{model_size, round_significant};

/// Real seconds one pass through the amplitude's time range takes.
const PASS_SECONDS: f64 = 3.0;
/// Rest at the end of a pass before the next one starts.
const REST_SECONDS: f64 = 0.7;
/// The largest motion is drawn this fraction of the model size, like the automatic
/// deformation scale of results.
const VISIBLE_FRACTION: f64 = 0.25;

/// A playing preview, started by the play button of the boundary condition dialog.
pub struct MotionPreview {
    started: f64,
    /// Characteristic size of the model for the scale.
    model_size: f64,
    /// The region the node indices were found for.
    nodes: Option<(Region, Vec<usize>)>,
    /// The moment drawn last, for the dialog.
    pub last: Option<Moment>,
}

/// One moment of the preview.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    /// Time on the amplitude's time axis.
    pub time: f64,
    /// Exaggeration of the drawn motion.
    pub scale: f64,
}

impl MotionPreview {
    pub fn new(now: f64, model: &Model) -> Self {
        Self {
            started: now,
            model_size: model.mesh.bounds().map_or(1.0, model_size),
            nodes: None,
            last: None,
        }
    }

    /// The drawn offsets of the region's nodes by node index at `now` (seconds of the GUI
    /// clock), for the prescribed displacement `values` (`None` where free) following the
    /// amplitude.
    pub fn frame(
        &mut self,
        now: f64,
        model: &Model,
        region: Region,
        values: [Option<f64>; 3],
        amplitude: &Amplitude,
    ) -> Vec<(usize, [f64; 3])> {
        if self.nodes.as_ref().is_none_or(|(r, _)| *r != region) {
            let mesh = &model.mesh;
            let indices = (region.nodes(mesh).into_iter())
                .filter_map(|id| mesh.node_index(id))
                .collect();
            self.nodes = Some((region, indices));
        }
        let values = values.map(|v| v.unwrap_or(0.0));
        let length = values.iter().map(|v| v * v).sum::<f64>().sqrt();
        let scale = automatic_scale(self.model_size, length * peak_factor(amplitude));
        let time = time_at(amplitude, now - self.started);
        let factor = scale * amplitude.value_at(time);
        let offset = values.map(|v| v * factor);
        let nodes = self.nodes.as_ref().map_or(&[][..], |(_, n)| n);
        self.last = Some(Moment { time, scale });
        nodes.iter().map(|&n| (n, offset)).collect()
    }
}

/// The time range a preview passes through: from the first to the last point, from the
/// start of the step if the amplitude counts step time.
pub fn time_range(amplitude: &Amplitude) -> (f64, f64) {
    let shift = amplitude.shift_time;
    let first = amplitude.points.first().map_or(0.0, |p| p[0] + shift);
    let last = amplitude.points.last().map_or(0.0, |p| p[0] + shift);
    let start = match amplitude.time_span {
        AmplitudeTime::Step => first.min(0.0),
        AmplitudeTime::Total => first,
    };
    if last > start {
        (start, last)
    } else {
        // A single point: a constant factor, shown over a unit of time.
        (start, start + 1.0)
    }
}

/// The amplitude time `elapsed` real seconds after the preview started; passes repeat.
fn time_at(amplitude: &Amplitude, elapsed: f64) -> f64 {
    let (start, end) = time_range(amplitude);
    let phase = elapsed.max(0.0) % (PASS_SECONDS + REST_SECONDS);
    start + (end - start) * (phase / PASS_SECONDS).min(1.0)
}

/// Largest magnitude of the factor; linear in between, it is reached at a point.
fn peak_factor(amplitude: &Amplitude) -> f64 {
    let shift = amplitude.shift_amplitude;
    (amplitude.points.iter())
        .map(|p| (p[1] + shift).abs())
        .fold(0.0, f64::max)
}

/// Scale that draws the largest motion a quarter of the model size, rounded to two digits;
/// 1 when nothing moves.
fn automatic_scale(model_size: f64, peak: f64) -> f64 {
    if peak > 0.0 && peak.is_finite() {
        round_significant(VISIBLE_FRACTION * model_size / peak, 2)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Amplitude {
        let mut amplitude = Amplitude::new("A");
        amplitude.points = vec![[0.0, 0.0], [2.0, -1.0], [4.0, 0.5]];
        amplitude
    }

    #[test]
    fn passes_through_the_time_range_and_rests_at_its_end() {
        let amplitude = ramp();
        assert_eq!(time_range(&amplitude), (0.0, 4.0));
        assert_eq!(time_at(&amplitude, 0.0), 0.0);
        assert_eq!(time_at(&amplitude, PASS_SECONDS / 2.0), 2.0);
        assert_eq!(time_at(&amplitude, PASS_SECONDS + 0.1), 4.0);
        assert_eq!(time_at(&amplitude, PASS_SECONDS + REST_SECONDS), 0.0);
    }

    #[test]
    fn step_time_starts_at_zero_and_a_single_point_spans_a_unit() {
        let mut amplitude = ramp();
        amplitude.points = vec![[1.0, 1.0], [3.0, 2.0]];
        assert_eq!(time_range(&amplitude), (0.0, 3.0));
        amplitude.time_span = AmplitudeTime::Total;
        assert_eq!(time_range(&amplitude), (1.0, 3.0));
        amplitude.points = vec![[0.0, 1.0]];
        assert_eq!(time_range(&amplitude), (0.0, 1.0));
    }

    #[test]
    fn scale_draws_the_largest_motion_a_quarter_of_the_model() {
        let amplitude = ramp();
        assert_eq!(peak_factor(&amplitude), 1.0);
        // Model size 100, 2 mm at the peak: 25 / 2.
        assert_eq!(automatic_scale(100.0, 2.0 * peak_factor(&amplitude)), 13.0);
        assert_eq!(automatic_scale(100.0, 0.0), 1.0);
    }
}
