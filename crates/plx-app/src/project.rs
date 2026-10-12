//! The project window, Ansys Workbench's project schematic: the analyses of the project as
//! blocks of a node graph, each with the state of its cells. An analysis opens in a window of
//! its own, the [`AnalysisWindow`], which looks like PrePoMax's main window.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer, SnarlWidget};
use egui_snarl::{InPin, NodeId, OutPin, Snarl};
use plx_io::project::{Link, Project, ProjectAnalysis};
use plx_model::{ModelSpace, UnitSystem};

use crate::app::{AnalysisWindow, Request};
use crate::icons::{self, Icon};
use crate::links;
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
    /// Rows of its block that are open: the setup and its categories.
    expanded: BTreeSet<Vec<String>>,
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
            expanded: BTreeSet::new(),
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
        }
    }

    /// What links read of the analysis; `None` while its window loads it.
    fn link_data(&self) -> Option<links::Data<'_>> {
        match (&self.window, &self.data) {
            (Some(window), _) => window.model().map(|m| links::Data {
                geometry: m.geometry.as_ref(),
                mesh: &m.mesh,
                fe: &m.fe,
                mesh_inputs: &m.mesh_inputs,
            }),
            (None, Some(data)) => Some(links::Data {
                geometry: data.geometry.as_ref(),
                mesh: &data.mesh,
                fe: &data.model,
                mesh_inputs: &data.mesh_inputs,
            }),
            _ => None,
        }
    }

    /// The results the analysis keeps.
    fn bound_results(&self) -> Option<&plx_io::project::BoundResults> {
        match (&self.window, &self.data) {
            (Some(window), _) => window.model()?.bound_results.as_ref(),
            (None, Some(data)) => data.results.as_ref(),
            _ => None,
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

/// What a link copies into its target.
enum Payload {
    Setup(Box<plx_model::FeModel>),
    Geometry(Option<plx_model::Geometry>),
    Mesh(Box<links::MeshCopy>),
    /// The target's step, the global step and the global results file of a submodel.
    Results(String, u32, PathBuf),
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
    /// Open or close a row of the block.
    Toggle(u64, Vec<String>),
    /// Link `item` of `source` to `slot` of `target`.
    Link {
        source: u64,
        item: Vec<String>,
        target: u64,
        slot: Vec<String>,
    },
    /// Remove the links of a pin, or the one between two pins.
    Unlink {
        source: Option<(u64, Vec<String>)>,
        target: Option<(u64, Vec<String>)>,
    },
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
    /// Counts the graphs shown, so that the view centres on the blocks of each new one.
    graph_generation: u64,
    /// Name typed for an analysis being renamed, with why it cannot be taken.
    rename: Option<(u64, String)>,
    /// Analysis to delete once the user agrees.
    confirm_delete: Option<u64>,
    /// Model space and unit system of the project, while their dialog is open.
    properties_dialog: Option<ModelPropertiesDialog>,
    /// A project file being read on a worker thread.
    opening: Option<(PathBuf, Receiver<Result<Project, String>>)>,
    /// Analyses Update Project still has to solve, in order, and the one it solves, with
    /// whether the update opened its window.
    update_queue: VecDeque<u64>,
    updating: Option<(u64, bool)>,
    /// Files given on the command line, imported into the first analysis once it is open.
    startup_import: Vec<PathBuf>,
    /// Fingerprint of the geometry or mesh a link last copied into its target, by target and
    /// path: copied again once the source's differs.
    applied: HashMap<(u64, Vec<String>), u64>,
    /// When links last brought their targets up to date, and whether they have to again now.
    last_sync: Option<Instant>,
    sync_due: bool,
    /// Global results being written for submodels, by the folder they are written to;
    /// `None` once writing failed.
    restoring: HashMap<PathBuf, Option<Receiver<Result<PathBuf, String>>>>,
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
            graph_generation: 0,
            rename: None,
            confirm_delete: None,
            properties_dialog: None,
            opening: None,
            update_queue: VecDeque::new(),
            updating: None,
            startup_import: Vec::new(),
            applied: HashMap::new(),
            last_sync: None,
            sync_due: true,
            restoring: HashMap::new(),
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
        self.applied.clear();
        self.restoring.clear();
        self.sync_due = true;
        self.graph = Snarl::new();
        self.graph_generation += 1;
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
                self.graph_generation += 1;
                self.space = project.space;
                self.units = project.units;
                self.links = project.links;
                self.applied.clear();
                self.restoring.clear();
                self.sync_due = true;
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
        self.applied.retain(|(target, _), _| *target != id);
        self.update_queue.retain(|&queued| queued != id);
    }

    /// Ansys Workbench's Update Project: solves every analysis that is not up to date, one
    /// after the other.
    fn update_project(&mut self) {
        let mut skipped = Vec::new();
        self.update_queue.clear();
        // Sources before their targets; a submodel is solved again after its global model.
        let ids: Vec<u64> = self.nodes.iter().map(|n| n.id).collect();
        for id in links::order(&ids, &self.links) {
            let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
                continue;
            };
            let after_source = (self.links.iter()).any(|l| {
                l.target == id && links::is_results(l) && self.update_queue.contains(&l.source)
            });
            match &node.status {
                Some(status) if (status.needs_solve() || after_source) && status.solvable() => {
                    self.update_queue.push_back(node.id)
                }
                Some(status) if status.needs_solve() => skipped.push(node.name.clone()),
                _ => {}
            }
        }
        for name in skipped {
            self.output.push(format!(
                "{name} skipped: it has no mesh or geometry, or no step yet."
            ));
        }
        if self.update_queue.is_empty() {
            self.output.push("Nothing to update.".into());
        }
    }

    /// Works through Update Project: opens the next analysis and solves it, then the next
    /// once the one before is done.
    fn run_update(&mut self, ctx: &egui::Context) {
        if let Some((id, opened)) = self.updating {
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
            } else if opened {
                // Windows the update opened close again, as Workbench solves in the
                // background; the window stays open to show what went wrong.
                self.close_window(id);
            }
            if !failed && self.update_queue.is_empty() {
                self.output.push("Project updated".into());
            }
            // The analyses after it take its new results before they are solved.
            self.sync_due = true;
            return;
        }
        if self.sync_due || self.restoring.values().any(Option::is_some) {
            return;
        }
        let Some(id) = self.update_queue.pop_front() else {
            return;
        };
        let opened = self.node(id).is_some_and(|n| n.window.is_none());
        self.open_window(ctx, id);
        self.updating = Some((id, opened));
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
            let update = icons::button(ui, Icon::Solve, UPDATE_TIP, !updating, false).clicked();
            let text = egui::Button::new("Update Project").frame(false);
            let text = ui.add_enabled(!updating, text).on_hover_text(UPDATE_TIP);
            if update || text.clicked() {
                self.update_project();
            }
            ui.add_space(4.0);
            if icons::button(ui, Icon::Stop, "Stop Update Project", updating, false).clicked() {
                self.update_queue.clear();
                self.updating = None;
                self.output.push("Update stopped".into());
            }
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
        let mut incoming: HashMap<u64, Vec<(Vec<String>, String)>> = HashMap::new();
        for link in &self.links {
            if let Some(source) = self.nodes.iter().find(|n| n.id == link.source) {
                (incoming.entry(link.target).or_default())
                    .push((links::slot(link).to_vec(), source.name.clone()));
            }
        }
        let blocks: Vec<BlockView> = (self.nodes.iter())
            .map(|n| {
                let fe = n.link_data().map(|d| d.fe);
                let rows = match &n.status {
                    Some(status) => block_rows(
                        status,
                        fe,
                        &n.expanded,
                        incoming.get(&n.id).map_or(&[][..], Vec::as_slice),
                    ),
                    None => Vec::new(),
                };
                BlockView {
                    id: n.id,
                    name: n.name.clone(),
                    rows,
                    loading: n.window.as_ref().is_some_and(|w| w.model().is_none()),
                    open: n.window.is_some(),
                }
            })
            .collect();
        self.sync_wires(&blocks);
        let mut viewer = Viewer {
            nodes: blocks,
            actions: Vec::new(),
        };
        SnarlWidget::new()
            .id_salt(("project schematic", self.graph_generation))
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

    /// Lets the wires of the schematic show the project's links, each between the rows its
    /// ends are shown in.
    fn sync_wires(&mut self, blocks: &[BlockView]) {
        let block = |id: u64| blocks.iter().find(|b| b.id == id);
        let mut wanted = std::collections::HashSet::new();
        for link in &self.links {
            let (Some(source), Some(target)) = (block(link.source), block(link.target)) else {
                continue;
            };
            let output = shown_row(&source.rows, &link.item);
            let input = shown_row(&target.rows, links::slot(link));
            let nodes = (self.graph_node(link.source), self.graph_node(link.target));
            if let (Some(output), Some(input), (Some(from), Some(to))) = (output, input, nodes) {
                wanted.insert((
                    egui_snarl::OutPinId { node: from, output },
                    egui_snarl::InPinId { node: to, input },
                ));
            }
        }
        let wires: Vec<_> = self.graph.wires().collect();
        for wire in wires {
            if !wanted.remove(&wire) {
                self.graph.disconnect(wire.0, wire.1);
            }
        }
        for (from, to) in wanted {
            self.graph.connect(from, to);
        }
    }

    fn name_of(&self, id: u64) -> String {
        (self.nodes.iter().find(|n| n.id == id)).map_or_else(String::new, |n| n.name.clone())
    }

    /// Links `item` of `source` to `slot` of `target`, in place of the links that fill the
    /// same part of the target.
    fn link(&mut self, source: u64, item: Vec<String>, target: u64, slot: Vec<String>) {
        let (from, to) = (self.name_of(source), self.name_of(target));
        if source == target {
            return;
        }
        if !links::compatible(&item, &slot) {
            self.output.push(format!(
                "Not linked: {} of {from} cannot go to {} of {to}.",
                links::label(&item),
                links::label(&slot)
            ));
            return;
        }
        if links::makes_cycle(&self.links, source, target) {
            self.output.push(format!(
                "Not linked: {to} comes before {from} already, and a link cannot go back."
            ));
            return;
        }
        let results = item.first().is_some_and(|c| c == links::RESULTS);
        // A category or item links into the same place of the target; a step's results go
        // to the step they are dropped on.
        let slot = if results { slot } else { item.clone() };
        let overlaps = |link: &Link| {
            if link.target != target || links::is_results(link) != results {
                return false;
            }
            if results {
                // A submodel reads one global results file.
                return link.source != source || links::slot(link) == slot.as_slice();
            }
            let other = links::slot(link);
            let mesh_and_geometry = |a: &[String], b: &[String]| {
                a.first().is_some_and(|c| c == links::MESH)
                    && b.first().is_some_and(|c| c == links::GEOMETRY)
            };
            links::covers(other, &slot)
                || links::covers(&slot, other)
                || mesh_and_geometry(other, &slot)
                || mesh_and_geometry(&slot, other)
        };
        let replaced: Vec<Link> = self.links.iter().filter(|l| overlaps(l)).cloned().collect();
        self.links.retain(|l| !overlaps(l));
        for link in replaced {
            self.output.push(format!(
                "{to} no longer takes {} from {}.",
                links::label(&link.item),
                self.name_of(link.source)
            ));
        }
        let text = if results {
            format!(
                "{to} takes the displacements of {} of {from} in {} (submodel).",
                links::label(&item),
                slot.get(1).map_or("", String::as_str)
            )
        } else {
            format!("{to} takes {} from {from}.", links::label(&item))
        };
        self.output.push(text);
        self.applied.retain(|(id, _), _| *id != target);
        self.links.push(Link {
            source,
            target,
            slot: if results { slot } else { Vec::new() },
            item,
        });
        self.sync_due = true;
    }

    /// Removes the links that `remove` picks; the targets keep their copies. A target that
    /// loses its last results link is no submodel any more.
    fn unlink(&mut self, remove: impl Fn(&Link) -> bool) {
        let removed: Vec<Link> = self.links.iter().filter(|l| remove(l)).cloned().collect();
        self.links.retain(|l| !remove(l));
        for link in removed {
            let target = link.target;
            self.output.push(format!(
                "{} no longer takes {} from {}; it keeps its copy.",
                self.name_of(target),
                links::label(&link.item),
                self.name_of(link.source)
            ));
            self.applied
                .retain(|(id, path), _| *id != target || path != &link.item);
            let submodel = (self.links.iter()).any(|l| l.target == target && links::is_results(l));
            if links::is_results(&link)
                && !submodel
                && let Some(node) = self.node(target)
            {
                match &mut node.window {
                    Some(window) => window.clear_submodel(),
                    None => {
                        if let Some(data) = &mut node.data {
                            links::clear_submodel(&mut data.model);
                        }
                        node.refresh_closed();
                    }
                }
            }
        }
        self.sync_due = true;
    }

    /// Brings what the analyses take through links up to date with their sources: often
    /// enough to look live, in the order of the links so that a chain is done in one go.
    fn sync_links(&mut self, ctx: &egui::Context) {
        self.poll_restoring();
        let due = self.sync_due
            || (self.last_sync).is_none_or(|t| t.elapsed() > Duration::from_millis(300));
        if !due {
            return;
        }
        self.sync_due = false;
        self.last_sync = Some(Instant::now());
        let ids: Vec<u64> = self.nodes.iter().map(|n| n.id).collect();
        for target in links::order(&ids, &self.links) {
            let to_target: Vec<Link> = (self.links.iter())
                .filter(|l| l.target == target)
                .cloned()
                .collect();
            let mut incoming = links::Incoming::default();
            let mut missing = Vec::new();
            for link in &to_target {
                let source = self.name_of(link.source);
                incoming
                    .paths
                    .push((links::slot(link).to_vec(), source.clone()));
                if let Some(payload) = self.payload(link, &mut missing) {
                    self.apply(target, &source, link, payload);
                }
            }
            if let Some(node) = self.node(target) {
                match &mut node.window {
                    Some(window) => window.set_links(incoming, missing),
                    None => node.refresh_closed(),
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(300));
    }

    /// What `link` has to copy into its target to bring it up to date with the source;
    /// `None` when it is up to date or cannot be brought up to date now. Sources of results
    /// links that have no results go to `missing`.
    fn payload(&mut self, link: &Link, missing: &mut Vec<String>) -> Option<Payload> {
        let source = self.nodes.iter().find(|n| n.id == link.source)?;
        let target = self.nodes.iter().find(|n| n.id == link.target)?;
        if let Some(window) = &target.window
            && (window.busy().is_some() || window.model().is_none())
        {
            return None;
        }
        let data = source.link_data();
        let path = &link.item;
        match path.first().map(String::as_str) {
            Some(links::RESULTS) => {
                let step = path.get(1)?;
                let Some(bound) = source.bound_results() else {
                    missing.push(source.name.clone());
                    return None;
                };
                let global = links::global_step_number(data?.fe, step)?;
                let frd = bound.files.iter().find(|f| {
                    Path::new(&f.name)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("frd"))
                })?;
                let dir = (self.settings.solver.work_dir())
                    .join(crate::analysis::job_name(&self.project_name()))
                    .join(crate::analysis::job_name(&target.name))
                    .join(format!(
                        "Global-{}-{:016x}",
                        crate::analysis::job_name(&source.name),
                        bound.input.unwrap_or_default()
                    ));
                let file = dir.join(&frd.name);
                if !file.is_file() {
                    match self.restoring.get(&dir) {
                        Some(Some(_)) => missing.push(source.name.clone()),
                        Some(None) => {}
                        None => {
                            let (sender, receiver) = std::sync::mpsc::channel();
                            let (bound, folder) = (bound.clone(), dir.clone());
                            std::thread::spawn(move || {
                                let written = plx_io::project::restore_results(&bound, &folder)
                                    .map_err(|e| e.to_string());
                                let _ = sender.send(written);
                            });
                            self.restoring.insert(dir, Some(receiver));
                            missing.push(source.name.clone());
                        }
                    }
                }
                let step = links::slot(link).get(1)?.clone();
                Some(Payload::Results(step, global, file))
            }
            Some(links::GEOMETRY | links::MESH) => {
                let data = data?;
                if !links::exists(data, path) {
                    return None;
                }
                let print = links::fingerprint(data, path);
                let key = (link.target, path.clone());
                if self.applied.get(&key) == Some(&print) {
                    return None;
                }
                self.applied.insert(key, print);
                Some(if path[0] == links::MESH {
                    Payload::Mesh(Box::new(links::MeshCopy::of(data)))
                } else {
                    Payload::Geometry(data.geometry.cloned())
                })
            }
            Some(links::SETUP) => {
                let (data, own) = (data?, target.link_data()?);
                if !links::exists(data, path)
                    || links::fingerprint(data, path) == links::fingerprint(own, path)
                {
                    return None;
                }
                Some(Payload::Setup(Box::new(data.fe.clone())))
            }
            _ => None,
        }
    }

    /// Copies what a link takes into its target, see [`ProjectApp::payload`].
    fn apply(&mut self, target: u64, source: &str, link: &Link, payload: Payload) {
        let Some(node) = self.node(target) else {
            return;
        };
        let path = &link.item;
        if let Some(window) = &mut node.window {
            match payload {
                Payload::Setup(fe) => {
                    window.apply_setup(&fe, path);
                    window.message(format!("{} taken from {source}", links::label(path)));
                }
                Payload::Geometry(geometry) => {
                    window.apply_geometry(geometry);
                    window.message(format!("Geometry taken from {source}"));
                }
                Payload::Mesh(mesh) => {
                    window.apply_mesh(*mesh);
                    window.message(format!("Mesh taken from {source}"));
                }
                Payload::Results(step, global, file) => window.apply_submodel(&step, global, file),
            }
            return;
        }
        let Some(data) = &mut node.data else {
            return;
        };
        match payload {
            Payload::Setup(fe) => links::apply_setup(&mut data.model, &fe, path),
            Payload::Geometry(geometry) => data.geometry = geometry,
            Payload::Mesh(mesh) => {
                let links::MeshCopy {
                    geometry,
                    mesh,
                    mesh_inputs,
                    mesh_sets,
                    element_types,
                } = *mesh;
                data.geometry = geometry;
                data.mesh = mesh;
                data.mesh_inputs = mesh_inputs;
                data.model.mesh_sets = mesh_sets;
                data.model.element_types = element_types;
            }
            Payload::Results(step, global, file) => {
                links::apply_submodel(&mut data.model, &step, global, file);
            }
        }
    }

    /// Picks up global results written for submodels.
    fn poll_restoring(&mut self) {
        let mut done = Vec::new();
        for (dir, receiver) in &self.restoring {
            if let Some(receiver) = receiver
                && let Ok(result) = receiver.try_recv()
            {
                done.push((dir.clone(), result));
            }
        }
        for (dir, result) in done {
            match result {
                Ok(_) => {
                    self.restoring.remove(&dir);
                }
                Err(error) => {
                    self.output
                        .push(format!("Global results not written: {error}"));
                    // Not tried again until the project is opened again.
                    self.restoring.insert(dir, None);
                }
            }
            self.sync_due = true;
        }
    }

    /// Solves the analyses whose results `id` needs and that have none yet, in the order of
    /// the links, then `id`.
    fn solve_chain(&mut self, id: u64) {
        let mut sources = Vec::new();
        let mut queue = vec![id];
        while let Some(target) = queue.pop() {
            for link in self.links.iter().filter(|l| l.target == target) {
                if links::is_results(link) && !sources.contains(&link.source) {
                    sources.push(link.source);
                    queue.push(link.source);
                }
            }
        }
        sources.retain(|s| (self.nodes.iter()).any(|n| n.id == *s && n.bound_results().is_none()));
        sources.push(id);
        let chain = links::order(&sources, &self.links);
        self.update_queue.retain(|queued| !chain.contains(queued));
        for id in chain.into_iter().rev() {
            self.update_queue.push_front(id);
        }
    }

    fn act(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::New(pos) => {
                self.add_analysis("Analysis-1", pos);
            }
            Action::Open(id) => self.open_window(ctx, id),
            Action::Solve(id) => self.solve_chain(id),
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
            Action::Toggle(id, path) => {
                if let Some(node) = self.node(id)
                    && !node.expanded.remove(&path)
                {
                    node.expanded.insert(path);
                }
            }
            Action::Link {
                source,
                item,
                target,
                slot,
            } => self.link(source, item, target, slot),
            Action::Unlink { source, target } => self.unlink(|link| {
                source
                    .as_ref()
                    .is_none_or(|(id, path)| link.source == *id && links::covers(path, &link.item))
                    && target.as_ref().is_none_or(|(id, path)| {
                        link.target == *id && links::covers(path, links::slot(link))
                    })
            }),
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
            if let Some(status) = window.status() {
                node.status = Some(status);
            }
        }
        if let Some(settings) = settings {
            self.settings = settings;
            self.share_settings();
        }
        for id in closing {
            self.close_window(id);
        }
        for (id, request) in requests {
            match request {
                Request::Save(save_as) => self.save(save_as),
                Request::SaveWithoutResults => self.save_without_results(),
                Request::OpenProject(path) => self.open(ctx, path),
                Request::SpaceAndUnits(space, units, convert) => {
                    self.set_space_and_units(space, units, convert)
                }
                Request::SolveChain => self.solve_chain(id),
                Request::OpenAnalysis(name) => {
                    if let Some(source) = self.nodes.iter().find(|n| n.name == name) {
                        let source = source.id;
                        self.open_window(ctx, source);
                    }
                }
                Request::Unlink(path) => {
                    self.unlink(|l| l.target == id && links::covers(&path, links::slot(l)))
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
        self.sync_links(&ctx);
        self.run_update(&ctx);
        self.analysis_windows(&ctx);
    }
}

/// A row of a block: a cell of the analysis, a category or item of its setup, or a step
/// whose results it has. Each row has a pin on either side to link it.
#[derive(Clone, Debug)]
struct Row {
    path: Vec<String>,
    text: String,
    depth: usize,
    icon: TreeIcon,
    state: Option<CellState>,
    /// Whether the row opens to show what is in it, and whether it is open.
    expander: Option<bool>,
    /// Whether the row takes a link (left pin) and gives one (right pin).
    input: bool,
    output: bool,
    /// The analysis a link fills the row from.
    linked_from: Option<String>,
    tip: String,
}

/// How a block of the schematic looks: what the viewer needs of an analysis.
struct BlockView {
    id: u64,
    name: String,
    rows: Vec<Row>,
    loading: bool,
    open: bool,
}

/// The rows of an analysis' block: its cells, the setup opened as far as `expanded` says, and
/// the steps whose results can drive a submodel.
fn block_rows(
    status: &AnalysisStatus,
    fe: Option<&plx_model::FeModel>,
    expanded: &BTreeSet<Vec<String>>,
    incoming: &[(Vec<String>, String)],
) -> Vec<Row> {
    let from = |path: &[String]| {
        (incoming.iter())
            .find(|(slot, _)| links::covers(slot, path))
            .map(|(_, source)| source.clone())
    };
    let cell = |name: &str, icon, state: CellState, linkable: bool| {
        let path = vec![name.to_owned()];
        Row {
            text: name.to_owned(),
            depth: 0,
            icon,
            state: Some(state),
            expander: None,
            input: linkable,
            output: linkable,
            linked_from: from(&path),
            tip: format!("{name}: {}", state.text()),
            path,
        }
    };
    let mut rows = vec![
        cell(links::GEOMETRY, TreeIcon::Geometry, status.geometry, true),
        cell(links::MESH, TreeIcon::Mesh, status.mesh, true),
    ];
    if !status.stale_parts.is_empty() {
        rows[1]
            .tip
            .push_str(&format!(" ({})", status.stale_parts.join(", ")));
    }
    let setup = vec![links::SETUP.to_owned()];
    let setup_open = expanded.contains(&setup);
    let mut row = cell(links::SETUP, TreeIcon::Steps, status.setup, true);
    row.expander = Some(setup_open);
    rows.push(row);
    if setup_open {
        for category in links::CATEGORIES {
            let path = vec![links::SETUP.to_owned(), category.to_owned()];
            let open = expanded.contains(&path);
            let entries = fe
                .map(|fe| links::entries(fe, category))
                .unwrap_or_default();
            rows.push(Row {
                text: if entries.is_empty() {
                    category.to_owned()
                } else {
                    format!("{category} ({})", entries.len())
                },
                depth: 1,
                icon: category_icon(category),
                state: None,
                expander: (!entries.is_empty()).then_some(open),
                input: true,
                output: true,
                linked_from: from(&path),
                tip: format!("{category}: drag to another analysis to link them all"),
                path: path.clone(),
            });
            if open {
                for name in entries {
                    let mut path = path.clone();
                    path.push(name.clone());
                    rows.push(Row {
                        tip: format!("{name}: drag to another analysis to link it"),
                        text: name,
                        depth: 2,
                        icon: category_icon(category),
                        state: None,
                        expander: None,
                        input: true,
                        output: true,
                        linked_from: from(&path),
                        path,
                    });
                }
            }
        }
    }
    rows.push(cell("Solution", TreeIcon::Analysis, status.solution, false));
    rows.push(cell(
        "Results",
        TreeIcon::FieldOutput,
        status.results,
        false,
    ));
    for step in fe.map(|fe| fe.steps.as_slice()).unwrap_or_default() {
        let path = vec![links::RESULTS.to_owned(), step.name.clone()];
        let tip = if step.active {
            format!(
                "Results of {}: drag to a step of a submodel, which takes its displacements \
                 from them",
                step.name
            )
        } else {
            format!("{} is deactivated and has no results", step.name)
        };
        rows.push(Row {
            text: step.name.clone(),
            depth: 1,
            icon: TreeIcon::Step,
            state: None,
            expander: None,
            input: true,
            output: step.active,
            linked_from: from(&path),
            tip,
            path,
        });
    }
    rows
}

/// The row a path is shown in: the row itself, or the closed row it lies in.
fn shown_row(rows: &[Row], path: &[String]) -> Option<usize> {
    (rows.iter().enumerate())
        .filter(|(_, row)| links::covers(&row.path, path))
        .max_by_key(|(_, row)| row.path.len())
        .map(|(index, _)| index)
}

/// The symbol of each cell and category of a block, as in the analysis tree.
fn category_icon(category: &str) -> TreeIcon {
    match category {
        "Features" => TreeIcon::Features,
        "Materials" => TreeIcon::Material,
        "Sections" => TreeIcon::Section,
        "Constraints" => TreeIcon::Constraints,
        "Contacts" => TreeIcon::Contacts,
        "Amplitudes" => TreeIcon::Amplitude,
        "Initial Conditions" => TreeIcon::InitialConditions,
        _ => TreeIcon::Step,
    }
}

/// Colour of the wires of a link: data in blue, results in orange.
fn wire_color(path: &[String]) -> egui::Color32 {
    if path.first().is_some_and(|c| c == links::RESULTS) {
        crate::style::adapt(egui::Color32::from_rgb(230, 130, 20))
    } else {
        crate::style::adapt(egui::Color32::from_rgb(40, 110, 200))
    }
}

struct Viewer {
    nodes: Vec<BlockView>,
    actions: Vec<Action>,
}

impl Viewer {
    fn block(&self, id: u64) -> Option<&BlockView> {
        self.nodes.iter().find(|n| n.id == id)
    }

    fn row(&self, snarl: &Snarl<u64>, node: NodeId, index: usize) -> Option<(u64, &Row)> {
        let id = *snarl.get_node(node)?;
        Some((id, self.block(id)?.rows.get(index)?))
    }
}

/// Width of the rows of a block.
const ROW_WIDTH: f32 = 210.0;
const ROW_HEIGHT: f32 = 20.0;

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
        let label = ui.add(
            egui::Label::new(text)
                .selectable(false)
                .sense(egui::Sense::click()),
        );
        if block.open {
            ui.weak("(open)");
        }
        if block.loading {
            ui.spinner();
        }
        if label.double_clicked() {
            self.actions.push(Action::Open(id));
        }
        // The name takes the clicks of the header, so it opens the block's menu too.
        label
            .on_hover_text("Double-click opens the analysis in its own window.")
            .context_menu(|ui| node_menu(ui, id, &mut self.actions));
    }

    fn inputs(&mut self, node: &u64) -> usize {
        self.block(*node).map_or(0, |b| b.rows.len())
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u64>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let Some((id, row)) = self.row(snarl, pin.id.node, pin.id.input) else {
            return hidden_pin();
        };
        let row = row.clone();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ROW_WIDTH, ROW_HEIGHT), egui::Sense::click());
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, 2.0, crate::style::hover_fill());
        }
        let mut x = rect.left() + 2.0 + 14.0 * row.depth as f32;
        let middle = rect.center().y;
        let mut toggled = false;
        if let Some(open) = row.expander {
            let arrow =
                egui::Rect::from_center_size(egui::pos2(x + 5.0, middle), egui::vec2(10.0, 10.0));
            let color = ui.visuals().text_color();
            let points = if open {
                vec![
                    arrow.left_top() + egui::vec2(1.0, 2.0),
                    arrow.right_top() + egui::vec2(-1.0, 2.0),
                    arrow.center_bottom() - egui::vec2(0.0, 1.0),
                ]
            } else {
                vec![
                    arrow.left_top() + egui::vec2(2.0, 1.0),
                    arrow.right_center() - egui::vec2(1.0, 0.0),
                    arrow.left_bottom() + egui::vec2(2.0, -1.0),
                ]
            };
            painter.add(egui::Shape::convex_polygon(
                points,
                color,
                egui::Stroke::NONE,
            ));
            let pointer = response.interact_pointer_pos();
            toggled = response.clicked() && pointer.is_some_and(|p| p.x < arrow.right() + 2.0);
        }
        x += 12.0;
        tree_icons::paint(painter, egui::pos2(x, middle - 8.0), row.icon);
        x += 20.0;
        let color = if row.linked_from.is_some() {
            crate::style::adapt(egui::Color32::from_rgb(40, 110, 200))
        } else {
            ui.visuals().text_color()
        };
        painter.text(
            egui::pos2(x, middle),
            egui::Align2::LEFT_CENTER,
            &row.text,
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
        if let Some(state) = row.state {
            let symbol = egui::pos2(rect.right() - 18.0, middle - 8.0);
            status::paint(painter, symbol, state);
        }
        let mut tip = row.tip.clone();
        if let Some(source) = &row.linked_from {
            tip.push_str(&format!("\nLinked from {source}."));
        }
        if row.input || row.output {
            tip.push_str("\nRight-click a pin to remove its links.");
        }
        let response = response.on_hover_text(tip);
        if toggled || (row.expander.is_some() && response.double_clicked()) {
            self.actions.push(Action::Toggle(id, row.path.clone()));
        } else if response.double_clicked() {
            self.actions.push(Action::Open(id));
        }
        if row.input {
            PinInfo::circle().with_fill(wire_color(&row.path))
        } else {
            hidden_pin()
        }
    }

    fn outputs(&mut self, node: &u64) -> usize {
        self.block(*node).map_or(0, |b| b.rows.len())
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        snarl: &mut Snarl<u64>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        ui.allocate_exact_size(egui::vec2(0.0, ROW_HEIGHT), egui::Sense::hover());
        match self.row(snarl, pin.id.node, pin.id.output) {
            Some((_, row)) if row.output => {
                let color = wire_color(&row.path);
                PinInfo::circle().with_fill(color).with_wire_color(color)
            }
            _ => hidden_pin(),
        }
    }

    fn has_body(&mut self, _node: &u64) -> bool {
        false
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<u64>) {
        let source = self.row(snarl, from.id.node, from.id.output);
        let target = self.row(snarl, to.id.node, to.id.input);
        if let (Some((source, item)), Some((target, slot))) = (source, target)
            && item.output
            && slot.input
        {
            self.actions.push(Action::Link {
                source,
                item: item.path.clone(),
                target,
                slot: slot.path.clone(),
            });
        }
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<u64>) {
        let source = self.row(snarl, from.id.node, from.id.output);
        let target = self.row(snarl, to.id.node, to.id.input);
        if let (Some((source, item)), Some((target, slot))) = (source, target) {
            self.actions.push(Action::Unlink {
                source: Some((source, item.path.clone())),
                target: Some((target, slot.path.clone())),
            });
        }
    }

    fn drop_outputs(&mut self, pin: &OutPin, snarl: &mut Snarl<u64>) {
        if let Some((source, item)) = self.row(snarl, pin.id.node, pin.id.output) {
            self.actions.push(Action::Unlink {
                source: Some((source, item.path.clone())),
                target: None,
            });
        }
    }

    fn drop_inputs(&mut self, pin: &InPin, snarl: &mut Snarl<u64>) {
        if let Some((target, slot)) = self.row(snarl, pin.id.node, pin.id.input) {
            self.actions.push(Action::Unlink {
                source: None,
                target: Some((target, slot.path.clone())),
            });
        }
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
        node_menu(ui, id, &mut self.actions);
    }
}

/// The pin of a row that takes or gives no link: there, but not to be seen.
fn hidden_pin() -> PinInfo {
    PinInfo::circle()
        .with_fill(egui::Color32::TRANSPARENT)
        .with_stroke(egui::Stroke::NONE)
}

/// The context menu of an analysis' block.
fn node_menu(ui: &mut egui::Ui, id: u64, actions: &mut Vec<Action>) {
    let mut entry = |ui: &mut egui::Ui, text: &str, action: Action| {
        if ui.button(text).clicked() {
            actions.push(action);
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

fn schematic_style() -> SnarlStyle {
    SnarlStyle {
        collapsible: Some(false),
        // Blocks are never magnified beyond their size, also when the view centres on them.
        max_scale: Some(1.0),
        min_scale: Some(0.3),
        // A row per pin, all of the same height, so that the rows line up with their pins.
        node_layout: Some(egui_snarl::ui::NodeLayout {
            kind: egui_snarl::ui::NodeLayoutKind::Coil,
            min_pin_row_height: ROW_HEIGHT,
            equal_pin_row_heights: true,
        }),
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
