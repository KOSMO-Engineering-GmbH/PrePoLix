//! The Export VTU and Export VTK dialogs: which increments of the current results go to
//! ParaView, and in which encoding. One increment becomes one file; several become one file
//! each plus a collection that ParaView opens as a time series (see [`plx_io::vtk`]).

use plx_io::vtk::VtkFormat;
use plx_results::Increment;

/// Which increments are exported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The increment shown.
    Current,
    /// All increments of the step of the increment shown.
    Step,
    /// All increments of all steps.
    All,
}

impl Scope {
    /// The increments of `increments` the scope covers, `current` being the one shown.
    pub fn select(self, increments: &[Increment], current: usize) -> Vec<&Increment> {
        let shown = increments.get(current);
        match self {
            Scope::Current => shown.into_iter().collect(),
            Scope::Step => increments
                .iter()
                .filter(|i| shown.is_some_and(|s| s.step == i.step))
                .collect(),
            Scope::All => increments.iter().collect(),
        }
    }
}

pub struct VtkExportDialog {
    /// The results file exported.
    pub result: usize,
    pub format: VtkFormat,
    pub scope: Scope,
    /// Binary (compressed for VTU) rather than ASCII.
    pub binary: bool,
}

pub enum DialogResult {
    Open,
    Export,
    Cancel,
}

impl VtkExportDialog {
    pub fn new(result: usize, format: VtkFormat) -> Self {
        Self {
            result,
            format,
            scope: Scope::Current,
            binary: true,
        }
    }

    pub fn title(&self) -> &'static str {
        match self.format {
            VtkFormat::Xml => "Export VTU",
            VtkFormat::Legacy => "Export VTK",
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        increments: &[Increment],
        current: usize,
    ) -> DialogResult {
        let mut result = DialogResult::Open;
        let mut open = true;
        let title = self.title();
        let format = self.format;
        let shown = increments.get(current);
        egui::Window::new(title)
            .id(crate::ids::window(ctx, "vtk export dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.strong("Increments");
                let step = shown.map_or(0, |s| s.step);
                let count = |scope: Scope| scope.select(increments, current).len();
                let current_label = shown.map_or("Current increment".into(), |s| {
                    format!(
                        "Current increment (step {}, increment {})",
                        s.step, s.increment
                    )
                });
                ui.radio_value(&mut self.scope, Scope::Current, current_label);
                ui.radio_value(
                    &mut self.scope,
                    Scope::Step,
                    format!("All increments of step {step} ({})", count(Scope::Step)),
                );
                ui.radio_value(
                    &mut self.scope,
                    Scope::All,
                    format!("All increments of all steps ({})", count(Scope::All)),
                );
                ui.add_space(4.0);
                ui.strong("Encoding");
                ui.horizontal(|ui| {
                    let binary = match format {
                        VtkFormat::Xml => "Binary (compressed)",
                        VtkFormat::Legacy => "Binary",
                    };
                    ui.radio_value(&mut self.binary, true, binary);
                    ui.radio_value(&mut self.binary, false, "ASCII");
                });
                ui.add_space(4.0);
                let several = count(self.scope) > 1;
                let note = match (several, format) {
                    (false, _) => format!(
                        "Writes one .{} file with the undeformed mesh and all result fields. \
                         Apply Warp By Vector with DISP in ParaView to show the deformation.",
                        format.extension()
                    ),
                    (true, VtkFormat::Xml) => "Writes one .vtu file per increment into a \
                        folder and a .pvd collection next to it. Open the .pvd file in \
                        ParaView to step through the increments as a time series."
                        .into(),
                    (true, VtkFormat::Legacy) => "Writes one .vtk file per increment into a \
                        folder and a .vtk.series file next to it. Open the .vtk.series file in \
                        ParaView (5.5 or later) to step through the increments as a time \
                        series."
                        .into(),
                };
                ui.set_max_width(380.0);
                ui.label(note);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.button("Cancel").clicked() {
                        result = DialogResult::Cancel;
                    }
                    if ui.button("Export …").clicked() {
                        result = DialogResult::Export;
                    }
                });
            });
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            result = DialogResult::Cancel;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plx_results::AnalysisKind;

    fn increment(step: u32, increment: u32) -> Increment {
        Increment {
            nodal_diameter: None,
            step,
            increment,
            kind: AnalysisKind::Static,
            value: f64::from(increment),
            fields: Vec::new(),
        }
    }

    #[test]
    fn scopes_pick_the_shown_increment_its_step_or_all() {
        let increments = [increment(1, 1), increment(1, 2), increment(2, 1)];
        let numbers = |scope: Scope| -> Vec<(u32, u32)> {
            scope
                .select(&increments, 1)
                .iter()
                .map(|i| (i.step, i.increment))
                .collect()
        };
        assert_eq!(numbers(Scope::Current), [(1, 2)]);
        assert_eq!(numbers(Scope::Step), [(1, 1), (1, 2)]);
        assert_eq!(numbers(Scope::All).len(), 3);
    }
}
