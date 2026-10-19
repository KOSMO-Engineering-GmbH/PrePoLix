//! The editor of a materials block of the project schematic: a collection of materials kept
//! in the project, which analyses link item by item or as a whole. It edits them with the
//! dialogs of the FE Model tree and copies them from the material library or an analysis.

use std::path::Path;

use plx_mesh::FeMesh;
use plx_model::{Exposed, FeModel, Material, UnitSystem};

use crate::material_library::{LibraryResult, MaterialLibraryEditor};
use crate::model::Model;
use crate::setup::{Editor, EditorResult, NewItem};
use crate::tree::TreeItem;

pub enum MaterialSetResult {
    Open,
    /// Closed with OK: the materials, stored in SI units, their exposed parameters, and the
    /// unit system they are shown in.
    Ok(Vec<Material>, Vec<Exposed>, UnitSystem),
    Cancel,
}

pub struct MaterialSetEditor {
    /// The block edited.
    pub id: u64,
    name: String,
    /// What the dialogs of the FE Model tree edit: a model without a mesh whose setup has
    /// only the materials.
    model: Model,
    selected: Option<usize>,
    item: Option<Editor>,
    library: Option<MaterialLibraryEditor>,
    /// The materials of the project's analyses, by analysis, to import.
    analyses: Vec<(String, Vec<Material>)>,
    /// The blocks value links give exposed parameters from, by key.
    linked: Vec<(u32, String)>,
}

impl MaterialSetEditor {
    pub fn new(
        id: u64,
        name: &str,
        fe: &FeModel,
        units: UnitSystem,
        analyses: Vec<(String, Vec<Material>)>,
        linked: Vec<(u32, String)>,
    ) -> Self {
        let materials = &fe.materials;
        let mut model = Model::new(Path::new(""), FeMesh::default());
        model.fe = FeModel {
            materials: materials.to_vec(),
            exposed: fe.exposed.clone(),
            ..Default::default()
        };
        model.fe.properties.units = units;
        Self {
            id,
            name: name.to_owned(),
            model,
            selected: (!materials.is_empty()).then_some(0),
            item: None,
            library: None,
            analyses,
            linked,
        }
    }

    fn materials(&mut self) -> &mut Vec<Material> {
        &mut self.model.fe.materials
    }

    pub fn rename(&mut self, name: &str) {
        name.clone_into(&mut self.name);
    }

    fn edit(&mut self, index: usize) {
        self.selected = Some(index);
        let item = TreeItem::Material(index);
        self.item = Editor::edit(&item, &self.model.fe, &self.model.mesh);
    }

    /// Copies the materials of an analysis; a material of the same name is replaced.
    fn import(&mut self, materials: &[Material]) {
        for material in materials {
            let own = self.materials();
            match own.iter_mut().find(|m| m.name == material.name) {
                Some(slot) => *slot = material.clone(),
                None => own.push(material.clone()),
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> MaterialSetResult {
        // The dialogs of a material and the library come over this window and take its
        // input while they are open.
        let busy = self.item.is_some() || self.library.is_some();
        let mut result = MaterialSetResult::Open;
        let mut open = true;
        egui::Window::new(format!("Materials - {}", self.name))
            .id(egui::Id::new(("material set", self.id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.add_enabled_ui(!busy, |ui| result = self.body(ui));
            });
        if !open {
            result = MaterialSetResult::Cancel;
        }
        if let Some(editor) = &mut self.item {
            editor.set_linked(self.linked.clone());
            match editor.show(ctx, &self.model) {
                EditorResult::Open => {}
                EditorResult::Cancel => self.item = None,
                EditorResult::Ok => {
                    let editor = self.item.take().expect("shown above");
                    let count = self.model.fe.materials.len();
                    editor.apply(&mut self.model.fe);
                    if self.model.fe.materials.len() > count {
                        self.selected = Some(count);
                    }
                }
            }
        }
        if let Some(library) = &mut self.library {
            match library.show(ctx) {
                LibraryResult::Open => {}
                LibraryResult::Cancel => self.library = None,
                LibraryResult::Ok(materials) => {
                    self.library = None;
                    if let Some(materials) = materials {
                        *self.materials() = materials;
                        self.selected =
                            self.selected.filter(|&i| i < self.model.fe.materials.len());
                    }
                }
            }
        }
        result
    }

    fn body(&mut self, ui: &mut egui::Ui) -> MaterialSetResult {
        let mut result = MaterialSetResult::Open;
        ui.horizontal(|ui| {
            ui.label("Shown in");
            let units = &mut self.model.fe.properties.units;
            egui::ComboBox::from_id_salt("material set units")
                .selected_text(units.label())
                .width(220.0)
                .show_ui(ui, |ui| {
                    for choice in UnitSystem::ALL {
                        ui.selectable_value(units, choice, choice.label());
                    }
                });
        })
        .response
        .on_hover_text(
            "The unit system the values are shown and typed in; they are stored in SI units, \
             and every analysis that links them shows them in its own.",
        );
        ui.add_space(4.0);
        ui.horizontal_top(|ui| {
            crate::keywords::frame().show(ui, |ui| {
                ui.set_min_size(egui::vec2(220.0, 240.0));
                ui.set_max_width(220.0);
                egui::ScrollArea::vertical()
                    .id_salt("material set list")
                    .max_height(240.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| ui.vertical(|ui| self.list(ui)));
            });
            ui.vertical(|ui| {
                ui.set_width(150.0);
                let wide = |text: &str| egui::Button::new(text).min_size(egui::vec2(150.0, 0.0));
                if ui.add(wide("New …")).clicked() {
                    self.item = Editor::create(NewItem::Material, &self.model.fe);
                }
                let selected = self.selected;
                if ui.add_enabled(selected.is_some(), wide("Edit …")).clicked()
                    && let Some(index) = selected
                {
                    self.edit(index);
                }
                if ui.add_enabled(selected.is_some(), wide("Delete")).clicked()
                    && let Some(index) = selected
                {
                    self.materials().remove(index);
                    let left = self.model.fe.materials.len();
                    self.selected = (left > 0).then(|| index.min(left - 1));
                }
                ui.separator();
                let tip = "Opens the material library: copy materials from it into this block, \
                           or from this block into the library.";
                if ui
                    .add(wide("Add from Library …"))
                    .on_hover_text(tip)
                    .clicked()
                {
                    self.library = Some(MaterialLibraryEditor::new(
                        &self.model.fe.materials,
                        self.model.fe.properties.units,
                    ));
                }
                let analyses = std::mem::take(&mut self.analyses);
                let with_materials = analyses.iter().any(|(_, m)| !m.is_empty());
                ui.add_enabled_ui(with_materials, |ui| {
                    ui.menu_button("Import from Analysis", |ui| {
                        for (name, materials) in &analyses {
                            let text = format!("{name} ({})", materials.len());
                            let button =
                                ui.add_enabled(!materials.is_empty(), egui::Button::new(text));
                            if button.clicked() {
                                self.import(materials);
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Copies the materials of an analysis into this block, so that \
                         analyses can link them from here; one of the same name is replaced.",
                    );
                });
                self.analyses = analyses;
            });
        });
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if ui.button("Cancel").clicked() {
                result = MaterialSetResult::Cancel;
            }
            if ui.button("OK").clicked() {
                result = MaterialSetResult::Ok(
                    self.model.fe.materials.clone(),
                    self.model.fe.exposed.clone(),
                    self.model.fe.properties.units,
                );
            }
        });
        result
    }

    fn list(&mut self, ui: &mut egui::Ui) {
        if self.model.fe.materials.is_empty() {
            ui.weak("No materials yet: New … or Add from Library …");
            return;
        }
        let mut edit = None;
        ui.spacing_mut().item_spacing.y = 1.0;
        for (index, material) in self.model.fe.materials.iter().enumerate() {
            let selected = self.selected == Some(index);
            let text = egui::RichText::new(&material.name);
            let row = crate::keywords::tree_row(ui, 0, None, selected, text);
            if row.clicked() {
                self.selected = Some(index);
            }
            if row.double_clicked() {
                edit = Some(index);
            }
        }
        if let Some(index) = edit {
            self.edit(index);
        }
    }
}
