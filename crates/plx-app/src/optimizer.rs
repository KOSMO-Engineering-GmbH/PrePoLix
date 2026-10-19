//! The editor of an Optimizer block of the project schematic: what it varies and when it is
//! done, with the iterations of its last run as a table and a diagram.

use plx_model::{OptimizerKind, UnitSystem, ValueKind};

use crate::xy_plot::{self, XyData};

pub enum OptimizerEditorResult {
    Open,
    /// Closed with OK: the settings and the unit system its values are shown in.
    Ok(OptimizerKind, UnitSystem),
    /// Run with these settings, the editor staying open.
    Run(OptimizerKind, UnitSystem),
    /// Forget the iterations and give the start value.
    Reset,
    Cancel,
}

/// The iterations of a run as the editor shows them, text made by the project window, which
/// knows the units of the measure.
#[derive(Default)]
pub struct History {
    /// Value, measure, elements and time of each iteration.
    pub rows: Vec<[String; 4]>,
    /// The diagram: x and the measure in its units, of the iterations that gave one.
    pub plot: XyData,
    /// How the last run ended, or the iteration it is at.
    pub report: String,
    pub running: bool,
}

/// The editor of an Optimizer block.
pub struct OptimizerEditor {
    pub id: u64,
    name: String,
    kind: OptimizerKind,
    units: UnitSystem,
}

impl OptimizerEditor {
    pub fn new(id: u64, name: &str, kind: OptimizerKind, units: UnitSystem) -> Self {
        Self {
            id,
            name: name.to_owned(),
            kind,
            units,
        }
    }

    pub fn rename(&mut self, name: &str) {
        name.clone_into(&mut self.name);
    }

    pub fn show(&mut self, ctx: &egui::Context, history: &History) -> OptimizerEditorResult {
        let mut result = OptimizerEditorResult::Open;
        let mut open = true;
        egui::Window::new(format!("Optimizer - {}", self.name))
            .id(egui::Id::new(("optimizer", self.id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.set_width(460.0);
                egui::Grid::new("optimizer form")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| self.form(ui));
                ui.weak(match self.kind {
                    OptimizerKind::MeshConvergence { .. } => {
                        "Link its value to the Factor of a Refine Mesh block and the value of \
                         a Measure block of the results on that mesh to its Measure row. \
                         Update Project refines the mesh by the factor until the measure \
                         changes less than the tolerance."
                    }
                    OptimizerKind::ParameterStudy { .. } => {
                        "Link its value to a parameter of a mesh block and the value of a \
                         Measure block to its Measure row. Update Project solves each value \
                         and lists the measure of each."
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(&history.report).strong());
                if !history.rows.is_empty() {
                    history_table(ui, history);
                    if history.plot.x.len() > 1 {
                        ui.add_space(4.0);
                        ui.allocate_ui(egui::vec2(460.0, 200.0), |ui| {
                            xy_plot::show(ui, &history.plot);
                        });
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let run = ui.add_enabled(!history.running, egui::Button::new("Run"));
                    if run
                        .on_hover_text("Runs the loop now with these settings, from the start.")
                        .clicked()
                    {
                        result = OptimizerEditorResult::Run(self.kind.clone(), self.units);
                    }
                    let reset = ui.add_enabled(!history.running, egui::Button::new("Reset"));
                    if reset
                        .on_hover_text("Forgets the iterations and gives the start value.")
                        .clicked()
                    {
                        result = OptimizerEditorResult::Reset;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Cancel").clicked() {
                            result = OptimizerEditorResult::Cancel;
                        }
                        if ui.button("OK").clicked() {
                            result = OptimizerEditorResult::Ok(self.kind.clone(), self.units);
                        }
                    });
                });
            });
        if !open {
            result = OptimizerEditorResult::Cancel;
        }
        result
    }

    fn form(&mut self, ui: &mut egui::Ui) {
        ui.label("Optimizer");
        let current = self.kind.label();
        egui::ComboBox::from_id_salt("optimizer kind")
            .selected_text(current)
            .width(200.0)
            .show_ui(ui, |ui| {
                for label in OptimizerKind::LABELS {
                    if ui.selectable_label(label == current, label).clicked() && label != current {
                        self.kind = OptimizerKind::of_label(label);
                    }
                }
            });
        ui.end_row();
        let units = self.units;
        match &mut self.kind {
            OptimizerKind::MeshConvergence {
                start,
                factor,
                tolerance,
                max_iterations,
                max_elements,
            } => {
                ui.label("Start refinement");
                ui.add(egui::DragValue::new(start).range(0.05..=20.0).speed(0.01));
                ui.end_row();
                ui.label("Factor per step");
                ui.add(egui::DragValue::new(factor).range(1.05..=4.0).speed(0.01));
                ui.end_row();
                ui.label("Tolerance");
                let mut percent = *tolerance * 100.0;
                let field = egui::DragValue::new(&mut percent)
                    .range(0.01..=50.0)
                    .speed(0.1)
                    .suffix(" %");
                if ui.add(field).changed() {
                    *tolerance = percent / 100.0;
                }
                ui.end_row();
                ui.label("Max iterations");
                ui.add(egui::DragValue::new(max_iterations).range(2..=50));
                ui.end_row();
                ui.label("Max elements");
                ui.add(
                    egui::DragValue::new(max_elements)
                        .range(100..=50_000_000)
                        .speed(1000.0),
                );
                ui.end_row();
            }
            OptimizerKind::ParameterStudy { kind, values } => {
                ui.label("Shown in");
                egui::ComboBox::from_id_salt("optimizer units")
                    .selected_text(self.units.label())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for choice in UnitSystem::ALL {
                            ui.selectable_value(&mut self.units, choice, choice.label());
                        }
                    });
                ui.end_row();
                ui.label("Kind");
                egui::ComboBox::from_id_salt("optimizer value kind")
                    .selected_text(kind.label())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for choice in ValueKind::all() {
                            if ui.selectable_value(kind, choice, choice.label()).changed() {
                                for value in values.iter_mut() {
                                    *value = choice.clean(*value);
                                }
                            }
                        }
                    });
                ui.end_row();
                let mut remove = None;
                for (index, value) in values.iter_mut().enumerate() {
                    ui.label(format!("Value {}", index + 1));
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [150.0, 18.0],
                            crate::parameter_set::field(value, *kind, units),
                        );
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                    ui.end_row();
                }
                if let Some(index) = remove {
                    values.remove(index);
                }
                ui.label("");
                if ui.button("Add Value").clicked() {
                    let next = values.last().map_or(1.0, |v| kind.clean(v * 1.5));
                    values.push(next);
                }
                ui.end_row();
            }
        }
    }
}

fn history_table(ui: &mut egui::Ui, history: &History) {
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .show(ui, |ui| {
            egui::Grid::new("optimizer history")
                .num_columns(5)
                .striped(true)
                .spacing([14.0, 2.0])
                .show(ui, |ui| {
                    for title in ["#", "Value", "Measure", "Elements", "Time"] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for (index, row) in history.rows.iter().enumerate() {
                        ui.label(format!("{}", index + 1));
                        for cell in row {
                            ui.label(cell);
                        }
                        ui.end_row();
                    }
                });
        });
}

/// A value of an optimizer as text, in `units` for a length.
pub fn shown(kind: ValueKind, value: f64, units: UnitSystem) -> String {
    match kind {
        ValueKind::Quantity(quantity) => {
            let shown = units.shown(value, quantity);
            let unit = units.unit(quantity);
            format!("{} {unit}", crate::results::format_value(shown as f32))
        }
        ValueKind::Count => format!("{value:.0}"),
        ValueKind::Factor => crate::results::format_value(value as f32),
    }
}
