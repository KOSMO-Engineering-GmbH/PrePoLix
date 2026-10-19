//! The editor of a CAD Operation block of the project schematic: the geometry linked to the
//! block in its own 3D view, where the edges or faces the operation acts on are picked, with the
//! kind of operation, its length and a preview of the result.

use std::collections::BTreeSet;
use std::path::Path;

use plx_mesher::CadEntity;
use plx_model::{CadOperation, CadOperationKind, Geometry, Quantity, UnitSystem};

use crate::model::Model;
use crate::viewport::{Click, ViewCommand, Viewport};

pub enum CadEditorResult {
    Open,
    /// Closed with OK: the operation, and its result when the preview made it for exactly
    /// this operation.
    Ok(CadOperation, Option<Geometry>),
    Cancel,
}

/// Which geometry the view shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Input,
    Result,
}

pub struct CadOperationEditor {
    /// The block edited.
    pub id: u64,
    name: String,
    input: Geometry,
    /// The input as shown, where edges or faces are picked; `None` when it cannot be shown.
    view: Option<Model>,
    /// The result of the last preview and the operation it was made for.
    result: Option<(Model, Geometry, CadOperation)>,
    shown: Shown,
    kind: CadOperationKind,
    value: f64,
    units: UnitSystem,
    /// A parameter study gives the length.
    length_linked: bool,
    /// The picked edges or faces of the input.
    picks: BTreeSet<CadEntity>,
    /// Why the input cannot be shown or the preview failed.
    message: Option<String>,
    /// The view has to take the shown geometry anew.
    scene_due: bool,
    /// The camera has to frame the geometry, once it is shown first.
    fit_due: bool,
}

impl CadOperationEditor {
    pub fn new(
        id: u64,
        name: &str,
        input: Geometry,
        operation: &CadOperation,
        units: UnitSystem,
        length_linked: bool,
    ) -> Self {
        // Picks only stay with the signatures of the entities they are.
        let input = if input.signatures.is_empty() {
            plx_mesher::with_signatures(input.clone()).unwrap_or(input)
        } else {
            input
        };
        let (view, message) = match plx_mesher::tessellate(&input) {
            Ok(display) => (Some(Model::geometry_view(Path::new(name), display)), None),
            Err(error) => (None, Some(format!("The geometry cannot be shown: {error}"))),
        };
        let picks = (operation.resolve(&input).unwrap_or_default().into_iter())
            .filter(|&e| operation.kind.picks(e))
            .collect();
        let mut editor = Self {
            id,
            name: name.to_owned(),
            input,
            view,
            result: None,
            shown: Shown::Input,
            kind: operation.kind,
            value: operation.value,
            units,
            length_linked,
            picks,
            message,
            scene_due: true,
            fit_due: true,
        };
        editor.update_highlight();
        editor
    }

    pub fn rename(&mut self, name: &str) {
        name.clone_into(&mut self.name);
    }

    /// The operation as set in the dialog.
    fn operation(&self) -> CadOperation {
        let entities = (self.picks.iter())
            .filter(|&&e| self.kind.picks(e))
            .filter_map(|&e| {
                self.input
                    .signatures
                    .iter()
                    .find(|s| s.entity == e)
                    .cloned()
            })
            .collect();
        CadOperation {
            kind: self.kind,
            entities,
            value: self.value,
        }
    }

    fn update_highlight(&mut self) {
        if let Some(view) = &mut self.view {
            view.highlight = crate::meshing::entities_highlight(view, &self.picks);
        }
        self.scene_due = true;
    }

    /// The view as shown now.
    fn shown_model(&self) -> Option<&Model> {
        match self.shown {
            Shown::Input => self.view.as_ref(),
            Shown::Result => self.result.as_ref().map(|(model, ..)| model),
        }
    }

    /// What a click points at for the operation: the CAD edge near it, else the edge of
    /// the face under it nearest to it; or the planar face under it.
    fn entity_at(&self, click: &Click) -> Option<CadEntity> {
        let view = self.view.as_ref()?;
        let hit = view.pick_click(click)?;
        if self.kind.picks(CadEntity::Face(0)) {
            let face = crate::geometry_selection::face_at(view, &hit).map(CadEntity::Face)?;
            let planar =
                (self.input.signatures.iter()).any(|s| s.entity == face && s.kind == "Plane");
            return planar.then_some(face);
        }
        match crate::meshing::cad_pick(view, &hit, click.precision_at(hit.point)) {
            Some(edge @ CadEntity::Edge(_)) => Some(edge),
            _ => crate::geometry_selection::edge_at(view, &hit).map(CadEntity::Edge),
        }
    }

    /// Applies the operation to the input and shows the result, or says why it failed.
    fn preview(&mut self) {
        let operation = self.operation();
        let made = operation
            .problem()
            .map_or_else(|| Ok(()), Err)
            .and_then(|()| operation.resolve(&self.input))
            .and_then(|entities| {
                plx_mesher::apply_cad_operation(
                    &self.input,
                    operation.kind,
                    &entities,
                    operation.value,
                )
                .map_err(|e| e.to_string())
            })
            .and_then(|geometry| {
                let display = plx_mesher::tessellate(&geometry).map_err(|e| e.to_string())?;
                Ok((
                    Model::geometry_view(Path::new(&self.name), display),
                    geometry,
                ))
            });
        match made {
            Ok((model, geometry)) => {
                self.result = Some((model, geometry, operation));
                self.shown = Shown::Result;
                self.message = None;
            }
            Err(why) => {
                self.result = None;
                self.shown = Shown::Input;
                self.message = Some(format!("The operation failed: {why}"));
            }
        }
        self.scene_due = true;
    }

    pub fn show(&mut self, ctx: &egui::Context, viewport: &mut Viewport) -> CadEditorResult {
        let mut result = CadEditorResult::Open;
        let mut open = true;
        egui::Window::new(format!("CAD Operation - {}", self.name))
            .id(egui::Id::new(("cad operation", self.id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([760.0, 600.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                self.form(ui);
                ui.separator();
                let height = (ui.available_height() - 40.0).max(320.0);
                let size = egui::vec2(ui.available_width().max(480.0), height);
                ui.allocate_ui(size, |ui| self.view_ui(ui, viewport));
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.button("Cancel").clicked() {
                        result = CadEditorResult::Cancel;
                    }
                    let operation = self.operation();
                    let problem = operation.problem();
                    let ok = ui
                        .add_enabled(problem.is_none(), egui::Button::new("OK"))
                        .on_hover_text(
                            "Keeps the operation; Update Project makes the geometry again \
                             unless the preview made it.",
                        );
                    if ok.clicked() {
                        let made = (self.result.as_ref())
                            .filter(|(_, _, previewed)| *previewed == operation)
                            .map(|(_, geometry, _)| geometry.clone());
                        result = CadEditorResult::Ok(operation, made);
                    }
                    if let Some(problem) = problem {
                        ui.colored_label(crate::style::error(), problem);
                    }
                });
            });
        if !open {
            result = CadEditorResult::Cancel;
        }
        result
    }

    fn form(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("cad operation form")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Operation");
                ui.horizontal(|ui| {
                    for kind in CadOperationKind::ALL {
                        if ui
                            .selectable_value(&mut self.kind, kind, kind.label())
                            .changed()
                        {
                            // Edges stay picked from fillet to chamfer, not for faces.
                            self.picks.retain(|&e| kind.picks(e));
                            self.update_highlight();
                        }
                    }
                });
                ui.end_row();
                ui.label(self.kind.value_label());
                ui.add_enabled_ui(!self.length_linked, |ui| {
                    let response = ui.add(crate::numeric::quantity(
                        &mut self.value,
                        self.units,
                        Quantity::Length,
                    ));
                    if self.length_linked {
                        response.on_disabled_hover_text(
                            "A parameter study gives it through the link of its row.",
                        );
                    }
                });
                ui.end_row();
                let (one, many) = (self.kind.picked_one(), self.kind.picked_noun());
                ui.label(capitalised(many));
                ui.horizontal(|ui| {
                    ui.label(match self.picks.len() {
                        0 => "none picked".to_owned(),
                        1 => format!("1 {one}"),
                        n => format!("{n} {many}"),
                    });
                    if ui
                        .add_enabled(!self.picks.is_empty(), egui::Button::new("Clear"))
                        .clicked()
                    {
                        self.picks.clear();
                        self.update_highlight();
                    }
                });
                ui.end_row();
            });
        ui.horizontal(|ui| {
            let previewed = self.result.is_some();
            ui.label("Show");
            for (shown, label) in [(Shown::Input, "Input"), (Shown::Result, "Result")] {
                let enabled = shown == Shown::Input || previewed;
                if ui
                    .add_enabled(
                        enabled,
                        egui::Button::selectable(self.shown == shown, label),
                    )
                    .clicked()
                {
                    self.shown = shown;
                    self.scene_due = true;
                }
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.view.is_some() && !self.picks.is_empty(),
                    egui::Button::new("Preview"),
                )
                .on_hover_text("Applies the operation to the geometry and shows the result.")
                .clicked()
            {
                self.preview();
            }
            if ui.button("Fit").clicked() {
                self.fit_due = true;
            }
        });
        match &self.message {
            Some(message) => {
                ui.colored_label(crate::style::error(), message);
            }
            None if self.shown == Shown::Input => {
                ui.weak(match self.kind {
                    CadOperationKind::PushPull => "Click the planar faces to move along their \
                        normal, outwards for a positive offset; a click on a picked face removes \
                        it. Neighbours at right angles follow, sloped ones get a step."
                        .to_owned(),
                    kind => format!(
                        "Click the edges to {}; a click on a picked edge removes it.",
                        kind.label().to_lowercase()
                    ),
                });
            }
            None => {
                ui.weak(format!(
                    "The result of the operation; show the input to pick {}.",
                    self.kind.picked_noun()
                ));
            }
        }
    }

    fn view_ui(&mut self, ui: &mut egui::Ui, viewport: &mut Viewport) {
        if self.scene_due {
            self.scene_due = false;
            match self.shown_model() {
                Some(model) => viewport.set_parts(&model.scene_meshes()),
                None => viewport.set_parts(&[]),
            }
            // The picked edges, drawn over the input in the highlight colour.
            let edges = match (self.shown, &self.view) {
                (Shown::Input, Some(view)) => (view.highlight.lines.iter())
                    .filter_map(|ends| {
                        let [a, b] = ends.map(|id| view.node_position(view.mesh.node_index(id)?));
                        Some([a?, b?])
                    })
                    .collect(),
                _ => Vec::new(),
            };
            viewport.overlay = crate::overlay::Overlay {
                edges,
                show_view_triad: true,
                ..Default::default()
            };
        }
        if self.fit_due
            && let Some(bounds) = self.shown_model().and_then(Model::visible_bounds)
        {
            self.fit_due = false;
            viewport.apply(ViewCommand::Fit, Some(bounds));
        }
        let picking = self.shown == Shown::Input;
        viewport.selecting = picking;
        let response = viewport.ui(ui);
        if let Some(command) = response.command {
            let bounds = self.shown_model().and_then(Model::visible_bounds);
            viewport.apply(command, bounds);
        }
        if let Some(click) = response.pivot {
            let pivot = self.shown_model().and_then(|m| m.pick_click(&click));
            viewport.set_pivot(pivot.map(|hit| hit.point));
        }
        if !picking {
            viewport.preview = Default::default();
            return;
        }
        if let Some(click) = response.click
            && let Some(entity) = self.entity_at(&click)
        {
            if !self.picks.remove(&entity) {
                self.picks.insert(entity);
            }
            self.update_highlight();
        }
        if let Some(hover) = response.hover {
            viewport.preview = (hover.and_then(|click| self.entity_at(&click)))
                .zip(self.view.as_ref())
                .map(|(entity, view)| {
                    crate::meshing::entities_preview(view, &BTreeSet::from([entity]))
                })
                .unwrap_or_default();
            ui.ctx().request_repaint();
        }
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
