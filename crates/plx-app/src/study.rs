//! The editor of a Parameter Study block of the project schematic: its columns and the table
//! of design points, like the parameter set of Ansys Workbench. It edits the block through
//! [`StudyEdit`]s, which the project window applies, so that the table follows a run while
//! the editor is open.

use plx_model::{PointState, UnitSystem, ValueKind};

/// A change the editor asks for.
pub enum StudyEdit {
    Units(UnitSystem),
    SetInput {
        point: usize,
        column: usize,
        value: f64,
    },
    AddPoint,
    Duplicate(usize),
    RemovePoint(usize),
    SetCurrent(usize),
    RenameInput(usize, String),
    RenameOutput(usize, String),
    RemoveInput(usize),
    RemoveOutput(usize),
    Run,
    Reset,
    Close,
}

/// A design point as the editor shows it.
pub struct PointView {
    /// The inputs in SI units, edited in place.
    pub inputs: Vec<f64>,
    /// The outputs as text in the units of their sources.
    pub outputs: Vec<String>,
    pub state: PointState,
    pub time: String,
    pub elements: String,
}

/// What the editor shows of a study, made by the project window, which knows the units of
/// the outputs.
#[derive(Default)]
pub struct StudyView {
    pub units: UnitSystem,
    /// Name and kind of each input.
    pub inputs: Vec<(String, Option<ValueKind>)>,
    /// Name and unit of each output.
    pub outputs: Vec<(String, String)>,
    pub points: Vec<PointView>,
    pub current: usize,
    /// The point being solved while the study runs.
    pub running: Option<usize>,
    /// The study or the project is being updated: the table cannot be changed.
    pub busy: bool,
}

/// The editor of a Parameter Study block.
pub struct StudyEditor {
    pub id: u64,
    name: String,
    /// The column being renamed: input or output, its index and the name typed so far.
    renaming: Option<(bool, usize, String)>,
}

impl StudyEditor {
    pub fn new(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.to_owned(),
            renaming: None,
        }
    }

    pub fn rename(&mut self, name: &str) {
        name.clone_into(&mut self.name);
    }

    pub fn show(&mut self, ctx: &egui::Context, view: &StudyView) -> Vec<StudyEdit> {
        let mut edits = Vec::new();
        let mut open = true;
        egui::Window::new(format!("Parameter Study - {}", self.name))
            .id(egui::Id::new(("study", self.id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .default_width(640.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Shown in");
                    let mut units = view.units;
                    egui::ComboBox::from_id_salt("study units")
                        .selected_text(units.label())
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for choice in UnitSystem::ALL {
                                ui.selectable_value(&mut units, choice, choice.label());
                            }
                        });
                    if units != view.units {
                        edits.push(StudyEdit::Units(units));
                    }
                });
                if view.inputs.is_empty() || view.outputs.is_empty() {
                    ui.weak(
                        "Drag the Inputs pin of the block to the exposed parameters of \
                         analyses, the numbers of mesh blocks or the Factor of a Refine Mesh \
                         block, and the value of a Measure block to its Outputs pin: each \
                         link is a column of the table.",
                    );
                }
                self.columns(ui, view, &mut edits);
                ui.separator();
                self.table(ui, view, &mut edits);
                ui.separator();
                ui.horizontal(|ui| {
                    let idle = !view.busy;
                    if ui
                        .add_enabled(idle, egui::Button::new("Add Point"))
                        .on_hover_text("Adds a point with the inputs of the current one.")
                        .clicked()
                    {
                        edits.push(StudyEdit::AddPoint);
                    }
                    let run = ui.add_enabled(idle, egui::Button::new("Run"));
                    if run
                        .on_hover_text(
                            "Solves the points that are not up to date, one after the other, \
                             and the current one last.",
                        )
                        .clicked()
                    {
                        edits.push(StudyEdit::Run);
                    }
                    let reset = ui.add_enabled(idle, egui::Button::new("Reset"));
                    if reset
                        .on_hover_text("Forgets the outputs of all points; the inputs stay.")
                        .clicked()
                    {
                        edits.push(StudyEdit::Reset);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            edits.push(StudyEdit::Close);
                        }
                    });
                });
            });
        if !open {
            edits.push(StudyEdit::Close);
        }
        edits
    }

    /// The inputs and outputs, each with its kind or unit, to rename or remove.
    fn columns(&mut self, ui: &mut egui::Ui, view: &StudyView, edits: &mut Vec<StudyEdit>) {
        let rows = (view.inputs.iter().map(|(name, kind)| {
            let kind = kind.map_or("number".to_owned(), |k| match k.quantity() {
                Some(q) => format!("{} [{}]", k.label(), view.units.unit(q)),
                None => k.label().to_owned(),
            });
            (true, name, kind)
        }))
        .chain(
            view.outputs
                .iter()
                .map(|(name, unit)| (false, name, unit.clone())),
        );
        egui::Grid::new("study columns")
            .num_columns(4)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                let (mut input, mut output) = (0, 0);
                for (is_input, name, kind) in rows {
                    let index = if is_input { &mut input } else { &mut output };
                    let at = *index;
                    *index += 1;
                    let id = if is_input {
                        format!("P{}", at + 1)
                    } else {
                        format!("O{}", at + 1)
                    };
                    ui.label(&id).on_hover_text(if is_input {
                        "An input: the study gives its value through the links of its row."
                    } else {
                        "An output: the study reads it from the block linked to its row."
                    });
                    match &mut self.renaming {
                        Some((i, a, text)) if *i == is_input && *a == at => {
                            let response =
                                ui.add(egui::TextEdit::singleline(text).desired_width(180.0));
                            if response.lost_focus() {
                                let text = text.trim().to_owned();
                                if !text.is_empty() && &text != name {
                                    edits.push(if is_input {
                                        StudyEdit::RenameInput(at, text)
                                    } else {
                                        StudyEdit::RenameOutput(at, text)
                                    });
                                }
                                self.renaming = None;
                            } else {
                                response.request_focus();
                            }
                        }
                        _ => {
                            let label = ui.add(egui::Label::new(name).sense(egui::Sense::click()));
                            if label
                                .on_hover_text("Double-click to rename")
                                .double_clicked()
                            {
                                self.renaming = Some((is_input, at, name.clone()));
                            }
                        }
                    }
                    ui.weak(kind);
                    let remove = ui.add_enabled(!view.busy, egui::Button::new("Remove").small());
                    if remove
                        .on_hover_text("Removes the column and its links.")
                        .clicked()
                    {
                        edits.push(if is_input {
                            StudyEdit::RemoveInput(at)
                        } else {
                            StudyEdit::RemoveOutput(at)
                        });
                    }
                    ui.end_row();
                }
            });
    }

    /// The design points: inputs to edit, outputs, state, time and elements.
    fn table(&mut self, ui: &mut egui::Ui, view: &StudyView, edits: &mut Vec<StudyEdit>) {
        if view.points.is_empty() {
            ui.weak("No design points yet: link an input or an output first.");
            return;
        }
        egui::ScrollArea::both()
            .max_height(320.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("study points")
                    .striped(true)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        ui.strong("Point");
                        ui.strong("State");
                        for (i, (name, _)) in view.inputs.iter().enumerate() {
                            ui.strong(format!("P{} {name}", i + 1));
                        }
                        for (i, (name, unit)) in view.outputs.iter().enumerate() {
                            let unit = if unit.is_empty() {
                                String::new()
                            } else {
                                format!(" [{unit}]")
                            };
                            ui.strong(format!("O{} {name}{unit}", i + 1));
                        }
                        ui.strong("Time");
                        ui.strong("Elements");
                        ui.label("");
                        ui.end_row();
                        for (index, point) in view.points.iter().enumerate() {
                            self.point_row(ui, view, index, point, edits);
                            ui.end_row();
                        }
                    });
            });
    }

    fn point_row(
        &mut self,
        ui: &mut egui::Ui,
        view: &StudyView,
        index: usize,
        point: &PointView,
        edits: &mut Vec<StudyEdit>,
    ) {
        let current = index == view.current;
        let label = if current {
            format!("DP {index} (current)")
        } else {
            format!("DP {index}")
        };
        let label = if current {
            egui::RichText::new(label).strong()
        } else {
            egui::RichText::new(label)
        };
        ui.label(label).on_hover_text(if current {
            "The current point: the blocks stand at its inputs."
        } else {
            "A design point; right-click its state to make it the current one."
        });
        let state = if view.running == Some(index) {
            "running".to_owned()
        } else {
            point.state.label().to_owned()
        };
        let color = match point.state {
            _ if view.running == Some(index) => ui.visuals().text_color(),
            PointState::UpToDate => crate::style::adapt(egui::Color32::from_rgb(40, 150, 60)),
            PointState::Failed => crate::style::error(),
            PointState::OutOfDate | PointState::NotSolved => {
                crate::style::adapt(egui::Color32::from_rgb(200, 140, 0))
            }
        };
        let state = ui.add(
            egui::Label::new(egui::RichText::new(state).color(color)).sense(egui::Sense::click()),
        );
        let tip = match point.state {
            PointState::Failed => {
                "A solve or a measure gave no value: the solution monitor and the output tell why."
            }
            PointState::OutOfDate => {
                "Solved, but the blocks it runs, its inputs or the outputs changed since."
            }
            _ => "Right-click for more.",
        };
        state.on_hover_text(tip).context_menu(|ui| {
            if ui
                .add_enabled(!view.busy && !current, egui::Button::new("Set as Current"))
                .on_hover_text(
                    "The blocks take its inputs; Update Project then solves them, so the \
                     analyses show its results.",
                )
                .clicked()
            {
                edits.push(StudyEdit::SetCurrent(index));
                ui.close();
            }
            if ui
                .add_enabled(!view.busy, egui::Button::new("Duplicate"))
                .clicked()
            {
                edits.push(StudyEdit::Duplicate(index));
                ui.close();
            }
            if ui
                .add_enabled(!view.busy && !current, egui::Button::new("Delete"))
                .clicked()
            {
                edits.push(StudyEdit::RemovePoint(index));
                ui.close();
            }
        });
        for (column, (&value, (_, kind))) in point.inputs.iter().zip(&view.inputs).enumerate() {
            let mut value = value;
            let field = match kind {
                Some(kind) => crate::parameter_set::field(&mut value, *kind, view.units),
                None => crate::numeric::drag_value(&mut value),
            };
            let changed = ui.add_enabled(!view.busy, field).changed();
            if changed {
                edits.push(StudyEdit::SetInput {
                    point: index,
                    column,
                    value,
                });
            }
        }
        for output in &point.outputs {
            ui.label(output);
        }
        ui.label(&point.time);
        ui.label(&point.elements);
        ui.label("");
    }
}

/// A value of a study as text, in `units` for a quantity.
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
