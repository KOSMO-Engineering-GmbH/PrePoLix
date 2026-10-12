//! The project window, Ansys Workbench's project schematic: the analyses of the project as
//! blocks of a node graph, each with the state of its cells. An analysis opens in a window of
//! its own, the [`AnalysisWindow`], which looks like PrePoMax's main window.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer, SnarlWidget};
use egui_snarl::{InPin, NodeId, OutPin, Snarl};
use plx_io::project::{Link, Project, ProjectAnalysis};
use plx_model::{ModelSpace, UnitSystem};

use crate::app::{AnalysisWindow, Request};
use crate::icons::{self, Icon};
use crate::model::PROJECT_EXTENSION;
use crate::model_properties::{DialogResult, ModelPropertiesDialog};
use crate::settings::{self, Settings, SettingsWindow, WindowResult};
use crate::status::{self, AnalysisStatus, CellState};
use crate::tree_icons::{self, TreeIcon};

/// Storage key of the project file open at exit, reopened at the next start.
const LAST_PROJECT_KEY: &str = "last_project";

/// An analysis of the project with its block in the schematic.
struct Node {
    id: u64,
    name: String,
    /// The analysis' data while its window is closed; an open window holds it.
    data: Option<ProjectAnalysis>,
    window: Option<Box<AnalysisWindow>>,
    /// Where the window opens the first time: over the project window.
    window_position: Option<egui::Pos2>,
    /// The state of its cells, as last found.
    status: Option<AnalysisStatus>,
    /// Names of its steps, shown in its block.
    steps: Vec<String>,
}

impl Node {
    fn new(id: u64, name: String, data: ProjectAnalysis) -> Self {
        let mut node = Self {
            id,
            name,
            data: Some(data),
            window: None,
            window_position: None,
            status: None,
            steps: Vec::new(),
        };
        node.refresh_closed();
        node
    }

    /// Finds the state of a closed analysis from its data, once after it changed.
    fn refresh_closed(&mut self) {
        if let Some(data) = &self.data {
            self.status = Some(status::analysis_status(
                data.geometry.as_ref(),
                &data.mesh,
                &data.model,
                &data.mesh_inputs,
                data.results.as_ref(),
                false,
            ));
            self.steps = data.model.steps.iter().map(|s| s.name.clone()).collect();
        }
    }

    /// The analysis' data as the project saves it, from its window if it is open.
    fn data(&mut self) -> Option<ProjectAnalysis> {
        match &mut self.window {
            Some(window) => window.data(),
            None => self.data.clone(),
        }
    }
}

/// What the user did in the schematic, done after it is drawn.
enum Action {
    New(egui::Pos2),
    /// Open the analysis' window, on the tab of a cell if given.
    Open(u64),
    Solve(u64),
    Rename(u64),
    Duplicate(u64),
    Delete(u64),
}

pub struct ProjectApp {
    render_state: egui_wgpu::RenderState,
    settings: Settings,
    settings_window: Option<SettingsWindow>,
    updater: crate::update::Updater,
    output: Vec<String>,
    /// The project file; `None` until the project is saved.
    path: Option<PathBuf>,
    space: ModelSpace,
    units: UnitSystem,
    nodes: Vec<Node>,
    links: Vec<Link>,
    /// The schematic: a block per analysis, by its id.
    graph: Snarl<u64>,
    /// Name typed for an analysis being renamed, with why it cannot be taken.
    rename: Option<(u64, String)>,
    /// Analysis to delete once the user agrees.
    confirm_delete: Option<u64>,
    /// Model space and unit system of the project, while their dialog is open.
    properties_dialog: Option<ModelPropertiesDialog>,
    /// A project file being read on a worker thread.
    opening: Option<(PathBuf, Receiver<Result<Project, String>>)>,
    /// Analyses Update Project still has to solve, in order, and the one it solves.
    update_queue: VecDeque<u64>,
    updating: Option<u64>,
    /// Files given on the command line, imported into the first analysis once it is open.
    startup_import: Vec<PathBuf>,
}

impl ProjectApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .ok_or("PrePoLix requires the wgpu backend of eframe")?;
        crate::style::apply(&cc.egui_ctx);
        let adapter = render_state.adapter.get_info();
        let output = vec![format!(
            "PrePoLix {} started, graphics: {} ({:?})",
            env!("CARGO_PKG_VERSION"),
            adapter.name,
            adapter.backend
        )];
        let mut settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, settings::STORAGE_KEY))
            .unwrap_or_default();
        settings.upgrade();
        crate::style::set_theme(&cc.egui_ctx, settings.graphics.theme);
        plx_mesher::set_library_path(settings.gmsh.library());
        let mut app = Self {
            render_state,
            space: settings.new_model.space,
            units: settings.new_model.units,
            settings,
            settings_window: None,
            updater: Default::default(),
            output,
            path: None,
            nodes: Vec::new(),
            links: Vec::new(),
            graph: Snarl::new(),
            rename: None,
            confirm_delete: None,
            properties_dialog: None,
            opening: None,
            update_queue: VecDeque::new(),
            updating: None,
            startup_import: Vec::new(),
        };
        if app.settings.updates.check_at_startup && app.updater.installation().checks_at_startup() {
            app.updater.check(&cc.egui_ctx, false);
        }
        // A project on the command line opens; other files, such as a STEP or input file,
        // are imported into a new project's first analysis.
        let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
        let (projects, files): (Vec<PathBuf>, Vec<PathBuf>) = paths
            .into_iter()
            .partition(|p| crate::model::is_project_file(p));
        if let Some(project) = projects.into_iter().next() {
            app.open(&cc.egui_ctx, project);
        } else if !files.is_empty() {
            let name = (files[0].file_stem())
                .map_or_else(|| "Analysis-1".into(), |s| s.to_string_lossy().into_owned());
            let id = app.add_analysis(&name, egui::pos2(40.0, 40.0));
            app.startup_import = files;
            app.open_window(&cc.egui_ctx, id);
        } else if let Some(path) = cc.storage.and_then(stored_last_project) {
            // Without files on the command line, the project open at the last exit opens.
            app.open(&cc.egui_ctx, path);
        }
        Ok(app)
    }

    fn node(&mut self, id: u64) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    fn project_name(&self) -> String {
        (self.path.as_ref())
            .and_then(|p| p.file_stem())
            .map_or_else(|| "Untitled".into(), |s| s.to_string_lossy().into_owned())
    }

    /// A name no analysis has: `base`, else `base-2`, `base-3` …
    fn unique_name(&self, base: &str) -> String {
        let taken = |name: &str| self.nodes.iter().any(|n| n.name.eq_ignore_ascii_case(name));
        if !taken(base) {
            return base.to_owned();
        }
        (2..)
            .map(|i| format!("{base}-{i}"))
            .find(|name| !taken(name))
            .unwrap_or_default()
    }

    fn next_id(&self) -> u64 {
        self.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1
    }

    /// Adds an empty analysis in the project's model space and units; returns its id.
    fn add_analysis(&mut self, name: &str, position: egui::Pos2) -> u64 {
        let id = self.next_id();
        let name = self.unique_name(name);
        let mut data = ProjectAnalysis {
            id,
            name: name.clone(),
            ..Default::default()
        };
        data.model.properties.space = self.space;
        data.model.properties.units = self.units;
        self.nodes.push(Node::new(id, name, data));
        self.graph.insert_node(position, id);
        id
    }

    /// The analysis' block in the schematic.
    fn graph_node(&self, id: u64) -> Option<NodeId> {
        (self.graph.nodes_ids_data())
            .find(|(_, node)| node.value == id)
            .map(|(graph_id, _)| graph_id)
    }

    fn open_window(&mut self, ctx: &egui::Context, id: u64) {
        let path = self.model_path();
        let folder = crate::analysis::job_name(&self.project_name());
        let (render_state, settings) = (self.render_state.clone(), self.settings.clone());
        let at = ctx.input(|i| i.viewport().outer_rect).map(|r| r.min);
        let import = std::mem::take(&mut self.startup_import);
        let Some(node) = self.node(id) else {
            return;
        };
        if node.window.is_some() {
            // Brought to the front.
            let viewport = analysis_viewport(id);
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
            return;
        }
        let Some(data) = node.data.take() else {
            return;
        };
        let mut window = AnalysisWindow::new(render_state, settings, &node.name);
        window.set_project_folder(folder);
        window.load(ctx, path, data);
        if !import.is_empty() {
            window.import_after_load(import);
        }
        node.window = Some(Box::new(window));
        node.window_position = at;
    }

    /// The path the analyses' models take: the project file, or a name for an unsaved one.
    fn model_path(&self) -> PathBuf {
        (self.path.clone())
            .unwrap_or_else(|| PathBuf::from(format!("Untitled.{PROJECT_EXTENSION}")))
    }

    /// Closes the analysis' window; its data goes back to the project. Not while the window
    /// works on something that would be lost.
    fn close_window(&mut self, id: u64) -> bool {
        let Some(node) = self.node(id) else {
            return true;
        };
        let Some(window) = &mut node.window else {
            return true;
        };
        if let Some(reason) = window.busy() {
            let line = format!("{} stays open while {reason}.", node.name);
            window.message(line.clone());
            self.output.push(line);
            return false;
        }
        node.data = window.data();
        node.window = None;
        node.refresh_closed();
        true
    }

    fn close_all_windows(&mut self) -> bool {
        let ids: Vec<u64> = self.nodes.iter().map(|n| n.id).collect();
        ids.into_iter().all(|id| self.close_window(id))
    }

    /// Starts a new, empty project.
    fn new_project(&mut self) {
        if !self.close_all_windows() {
            return;
        }
        self.nodes.clear();
        self.links.clear();
        self.graph = Snarl::new();
        self.path = None;
        self.space = self.settings.new_model.space;
        self.units = self.settings.new_model.units;
        self.update_queue.clear();
        self.updating = None;
        self.output.push("New project".into());
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .set_title("Open Project")
            .add_filter("PrePoLix project (*.plx)", &["plx", "PLX"])
            .pick_file();
        if let Some(path) = picked {
            self.open(ctx, path);
        }
    }

    /// Reads the project file on a worker thread; it replaces the open project once read.
    fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.opening.is_some() {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let (file, ctx) = (path.clone(), ctx.clone());
        std::thread::spawn(move || {
            let project = plx_io::project::read_project(&file).map_err(|e| e.to_string());
            let _ = sender.send(project);
            ctx.request_repaint();
        });
        self.opening = Some((path, receiver));
    }

    fn poll_opening(&mut self) {
        let Some((path, receiver)) = &self.opening else {
            return;
        };
        let Ok(result) = receiver.try_recv() else {
            return;
        };
        let path = path.clone();
        self.opening = None;
        match result {
            Ok(project) => {
                if !self.close_all_windows() {
                    return;
                }
                self.nodes.clear();
                self.graph = Snarl::new();
                self.space = project.space;
                self.units = project.units;
                self.links = project.links;
                for analysis in project.analyses {
                    let [x, y] = analysis.position;
                    self.graph.insert_node(egui::pos2(x, y), analysis.id);
                    self.nodes
                        .push(Node::new(analysis.id, analysis.name.clone(), analysis));
                }
                self.output.push(format!(
                    "{} opened: {} analyses",
                    path.display(),
                    self.nodes.len()
                ));
                self.path = Some(path);
                self.update_queue.clear();
                self.updating = None;
            }
            Err(error) => self.output.push(format!("Not opened: {error}")),
        }
    }

    /// The project as it is saved, with the analyses' data from their windows; `results`
    /// false leaves out the results the analyses keep.
    fn project(&mut self, results: bool) -> Project {
        let positions: Vec<(u64, egui::Pos2)> = (self.graph.nodes_pos_ids())
            .map(|(_, pos, id)| (*id, pos))
            .collect();
        let mut analyses = Vec::with_capacity(self.nodes.len());
        for node in &mut self.nodes {
            let Some(mut data) = node.data() else {
                continue;
            };
            data.id = node.id;
            data.name = node.name.clone();
            if let Some((_, pos)) = positions.iter().find(|(id, _)| *id == node.id) {
                data.position = [pos.x, pos.y];
            }
            if !results {
                data.results = None;
            }
            analyses.push(data);
        }
        Project {
            space: self.space,
            units: self.units,
            analyses,
            links: self.links.clone(),
            ..Default::default()
        }
    }

    /// Saves the project to its file, or to one the user picks for a new project or with
    /// `save_as`.
    fn save(&mut self, save_as: bool) {
        if let Some(name) = self
            .nodes
            .iter()
            .find(|n| n.window.as_ref().is_some_and(|w| w.model().is_none()))
        {
            self.output
                .push(format!("Not saved: {} is still loading.", name.name));
            return;
        }
        let path = match &self.path {
            Some(path) if !save_as => path.clone(),
            _ => {
                let Some(path) = pick_project_file(self.path.as_deref(), "Save Project", "") else {
                    return;
                };
                path
            }
        };
        let project = self.project(true);
        match plx_io::project::save_project(&path, &project) {
            Ok(()) => {
                let line = timestamped(format!("{} saved", path.display()));
                self.output.push(line.clone());
                let folder = crate::analysis::job_name(
                    &path.file_stem().unwrap_or_default().to_string_lossy(),
                );
                for node in &mut self.nodes {
                    if let Some(window) = &mut node.window {
                        window.saved(&path);
                        window.set_project_folder(folder.clone());
                        window.message(line.clone());
                    }
                }
                self.path = Some(path);
            }
            Err(error) => self.output.push(format!("Not saved: {error}")),
        }
    }

    /// Saves a copy of the project without the results of its analyses, for example to send
    /// it; the open project stays on its own file and keeps its results.
    fn save_without_results(&mut self) {
        let Some(path) = pick_project_file(
            self.path.as_deref(),
            "Save Project Without Results",
            "-without-results",
        ) else {
            return;
        };
        let project = self.project(false);
        match plx_io::project::save_project(&path, &project) {
            Ok(()) => self.output.push(timestamped(format!(
                "{} saved without results",
                path.display()
            ))),
            Err(error) => self.output.push(format!("Not saved: {error}")),
        }
    }

    fn duplicate(&mut self, id: u64) {
        let position = (self.graph_node(id))
            .and_then(|g| self.graph.get_node_info(g))
            .map_or(egui::pos2(40.0, 40.0), |info| {
                info.pos + egui::vec2(40.0, 260.0)
            });
        let Some(node) = self.node(id) else {
            return;
        };
        let Some(mut data) = node.data() else {
            return;
        };
        let name = format!("Copy of {}", node.name);
        let new_id = self.next_id();
        let name = self.unique_name(&name);
        data.id = new_id;
        data.name = name.clone();
        self.nodes.push(Node::new(new_id, name.clone(), data));
        self.graph.insert_node(position, new_id);
        self.output.push(format!("{name} created"));
    }

    fn delete(&mut self, id: u64) {
        if !self.close_window(id) {
            return;
        }
        if let Some(graph_id) = self.graph_node(id) {
            self.graph.remove_node(graph_id);
        }
        if let Some(index) = self.nodes.iter().position(|n| n.id == id) {
            let node = self.nodes.remove(index);
            self.output.push(format!("{} deleted", node.name));
        }
        self.links.retain(|l| l.source != id && l.target != id);
        self.update_queue.retain(|&queued| queued != id);
    }

    /// Ansys Workbench's Update Project: solves every analysis that is not up to date, one
    /// after the other.
    fn update_project(&mut self) {
        self.update_queue = (self.nodes.iter())
            .filter(|n| n.status.as_ref().is_some_and(AnalysisStatus::needs_solve))
            .filter(|n| {
                n.status.as_ref().is_some_and(|s| {
                    s.mesh != CellState::Missing || s.geometry == CellState::UpToDate
                })
            })
            .map(|n| n.id)
            .collect();
        if self.update_queue.is_empty() {
            self.output.push("All analyses are up to date.".into());
        }
    }

    /// Works through Update Project: opens the next analysis and solves it, then the next
    /// once the one before is done.
    fn run_update(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.updating {
            let Some(node) = self.node(id) else {
                self.updating = None;
                return;
            };
            let Some(window) = &mut node.window else {
                self.updating = None;
                return;
            };
            if window.waiting() || window.solving() || window.is_running() {
                return;
            }
            // Done: solved, or stopped by an error the window shows.
            let failed = window.status().is_some_and(|s| s.needs_solve());
            let name = node.name.clone();
            self.updating = None;
            if failed {
                self.update_queue.clear();
                self.output
                    .push(format!("Update stopped: {name} could not be solved."));
            }
            return;
        }
        let Some(id) = self.update_queue.pop_front() else {
            return;
        };
        self.open_window(ctx, id);
        self.updating = Some(id);
        if let Some(window) = self.node(id).and_then(|n| n.window.as_mut()) {
            window.solve_when_loaded();
        }
    }

    /// Applies the project's model space and units to every analysis; `convert` converts
    /// their values into the new units, otherwise the numbers stay and apply in them.
    fn set_space_and_units(&mut self, space: ModelSpace, units: UnitSystem, convert: bool) {
        // A 2D model space takes no solid elements, and so on.
        for node in &self.nodes {
            let mesh = match (&node.window, &node.data) {
                (Some(window), _) => window.model().map(|m| &m.mesh),
                (None, Some(data)) => Some(&data.mesh),
                _ => None,
            };
            if let Some(error) = mesh.and_then(|m| crate::model_properties::space_error(space, m)) {
                self.output
                    .push(format!("Model space not changed: {}: {error}", node.name));
                return;
            }
        }
        self.space = space;
        self.units = units;
        for node in &mut self.nodes {
            match &mut node.window {
                Some(window) => window.set_space_and_units(space, units, convert),
                None => {
                    if let Some(data) = &mut node.data {
                        let notes = crate::model::convert_analysis(data, space, units, convert);
                        self.output.extend(notes);
                    }
                    node.refresh_closed();
                }
            }
        }
        self.output
            .push(format!("Project: {}, {}", space.label(), units.label()));
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Project").clicked() {
                    self.new_project();
                }
                let open = egui::Button::new("Open …").shortcut_text("Ctrl+O");
                if ui.add_enabled(self.opening.is_none(), open).clicked() {
                    self.open_dialog(ui.ctx());
                }
                let save = egui::Button::new("Save").shortcut_text("Ctrl+S");
                if ui.add(save).clicked() {
                    self.save(false);
                }
                let save_as = egui::Button::new("Save As …").shortcut_text("Ctrl+Shift+S");
                if ui.add(save_as).clicked() {
                    self.save(true);
                }
                let tip = "Saves a copy with only the meshes and the model definitions, \
                           without the results the analyses keep. The open project keeps them.";
                if ui
                    .button("Save Without Results …")
                    .on_hover_text(tip)
                    .clicked()
                {
                    self.save_without_results();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Project", |ui| {
                if ui.button("New Analysis").clicked() {
                    let at = self.free_position();
                    self.add_analysis("Analysis-1", at);
                }
                let update = egui::Button::new("Update Project");
                if ui.add(update).on_hover_text(UPDATE_TIP).clicked() {
                    self.update_project();
                }
                ui.separator();
                if ui.button("Model Space and Units …").clicked() {
                    let mut properties = self.settings.new_model.clone();
                    properties.space = self.space;
                    properties.units = self.units;
                    self.properties_dialog = Some(ModelPropertiesDialog::edit(properties));
                }
            });
            ui.menu_button("Tools", |ui| {
                if ui.button("Settings …").clicked() {
                    self.settings_window = Some(SettingsWindow::new(&self.settings));
                }
            });
            ui.menu_button("Help", |ui| {
                let tip = "Opens the user guide in the web browser";
                if ui.button("User Guide").on_hover_text(tip).clicked() {
                    let line = crate::help::open_user_guide();
                    self.output.push(line);
                }
                let updater = &mut self.updater;
                if ui
                    .add_enabled(!updater.busy(), egui::Button::new("Check for Updates …"))
                    .on_hover_text("Asks GitHub whether a newer version of PrePoLix is available")
                    .clicked()
                {
                    updater.check(ui.ctx(), true);
                }
            });
            // Version number, greyed out at the far right of the menu bar, with the symbol of
            // a newer release left of it.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                        .color(ui.visuals().weak_text_color()),
                );
                self.updater.indicator(ui);
            });
        });
    }

    fn tool_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            if icons::button(ui, Icon::New, "New Analysis", true, false).clicked() {
                let at = self.free_position();
                self.add_analysis("Analysis-1", at);
            }
            let can_open = self.opening.is_none();
            if icons::button(ui, Icon::Open, "Open Project (Ctrl+O)", can_open, false).clicked() {
                self.open_dialog(ui.ctx());
            }
            if icons::button(ui, Icon::Save, "Save Project (Ctrl+S)", true, false).clicked() {
                self.save(false);
            }
            ui.separator();
            let updating = self.updating.is_some() || !self.update_queue.is_empty();
            if icons::button(ui, Icon::Solve, UPDATE_TIP, !updating, false).clicked() {
                self.update_project();
            }
            if icons::button(ui, Icon::Stop, "Stop Update Project", updating, false).clicked() {
                self.update_queue.clear();
                self.updating = None;
                self.output.push("Update stopped".into());
            }
            ui.add_space(6.0);
            ui.label("Update Project");
        });
    }

    /// A place for a new block: right of the last one.
    fn free_position(&self) -> egui::Pos2 {
        (self.graph.nodes_pos())
            .map(|(pos, _)| pos)
            .max_by(|a, b| a.x.total_cmp(&b.x))
            .map_or(egui::pos2(40.0, 40.0), |pos| pos + egui::vec2(260.0, 0.0))
    }

    fn schematic(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut viewer = Viewer {
            nodes: (self.nodes.iter())
                .map(|n| BlockView {
                    id: n.id,
                    name: n.name.clone(),
                    status: n.status.clone(),
                    steps: n.steps.clone(),
                    open: n.window.is_some(),
                })
                .collect(),
            actions: Vec::new(),
        };
        SnarlWidget::new()
            .id_salt("project schematic")
            .style(schematic_style())
            .show(&mut self.graph, &mut viewer, ui);
        if self.nodes.is_empty() {
            ui.put(
                egui::Rect::from_center_size(ui.max_rect().center(), egui::vec2(420.0, 60.0)),
                egui::Label::new(
                    egui::RichText::new(
                        "Project > New Analysis, or right-click here, adds an analysis.\n\
                         Double-click an analysis to open it in its own window.",
                    )
                    .weak(),
                ),
            );
        }
        viewer.actions
    }

    fn act(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::New(pos) => {
                self.add_analysis("Analysis-1", pos);
            }
            Action::Open(id) => self.open_window(ctx, id),
            Action::Solve(id) => {
                self.update_queue.retain(|&queued| queued != id);
                self.update_queue.push_front(id);
            }
            Action::Rename(id) => {
                let name = self
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| n.name.clone());
                self.rename = name.map(|name| (id, name));
            }
            Action::Duplicate(id) => self.duplicate(id),
            Action::Delete(id) => self.confirm_delete = Some(id),
        }
    }

    fn rename_window(&mut self, ctx: &egui::Context) {
        let Some((id, mut name)) = self.rename.take() else {
            return;
        };
        let mut open = true;
        let (mut done, mut cancelled) = (false, false);
        let taken =
            (self.nodes.iter()).any(|n| n.id != id && n.name.eq_ignore_ascii_case(name.trim()));
        let valid = !name.trim().is_empty() && !taken;
        egui::Window::new("Rename Analysis")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let edit = ui.text_edit_singleline(&mut name);
                    edit.request_focus();
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid {
                        done = true;
                    }
                });
                if taken {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        "Another analysis has this name.",
                    );
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid, egui::Button::new("OK")).clicked() {
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });
        if cancelled {
            open = false;
        }
        if done {
            let name = name.trim().to_owned();
            if let Some(node) = self.node(id) {
                node.name = name.clone();
                if let Some(window) = &mut node.window {
                    window.rename(&name);
                }
            }
        } else if open {
            self.rename = Some((id, name));
        }
    }

    fn confirm_delete_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete else {
            return;
        };
        let name =
            (self.nodes.iter().find(|n| n.id == id)).map_or_else(String::new, |n| n.name.clone());
        let mut answer = None;
        egui::Window::new("Delete Analysis")
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.label(format!(
                    "Delete {name} with its model and results from the project?"
                ));
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        answer = Some(false);
                    }
                });
            });
        match answer {
            Some(true) => {
                self.confirm_delete = None;
                self.delete(id);
            }
            Some(false) => self.confirm_delete = None,
            None => {}
        }
    }

    fn properties_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.properties_dialog else {
            return;
        };
        let convert = dialog.convert;
        match dialog.show(ctx, None, None) {
            DialogResult::Open => {}
            DialogResult::Cancel => self.properties_dialog = None,
            DialogResult::Ok(properties) => {
                self.properties_dialog = None;
                self.set_space_and_units(properties.space, properties.units, convert);
            }
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let Some(window) = &mut self.settings_window else {
            return;
        };
        let new = match window.show(ctx) {
            WindowResult::Open => return,
            WindowResult::Apply(settings) => settings,
            WindowResult::Ok(settings) => {
                self.settings_window = None;
                settings
            }
            WindowResult::Cancel => {
                self.settings_window = None;
                return;
            }
        };
        if new.gmsh != self.settings.gmsh && !plx_mesher::set_library_path(new.gmsh.library()) {
            self.output
                .push("The changed Gmsh library is loaded after a restart".into());
        }
        if new.graphics.theme != self.settings.graphics.theme {
            crate::style::set_theme(ctx, new.graphics.theme);
        }
        self.settings = new;
        self.share_settings();
    }

    /// Gives every analysis window the program's settings.
    fn share_settings(&mut self) {
        for node in &mut self.nodes {
            if let Some(window) = &mut node.window {
                window.set_settings(&self.settings);
            }
        }
    }

    /// Shows the open analyses' windows and acts on what they ask.
    fn analysis_windows(&mut self, ctx: &egui::Context) {
        let project = self.project_name();
        let mut requests = Vec::new();
        let mut closing = Vec::new();
        let mut settings = None;
        for node in &mut self.nodes {
            let Some(window) = &mut node.window else {
                continue;
            };
            let mut builder = egui::ViewportBuilder::default()
                .with_title(format!("{} - {project} - PrePoLix", node.name))
                .with_app_id("prepolix")
                .with_inner_size([1400.0, 900.0])
                .with_min_inner_size([640.0, 400.0])
                .with_icon(std::sync::Arc::new(crate::window_icon()));
            if let Some(position) = node.window_position {
                builder = builder.with_position(position);
            }
            let id = node.id;
            ctx.show_viewport_immediate(analysis_viewport(id), builder, |ui, _| {
                window.ui(ui);
                if ui.input(|i| i.viewport().close_requested()) {
                    if window.busy().is_some() {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    }
                    closing.push(id);
                }
            });
            requests.extend(window.take_requests().into_iter().map(|r| (id, r)));
            if window.settings() != &self.settings {
                settings = Some(window.settings().clone());
            }
            // The block shows the state live.
            node.status = window.status();
            if let Some(model) = window.model() {
                node.steps = model.fe.steps.iter().map(|s| s.name.clone()).collect();
            }
        }
        if let Some(settings) = settings {
            self.settings = settings;
            self.share_settings();
        }
        for id in closing {
            self.close_window(id);
        }
        for (_, request) in requests {
            match request {
                Request::Save(save_as) => self.save(save_as),
                Request::SaveWithoutResults => self.save_without_results(),
                Request::OpenProject(path) => self.open(ctx, path),
                Request::SpaceAndUnits(space, units, convert) => {
                    self.set_space_and_units(space, units, convert)
                }
            }
        }
    }
}

/// The analysis window's viewport.
fn analysis_viewport(id: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("analysis", id))
}

impl eframe::App for ProjectApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, settings::STORAGE_KEY, &self.settings);
        let last = self.path.clone().filter(|p| p.is_file());
        eframe::set_value(storage, LAST_PROJECT_KEY, &last);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::style::sync(&ctx);
        self.poll_opening();
        let title = format!("{} - PrePoLix", self.project_name());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.open_dialog(&ctx);
        }
        let shift_command = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        if ctx.input_mut(|i| i.consume_key(shift_command, egui::Key::S)) {
            self.save(true);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            self.save(false);
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            (i.raw.dropped_files.iter())
                .map(|f| f.path().to_path_buf())
                .filter(|p| crate::model::is_project_file(p))
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            self.open(&ctx, path);
        }
        egui::Panel::top("project menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("project tools").show(ui, |ui| self.tool_bar(ui));
        egui::Panel::bottom("project status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some((path, _)) = &self.opening {
                    ui.spinner();
                    ui.label(format!("Opening {} …", path.display()));
                } else {
                    let file = (self.path.as_ref())
                        .map_or_else(|| "not saved yet".into(), |p| p.display().to_string());
                    ui.label(format!("Project: {file}"));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!(
                        "Unit system: {}   Model space: {}",
                        self.units.label(),
                        self.space.label()
                    ));
                });
            });
        });
        let pane = egui::Frame::new()
            .fill(crate::style::window())
            .stroke(egui::Stroke::new(1.0, crate::style::border()));
        egui::Panel::bottom("project output")
            .resizable(true)
            .default_size(110.0)
            .size_range(40.0..=400.0)
            .frame(pane.inner_margin(4))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.output {
                            ui.label(line);
                        }
                    });
            });
        let actions = egui::CentralPanel::no_frame()
            .show(ui, |ui| self.schematic(ui))
            .inner;
        for action in actions {
            self.act(&ctx, action);
        }
        self.rename_window(&ctx);
        self.confirm_delete_window(&ctx);
        self.properties_window(&ctx);
        self.settings_window(&ctx);
        self.updater.show(&ctx, &mut self.output);
        self.run_update(&ctx);
        self.analysis_windows(&ctx);
    }
}

/// How a block of the schematic looks: what the viewer needs of an analysis.
struct BlockView {
    id: u64,
    name: String,
    status: Option<AnalysisStatus>,
    steps: Vec<String>,
    open: bool,
}

struct Viewer {
    nodes: Vec<BlockView>,
    actions: Vec<Action>,
}

impl Viewer {
    fn block(&self, id: u64) -> Option<&BlockView> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

/// The symbol of each cell of a block, as in the analysis tree.
fn cell_icon(cell: &str) -> TreeIcon {
    match cell {
        "Geometry" => TreeIcon::Geometry,
        "Mesh" => TreeIcon::Mesh,
        "Setup" => TreeIcon::Steps,
        "Solution" => TreeIcon::Analysis,
        _ => TreeIcon::FieldOutput,
    }
}

impl SnarlViewer<u64> for Viewer {
    fn title(&mut self, node: &u64) -> String {
        self.block(*node)
            .map_or_else(String::new, |b| b.name.clone())
    }

    fn show_header(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u64>,
    ) {
        let id = snarl[node];
        let Some(block) = self.block(id) else {
            return;
        };
        let text = egui::RichText::new(&block.name).strong();
        let label = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
        if block.open {
            ui.weak("(open)");
        }
        if label.double_clicked() {
            self.actions.push(Action::Open(id));
        }
        label.on_hover_text("Double-click opens the analysis in its own window.");
    }

    fn inputs(&mut self, _node: &u64) -> usize {
        0
    }

    fn show_input(
        &mut self,
        _pin: &InPin,
        _ui: &mut egui::Ui,
        _snarl: &mut Snarl<u64>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        PinInfo::default()
    }

    fn outputs(&mut self, _node: &u64) -> usize {
        0
    }

    fn show_output(
        &mut self,
        _pin: &OutPin,
        _ui: &mut egui::Ui,
        _snarl: &mut Snarl<u64>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        PinInfo::default()
    }

    fn has_body(&mut self, _node: &u64) -> bool {
        true
    }

    fn show_body(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u64>,
    ) {
        let id = snarl[node];
        let Some(block) = self.block(id) else {
            return;
        };
        let status = block.status.clone();
        let steps = block.steps.clone();
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let Some(status) = status else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Loading …");
                });
                return;
            };
            for (cell, state) in status.cells() {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(180.0, 20.0), egui::Sense::click());
                let painter = ui.painter();
                if response.hovered() {
                    painter.rect_filled(rect, 2.0, crate::style::hover_fill());
                }
                let icon = egui::pos2(rect.left() + 2.0, rect.center().y - 8.0);
                tree_icons::paint(painter, icon, cell_icon(cell));
                painter.text(
                    egui::pos2(rect.left() + 24.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    cell,
                    egui::TextStyle::Body.resolve(ui.style()),
                    ui.visuals().text_color(),
                );
                let symbol = egui::pos2(rect.right() - 18.0, rect.center().y - 8.0);
                status::paint(painter, symbol, state);
                let mut tip = format!("{cell}: {}", state.text());
                if cell == "Mesh" && !status.stale_parts.is_empty() {
                    tip.push_str(&format!(" ({})", status.stale_parts.join(", ")));
                }
                if response.on_hover_text(tip).double_clicked() {
                    self.actions.push(Action::Open(id));
                }
            }
            if !steps.is_empty() {
                ui.separator();
                for step in &steps {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        tree_icons::paint(ui.painter(), rect.min, TreeIcon::Step);
                        ui.label(step);
                    });
                }
            }
        });
    }

    fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<u64>) -> bool {
        true
    }

    fn show_graph_menu(&mut self, pos: egui::Pos2, ui: &mut egui::Ui, _snarl: &mut Snarl<u64>) {
        if ui.button("New Analysis").clicked() {
            self.actions.push(Action::New(pos));
            ui.close();
        }
    }

    fn has_node_menu(&mut self, _node: &u64) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u64>,
    ) {
        let id = snarl[node];
        let mut entry = |ui: &mut egui::Ui, text: &str, action: Action| {
            if ui.button(text).clicked() {
                self.actions.push(action);
                ui.close();
            }
        };
        entry(ui, "Open", Action::Open(id));
        entry(ui, "Solve", Action::Solve(id));
        ui.separator();
        entry(ui, "Rename …", Action::Rename(id));
        entry(ui, "Duplicate", Action::Duplicate(id));
        entry(ui, "Delete …", Action::Delete(id));
    }
}

fn schematic_style() -> SnarlStyle {
    SnarlStyle {
        collapsible: Some(false),
        ..SnarlStyle::new()
    }
}

/// Tooltip of Update Project.
const UPDATE_TIP: &str = "Update Project: solves every analysis that is not up to date, one \
     after the other, and keeps the new results.";

/// The project file stored at the last exit, if it still exists. It is stored as an
/// `Option`, so it has to be read back as one.
fn stored_last_project(storage: &dyn eframe::Storage) -> Option<PathBuf> {
    eframe::get_value::<Option<PathBuf>>(storage, LAST_PROJECT_KEY)
        .flatten()
        .filter(|path| path.is_file())
}

/// Asks for a project file to save to, proposing the open one's name with `suffix`.
fn pick_project_file(current: Option<&Path>, title: &str, suffix: &str) -> Option<PathBuf> {
    let stem = (current.and_then(Path::file_stem))
        .map_or_else(|| "Project".into(), |s| s.to_string_lossy().into_owned());
    let mut dialog = rfd::FileDialog::new()
        .set_title(title)
        .add_filter("PrePoLix project (*.plx)", &[PROJECT_EXTENSION])
        .set_file_name(format!("{stem}{suffix}.{PROJECT_EXTENSION}"));
    if let Some(dir) = current.and_then(Path::parent) {
        dialog = dialog.set_directory(dir);
    }
    let mut path = dialog.save_file()?;
    if path.extension().is_none() {
        path.set_extension(PROJECT_EXTENSION);
    }
    Some(path)
}

/// Prefixes an output message with the local time, `HH:MM:SS`.
fn timestamped(message: String) -> String {
    format!("{} {message}", chrono::Local::now().format("%H:%M:%S"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryStorage(HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn the_project_stored_at_exit_is_read_back() {
        let path = std::env::temp_dir().join("prepolix_test_last_project.plx");
        std::fs::write(&path, "").unwrap();
        let mut storage = MemoryStorage::default();
        eframe::set_value(&mut storage, LAST_PROJECT_KEY, &Some(path.clone()));
        assert_eq!(stored_last_project(&storage), Some(path.clone()));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(stored_last_project(&storage), None);
        eframe::set_value(&mut storage, LAST_PROJECT_KEY, &None::<PathBuf>);
        assert_eq!(stored_last_project(&storage), None);
        assert_eq!(stored_last_project(&MemoryStorage::default()), None);
    }
}
