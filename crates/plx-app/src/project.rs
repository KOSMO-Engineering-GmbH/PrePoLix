//! The project window, Ansys Workbench's project schematic: the analyses of the project as
//! blocks of a node graph, each with the state of its cells. An analysis opens in a window of
//! its own, the [`AnalysisWindow`], which looks like PrePoMax's main window.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crate::node_picker::{self, NodePicker};
use egui_snarl::ui::{AnyPins, PinInfo, SnarlStyle, SnarlViewer, SnarlWidget};
use egui_snarl::{InPin, NodeId, OutPin, Snarl};
use plx_io::project::{Link, MaterialSet, MeshBlock, Project, ProjectAnalysis, ProjectNode};
use plx_model::{MeshPrimitive, ModelProperties, ModelSpace};

use crate::app::{AnalysisWindow, Request};
use crate::icons::{self, Icon};
use crate::links;
use crate::material_set::{MaterialSetEditor, MaterialSetResult};
use crate::mesh_block::{MeshBlockEditor, MeshBlockResult};
use crate::model::PROJECT_EXTENSION;
use crate::model_properties::{DialogResult, ModelPropertiesDialog};
use crate::node_icons::{self, NodeIcon};
use crate::settings::{self, Settings, SettingsWindow, WindowResult};
use crate::status::{self, AnalysisStatus, CellState};
use crate::tree::TreeView;

/// Storage key of the project file open at exit, reopened at the next start.
const LAST_PROJECT_KEY: &str = "last_project";

/// What a block of the schematic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Analysis,
    /// A collection of materials, held in the setup of its data: a source of data for
    /// analyses, which has no steps and is not solved.
    Materials,
    /// A structured mesh made from a few parameters, held in the mesh of its data.
    Mesh,
}

impl Kind {
    /// Whether the block is a source of data rather than an analysis.
    fn is_source(self) -> bool {
        self != Self::Analysis
    }

    /// What the block is called in the output and in dialogs.
    fn noun(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Materials => "materials block",
            Self::Mesh => "mesh block",
        }
    }
}

/// A block of the schematic: an analysis of the project, or a source of data analyses link
/// to, which holds what it gives in the same form as an analysis.
struct Node {
    id: u64,
    name: String,
    kind: Kind,
    /// The analysis' data while its window is closed; an open window holds it. A source
    /// always holds its data.
    data: Option<ProjectAnalysis>,
    window: Option<Box<AnalysisWindow>>,
    /// Where the window opens the first time: over the project window.
    window_position: Option<egui::Pos2>,
    /// The state of its cells, as last found.
    status: Option<AnalysisStatus>,
    /// Rows of its block that are open: the setup and its categories.
    expanded: BTreeSet<Vec<String>>,
    /// Its model space, shown in the header of its block.
    space: ModelSpace,
    /// What a mesh block meshes, and the fingerprint of what its mesh was made from.
    primitive: Option<MeshPrimitive>,
    generated: Option<u64>,
}

impl Node {
    fn new(id: u64, name: String, data: ProjectAnalysis) -> Self {
        Self::of_kind(Kind::Analysis, id, name, data)
    }

    fn of_kind(kind: Kind, id: u64, name: String, data: ProjectAnalysis) -> Self {
        let space = data.model.properties.space;
        // A materials block shows its materials at first.
        let expanded = match kind {
            Kind::Materials => BTreeSet::from([material_set_path()]),
            _ => BTreeSet::new(),
        };
        let mut node = Self {
            id,
            name,
            kind,
            data: Some(data),
            window: None,
            window_position: None,
            status: None,
            expanded,
            space,
            primitive: None,
            generated: None,
        };
        node.refresh_closed();
        node
    }

    /// A mesh block meshing `primitive`, with the mesh in `data` made from `generated`.
    fn mesh_block(
        id: u64,
        name: String,
        data: ProjectAnalysis,
        primitive: MeshPrimitive,
        generated: Option<u64>,
    ) -> Self {
        let mut node = Self::of_kind(Kind::Mesh, id, name, data);
        node.primitive = Some(primitive);
        node.generated = generated;
        node
    }

    /// The state of a mesh block's mesh: out of date once its parameters changed.
    fn mesh_state(&self) -> CellState {
        let made = self
            .data
            .as_ref()
            .is_some_and(|d| d.mesh.element_count() > 0);
        match &self.primitive {
            _ if !made => CellState::Missing,
            Some(primitive) if Some(primitive.fingerprint(self.space)) == self.generated => {
                CellState::UpToDate
            }
            _ => CellState::OutOfDate,
        }
    }

    /// Makes a mesh block's mesh from its parameters; returns the number of elements, or
    /// why it cannot be made.
    fn generate(&mut self) -> Result<usize, String> {
        let primitive = self.primitive.as_ref().ok_or("not a mesh block")?;
        if let Some(problem) = primitive.problem(self.space) {
            return Err(problem);
        }
        let mesh = primitive.generate(self.space, &primitive.label().to_uppercase());
        let count = mesh.element_count();
        self.generated = Some(primitive.fingerprint(self.space));
        let data = self.data.as_mut().ok_or("no data")?;
        data.mesh = mesh;
        data.model.properties.space = self.space;
        Ok(count)
    }

    /// A block read from the project file.
    fn read(node: ProjectNode) -> Self {
        match node {
            ProjectNode::Analysis(analysis) => {
                Self::new(analysis.id, analysis.name.clone(), *analysis)
            }
            ProjectNode::Materials(set) => {
                let mut data = ProjectAnalysis {
                    id: set.id,
                    name: set.name.clone(),
                    position: set.position,
                    ..Default::default()
                };
                data.model.materials = set.materials;
                data.model.properties.units = set.units;
                Self::of_kind(Kind::Materials, set.id, set.name, data)
            }
            ProjectNode::Mesh(block) => {
                let block = *block;
                let mut data = ProjectAnalysis {
                    id: block.id,
                    name: block.name.clone(),
                    position: block.position,
                    mesh: block.mesh,
                    ..Default::default()
                };
                data.model.properties.space = block.space;
                data.model.properties.units = block.units;
                Self::mesh_block(block.id, block.name, data, block.primitive, block.generated)
            }
        }
    }

    /// The block as the project file keeps it, from the window of an analysis if it is
    /// open; `position` is that of its block.
    fn written(&mut self, position: [f32; 2]) -> Option<ProjectNode> {
        let mut data = self.data()?;
        data.id = self.id;
        data.name = self.name.clone();
        data.position = position;
        Some(match self.kind {
            Kind::Analysis => ProjectNode::Analysis(Box::new(data)),
            Kind::Materials => ProjectNode::Materials(MaterialSet {
                id: data.id,
                name: data.name,
                position,
                units: data.model.properties.units,
                materials: data.model.materials,
            }),
            Kind::Mesh => ProjectNode::Mesh(Box::new(MeshBlock {
                id: data.id,
                name: data.name,
                position,
                space: self.space,
                units: data.model.properties.units,
                primitive: self.primitive.clone()?,
                mesh: data.mesh,
                generated: self.generated,
            })),
        })
    }

    /// Finds the state of a closed analysis from its data, once after it changed.
    fn refresh_closed(&mut self) {
        if self.kind.is_source() {
            return;
        }
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
    /// What a link of a category takes: the source's model, the names of the items it takes
    /// and those linked items of the target that come before them, see
    /// [`links::apply_bundle`].
    Bundle {
        fe: Box<plx_model::FeModel>,
        names: Vec<String>,
        ahead: Vec<String>,
    },
    Geometry(Option<plx_model::Geometry>),
    Mesh(Box<links::MeshCopy>),
    /// The step of the source's results and their file, see [`links::apply_results`].
    Results(u32, PathBuf),
    /// What a state link takes: the mesh with its geometry when it changed, the source's
    /// model when the setup or the steps up to `step` changed.
    Chain {
        mesh: Option<Box<links::MeshCopy>>,
        fe: Option<Box<plx_model::FeModel>>,
        step: usize,
    },
}

/// What the user did in the schematic, done after it is drawn.
enum Action {
    New(egui::Pos2),
    /// A new materials block at the position.
    AddMaterials(egui::Pos2),
    /// A new mesh block meshing the primitive at the position.
    AddMesh(egui::Pos2, MeshPrimitive),
    /// Make the mesh of a mesh block anew.
    Generate(u64),
    /// Open the analysis' window, in the workspace of a cell if given.
    Open(u64, Option<TreeView>),
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
    /// A new analysis that continues `source` after `step`, its block at the position.
    Continue {
        source: u64,
        step: String,
        position: egui::Pos2,
    },
    /// Split the analysis after the step: the steps after it go to a new analysis that
    /// continues it.
    Split(u64, String),
    /// Give the own steps of the analysis to the one it continues, and remove it.
    Merge(u64),
    /// Remove the links of a pin, or the one between two pins.
    Unlink {
        source: Option<(u64, Vec<String>)>,
        target: Option<(u64, Vec<String>)>,
    },
}

/// A project read from its file, with notes on its conversion, or why it could not be read.
type Opened = Result<(Project, Vec<String>), String>;

pub struct ProjectApp {
    render_state: egui_wgpu::RenderState,
    settings: Settings,
    settings_window: Option<SettingsWindow>,
    updater: crate::update::Updater,
    output: Vec<String>,
    /// The project file; `None` until the project is saved.
    path: Option<PathBuf>,
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
    /// The editor of a materials block.
    material_set: Option<MaterialSetEditor>,
    /// The editor of a mesh block.
    mesh_block: Option<MeshBlockEditor>,
    /// Update Project made meshes anew and goes on once links took them.
    update_after_sync: bool,
    /// The overview of blocks to add, open after a double-click into the schematic.
    picker: Option<NodePicker>,
    /// Show All Blocks was asked for: the view fits the blocks in the next frame.
    show_all: bool,
    /// The properties of a new analysis being asked for, with where its block goes.
    new_analysis: Option<(egui::Pos2, ModelPropertiesDialog)>,
    /// A project file being read on a worker thread, with notes on its conversion.
    opening: Option<(PathBuf, Receiver<Opened>)>,
    /// Analyses Update Project still has to solve, in order, and those it solves, with
    /// whether the update opened their window.
    update_queue: VecDeque<u64>,
    updating: Vec<(u64, bool)>,
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
    /// Problems of links already reported in the output, each once.
    warned: BTreeSet<String>,
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
            material_set: None,
            mesh_block: None,
            update_after_sync: false,
            picker: None,
            show_all: false,
            new_analysis: None,
            opening: None,
            update_queue: VecDeque::new(),
            updating: Vec::new(),
            startup_import: Vec::new(),
            applied: HashMap::new(),
            last_sync: None,
            sync_due: true,
            restoring: HashMap::new(),
            warned: BTreeSet::new(),
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
            let properties = app.settings.new_model.clone();
            let id = app.add_analysis(&name, egui::pos2(40.0, 40.0), properties);
            app.startup_import = files;
            app.open_window(&cc.egui_ctx, id, None);
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

    /// Adds an empty analysis with these model properties; returns its id.
    fn add_analysis(
        &mut self,
        name: &str,
        position: egui::Pos2,
        properties: ModelProperties,
    ) -> u64 {
        let id = self.next_id();
        let name = self.unique_name(name);
        let mut data = ProjectAnalysis {
            id,
            name: name.clone(),
            ..Default::default()
        };
        data.model.properties = properties;
        self.nodes.push(Node::new(id, name, data));
        self.graph.insert_node(position, id);
        id
    }

    /// Adds an empty materials block; returns its id.
    fn add_materials(&mut self, position: egui::Pos2) -> u64 {
        let id = self.next_id();
        let name = self.unique_name("Materials");
        let mut data = ProjectAnalysis {
            id,
            name: name.clone(),
            ..Default::default()
        };
        data.model.properties.units = self.settings.new_model.units;
        self.nodes
            .push(Node::of_kind(Kind::Materials, id, name.clone(), data));
        self.graph.insert_node(position, id);
        self.output.push(format!(
            "{name} created: double-click it to add materials, then drag them to analyses."
        ));
        id
    }

    /// Adds a mesh block meshing `primitive` and makes its mesh; returns its id.
    fn add_mesh(&mut self, position: egui::Pos2, primitive: MeshPrimitive) -> u64 {
        let id = self.next_id();
        let name = self.unique_name(primitive.label());
        let space = match &primitive {
            MeshPrimitive::Box { .. } => ModelSpace::ThreeD,
            MeshPrimitive::Rectangle { .. } => match self.settings.new_model.space {
                ModelSpace::ThreeD => ModelSpace::PlaneStress,
                space => space,
            },
        };
        let mut data = ProjectAnalysis {
            id,
            name: name.clone(),
            ..Default::default()
        };
        data.model.properties.space = space;
        data.model.properties.units = self.settings.new_model.units;
        let mut node = Node::mesh_block(id, name.clone(), data, primitive, None);
        let made = node.generate();
        self.nodes.push(node);
        self.graph.insert_node(position, id);
        self.output.push(match made {
            Ok(count) => format!(
                "{name} created: {}, {count} elements. Double-click it to change its size, then \
                 drag its mesh to analyses.",
                space.label()
            ),
            Err(why) => format!("{name} created without a mesh: {why}"),
        });
        id
    }

    /// Makes the meshes of the mesh blocks that are out of date anew; returns whether it
    /// made any.
    fn generate_meshes(&mut self) -> bool {
        let mut lines = Vec::new();
        for node in (self.nodes.iter_mut()).filter(|n| n.kind == Kind::Mesh) {
            if node.mesh_state() == CellState::UpToDate {
                continue;
            }
            lines.push(match node.generate() {
                Ok(count) => format!("{}: mesh made, {count} elements", node.name),
                Err(why) => format!("{}: no mesh made: {why}", node.name),
            });
        }
        if lines.is_empty() {
            return false;
        }
        self.output.extend(lines);
        self.sync_due = true;
        true
    }

    /// Makes the mesh of one mesh block anew.
    fn generate_mesh(&mut self, id: u64) {
        let Some(node) = self.node(id) else {
            return;
        };
        let line = match node.generate() {
            Ok(count) => format!("{}: mesh made, {count} elements", node.name),
            Err(why) => format!("{}: no mesh made: {why}", node.name),
        };
        self.output.push(line);
        self.sync_due = true;
    }

    fn edit_mesh(&mut self, id: u64) {
        let linked = self.links.iter().any(|l| l.source == id);
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return;
        };
        let (Some(primitive), Some(data)) = (&node.primitive, &node.data) else {
            return;
        };
        self.mesh_block = Some(MeshBlockEditor::new(
            id,
            &node.name,
            primitive.clone(),
            node.space,
            data.model.properties.units,
            linked,
        ));
    }

    fn mesh_block_window(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.mesh_block else {
            return;
        };
        let id = editor.id;
        match editor.show(ctx) {
            MeshBlockResult::Open => {}
            MeshBlockResult::Cancel => self.mesh_block = None,
            MeshBlockResult::Ok(primitive, space, units) => {
                self.mesh_block = None;
                if let Some(node) = self.node(id) {
                    node.primitive = Some(primitive);
                    node.space = space;
                    if let Some(data) = &mut node.data {
                        data.model.properties.units = units;
                    }
                    if node.mesh_state() == CellState::OutOfDate {
                        let line = format!(
                            "{}: the mesh is out of date; Update Project or Generate Mesh \
                             makes it anew.",
                            node.name
                        );
                        self.output.push(line);
                    }
                }
            }
        }
    }

    /// Opens the editor of a materials block, with the materials of the analyses to import.
    fn edit_materials(&mut self, id: u64) {
        let analyses = (self.nodes.iter())
            .filter(|n| n.kind == Kind::Analysis)
            .filter_map(|n| Some((n.name.clone(), n.link_data()?.fe.materials.clone())))
            .collect();
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            return;
        };
        let Some(data) = &node.data else {
            return;
        };
        self.material_set = Some(MaterialSetEditor::new(
            id,
            &node.name,
            &data.model.materials,
            data.model.properties.units,
            analyses,
        ));
    }

    fn material_set_window(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.material_set else {
            return;
        };
        let id = editor.id;
        match editor.show(ctx) {
            MaterialSetResult::Open => {}
            MaterialSetResult::Cancel => self.material_set = None,
            MaterialSetResult::Ok(materials, units) => {
                self.material_set = None;
                if let Some(data) = self.node(id).and_then(|n| n.data.as_mut()) {
                    data.model.materials = materials;
                    data.model.properties.units = units;
                }
                // The analyses that link them take them now.
                self.sync_due = true;
            }
        }
    }

    /// The analysis' block in the schematic.
    fn graph_node(&self, id: u64) -> Option<NodeId> {
        (self.graph.nodes_ids_data())
            .find(|(_, node)| node.value == id)
            .map(|(graph_id, _)| graph_id)
    }

    fn open_window(&mut self, ctx: &egui::Context, id: u64, view: Option<TreeView>) {
        match self.nodes.iter().find(|n| n.id == id).map(|n| n.kind) {
            Some(Kind::Analysis) => {}
            Some(Kind::Materials) => {
                // Asked for from an analysis window: the editor is in the project window.
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
                return self.edit_materials(id);
            }
            Some(Kind::Mesh) => {
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
                return self.edit_mesh(id);
            }
            None => return,
        }
        let path = self.model_path();
        let folder = crate::analysis::job_name(&self.project_name());
        let (render_state, settings) = (self.render_state.clone(), self.settings.clone());
        let at = ctx.input(|i| i.viewport().outer_rect).map(|r| r.min);
        let import = std::mem::take(&mut self.startup_import);
        // What it takes from others is known before it is solved.
        let incoming = self.incoming(id);
        self.sync_due = true;
        let Some(node) = self.node(id) else {
            return;
        };
        if node.window.is_some() {
            // Brought to the front.
            let viewport = analysis_viewport(id);
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
            if let (Some(view), Some(window)) = (view, &mut node.window) {
                window.show_workspace(view);
            }
            return;
        }
        let Some(data) = node.data.take() else {
            return;
        };
        let mut window = AnalysisWindow::new(render_state, settings, &node.name);
        window.set_project_folder(folder);
        window.set_links(incoming, Vec::new());
        if let Some(view) = view {
            window.show_workspace(view);
        }
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
        self.update_queue.clear();
        self.updating.clear();
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
            let project = plx_io::project::read_project(&file).map(|mut project| {
                // Format 2 kept the values in the project's unit system.
                let mut notes = Vec::new();
                if project.format < 3 {
                    let (space, units) = (project.space, project.units);
                    for analysis in project.analyses_mut() {
                        analysis.model.properties.space = space;
                        analysis.model.properties.units = units;
                        notes.extend(crate::model::migrate_to_si(analysis));
                    }
                    notes.push(
                        "The project was converted for this version; once saved, it no \
                         longer opens in older versions."
                            .into(),
                    );
                }
                // Format 3 differs only by what it lacks: state links; format 4 by what it
                // lacks too: blocks besides analyses.
                project.format = plx_model::PROJECT_FORMAT;
                (project, notes)
            });
            let _ = sender.send(project.map_err(|e| e.to_string()));
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
            Ok((project, notes)) => {
                if !self.close_all_windows() {
                    return;
                }
                self.output.extend(notes);
                self.nodes.clear();
                self.graph = Snarl::new();
                self.graph_generation += 1;
                self.links = project.links;
                self.applied.clear();
                self.restoring.clear();
                self.sync_due = true;
                for node in project.nodes {
                    let [x, y] = node.position();
                    self.graph.insert_node(egui::pos2(x, y), node.id());
                    self.nodes.push(Node::read(node));
                }
                let analyses = (self.nodes.iter())
                    .filter(|n| n.kind == Kind::Analysis)
                    .count();
                let sources = self.nodes.len() - analyses;
                self.output.push(if sources == 0 {
                    format!("{} opened: {analyses} analyses", path.display())
                } else {
                    let other = if sources == 1 { "block" } else { "blocks" };
                    format!(
                        "{} opened: {analyses} analyses, {sources} other {other}",
                        path.display()
                    )
                });
                self.path = Some(path);
                self.update_queue.clear();
                self.updating.clear();
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
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for node in &mut self.nodes {
            let position = (positions.iter())
                .find(|(id, _)| *id == node.id)
                .map_or([0.0; 2], |(_, pos)| [pos.x, pos.y]);
            let Some(mut written) = node.written(position) else {
                continue;
            };
            if let ProjectNode::Analysis(data) = &mut written
                && !results
            {
                data.results = None;
            }
            nodes.push(written);
        }
        Project {
            nodes,
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
        let (kind, primitive, generated) = (node.kind, node.primitive.clone(), node.generated);
        let new_id = self.next_id();
        let name = self.unique_name(&name);
        data.id = new_id;
        data.name = name.clone();
        self.nodes.push(match primitive {
            Some(primitive) => Node::mesh_block(new_id, name.clone(), data, primitive, generated),
            None => Node::of_kind(kind, new_id, name.clone(), data),
        });
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
        if self.material_set.as_ref().is_some_and(|e| e.id == id) {
            self.material_set = None;
        }
        if self.mesh_block.as_ref().is_some_and(|e| e.id == id) {
            self.mesh_block = None;
        }
        self.update_queue.retain(|&queued| queued != id);
    }

    /// Ansys Workbench's Update Project: solves every analysis that is not up to date, one
    /// after the other.
    fn update_project(&mut self) {
        // Mesh blocks first: what links their meshes takes the new one before it is found
        // out of date, so the update goes on once the links are brought up to date.
        if self.generate_meshes() {
            self.update_after_sync = true;
            return;
        }
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

    /// Works through Update Project: opens the next analyses and solves them, as many at a
    /// time as the settings allow; an analysis waits for those it takes data from.
    fn run_update(&mut self, ctx: &egui::Context) {
        for (id, opened) in std::mem::take(&mut self.updating) {
            let Some(node) = self.node(id) else {
                continue;
            };
            let Some(window) = &mut node.window else {
                continue;
            };
            if window.waiting() || window.solving() || window.is_running() {
                self.updating.push((id, opened));
                continue;
            }
            // Done: solved, or stopped by an error the window shows.
            let failed = window.current_status().is_some_and(|s| s.needs_solve());
            let name = node.name.clone();
            if failed {
                // What depends on it is not solved; independent analyses go on.
                let dependent = self.dependents(id);
                let skipped: Vec<String> = (self.nodes.iter())
                    .filter(|n| dependent.contains(&n.id) && self.update_queue.contains(&n.id))
                    .map(|n| n.name.clone())
                    .collect();
                self.update_queue
                    .retain(|queued| !dependent.contains(queued));
                self.output.push(if skipped.is_empty() {
                    format!("{name} could not be solved.")
                } else {
                    format!(
                        "{name} could not be solved; {} not updated.",
                        skipped.join(", ")
                    )
                });
            } else if opened {
                // Windows the update opened close again, as Workbench solves in the
                // background; the window stays open to show what went wrong.
                self.close_window(id);
            }
            if self.updating.is_empty() && self.update_queue.is_empty() {
                self.output.push(if failed {
                    "Update finished with errors".into()
                } else {
                    "Project updated".into()
                });
            }
            // The analyses after it take its new results before they are solved.
            self.sync_due = true;
        }
        if self.sync_due || self.restoring.values().any(Option::is_some) {
            return;
        }
        let parallel = self.settings.solver.parallel_runs.max(1) as usize;
        while self.updating.len() < parallel {
            // The first queued analysis whose sources are neither queued nor solving.
            let busy = |source: u64| {
                self.update_queue.contains(&source)
                    || self.updating.iter().any(|(id, _)| *id == source)
            };
            let ready = self.update_queue.iter().position(|&id| {
                !(self.links.iter()).any(|l| l.target == id && l.source != id && busy(l.source))
            });
            let Some(id) = ready.and_then(|index| self.update_queue.remove(index)) else {
                return;
            };
            let opened = self.node(id).is_some_and(|n| n.window.is_none());
            self.open_window(ctx, id, None);
            self.updating.push((id, opened));
            if let Some(window) = self.node(id).and_then(|n| n.window.as_mut()) {
                window.solve_when_loaded();
            }
        }
    }

    /// The analyses that take data from `id`, directly or through others.
    fn dependents(&self, id: u64) -> Vec<u64> {
        let mut found = Vec::new();
        let mut queue = vec![id];
        while let Some(source) = queue.pop() {
            for link in self.links.iter().filter(|l| l.source == source) {
                if !found.contains(&link.target) && link.target != id {
                    found.push(link.target);
                    queue.push(link.target);
                }
            }
        }
        found
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
                if let Some(action) = add_block_menu(ui, self.free_position()) {
                    let ctx = ui.ctx().clone();
                    self.act(&ctx, action);
                }
                ui.separator();
                let show_all = egui::Button::new("Show All Blocks").shortcut_text("Home");
                if ui.add(show_all).clicked() {
                    self.show_all = true;
                }
                ui.separator();
                let update = egui::Button::new("Update Project");
                if ui.add(update).on_hover_text(UPDATE_TIP).clicked() {
                    self.update_project();
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
            if icons::button(ui, Icon::New, "New Project", true, false).clicked() {
                self.new_project();
            }
            let can_open = self.opening.is_none();
            if icons::button(ui, Icon::Open, "Open Project (Ctrl+O)", can_open, false).clicked() {
                self.open_dialog(ui.ctx());
            }
            if icons::button(ui, Icon::Save, "Save Project (Ctrl+S)", true, false).clicked() {
                self.save(false);
            }
            ui.separator();
            let updating = !self.updating.is_empty() || !self.update_queue.is_empty();
            let update = icons::button(ui, Icon::Solve, UPDATE_TIP, !updating, false).clicked();
            let text = egui::Button::new("Update Project").frame(false);
            let text = ui.add_enabled(!updating, text).on_hover_text(UPDATE_TIP);
            if update || text.clicked() {
                self.update_project();
            }
            ui.add_space(4.0);
            if icons::button(ui, Icon::Stop, "Stop Update Project", updating, false).clicked() {
                self.update_queue.clear();
                self.updating.clear();
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
        let mut bundles: HashMap<u64, Vec<Bundle>> = HashMap::new();
        for link in &self.links {
            if let Some(source) = self.nodes.iter().find(|n| n.id == link.source) {
                (incoming.entry(link.target).or_default())
                    .extend(links::filled_paths(link, &source.name));
                if links::is_bundle(link) {
                    bundles.entry(link.target).or_default().push(Bundle {
                        path: link.item.clone(),
                        source: source.name.clone(),
                        taken: link.inherited.len(),
                        excluded: link.excluded.len(),
                    });
                }
            }
        }
        let blocks: Vec<BlockView> = (self.nodes.iter())
            .map(|n| {
                let fe = n.link_data().map(|d| d.fe);
                let rows = match (n.kind, &n.status) {
                    (Kind::Materials, _) => fe
                        .map(|fe| material_rows(fe, &n.expanded))
                        .unwrap_or_default(),
                    (Kind::Mesh, _) => mesh_rows(n),
                    (Kind::Analysis, Some(status)) => block_rows(
                        status,
                        fe,
                        &n.expanded,
                        incoming.get(&n.id).map_or(&[][..], Vec::as_slice),
                        bundles.get(&n.id).map_or(&[][..], Vec::as_slice),
                    ),
                    (Kind::Analysis, None) => Vec::new(),
                };
                let continues = (self.links.iter())
                    .find(|l| l.target == n.id && links::is_state(l))
                    .map(|l| self.name_of(l.source));
                BlockView {
                    id: n.id,
                    name: n.name.clone(),
                    kind: n.kind,
                    space: n.space,
                    steps: (fe.into_iter().flat_map(|fe| &fe.steps))
                        .map(|s| s.name.clone())
                        .collect(),
                    continues,
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
            to_global: Default::default(),
            show_all: std::mem::take(&mut self.show_all).then(|| ui.max_rect()),
        };
        let response = SnarlWidget::new()
            .id_salt(("project schematic", self.graph_generation))
            .style(schematic_style())
            .show(&mut self.graph, &mut viewer, ui);
        // A double-click into empty space shows the blocks to add there, as in Blender.
        if response.double_clicked()
            && let Some(screen) = response.interact_pointer_pos()
        {
            let graph = viewer.to_global.inverse() * screen;
            self.picker = Some(NodePicker::new(screen, graph));
        }
        if let Some(picker) = &mut self.picker {
            let kinds = block_kinds();
            let entries: Vec<_> = kinds.iter().map(|k| k.entry()).collect();
            match picker.show(ui.ctx(), &entries) {
                node_picker::Outcome::Open => {}
                node_picker::Outcome::Closed => self.picker = None,
                node_picker::Outcome::Picked(i) => {
                    viewer.actions.push(kinds[i].action(picker.graph));
                    self.picker = None;
                }
            }
        }
        if self.nodes.is_empty() {
            ui.put(
                egui::Rect::from_center_size(ui.max_rect().center(), egui::vec2(420.0, 60.0)),
                egui::Label::new(
                    egui::RichText::new(
                        "Double-click here, or use the Project menu, to add an analysis,\n\
                         materials or a mesh. Double-click an analysis to open it in its\n\
                         own window.",
                    )
                    .weak(),
                )
                // Clicks go through to the schematic below.
                .selectable(false),
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
            // A link of a category is a bundle of links of its items: a wire per item
            // between the rows that show it, one wire while both categories are closed.
            let paths: Vec<(Vec<String>, Vec<String>)> =
                if links::is_bundle(link) && !link.inherited.is_empty() {
                    (link.inherited.iter())
                        .map(|name| {
                            let mut path = link.item.clone();
                            path.push(name.clone());
                            (path.clone(), path)
                        })
                        .collect()
                } else {
                    vec![(links::output_path(link), links::slot(link).to_vec())]
                };
            let nodes = (self.graph_node(link.source), self.graph_node(link.target));
            for (output, input) in paths {
                let output = shown_row(&source.rows, &output);
                let input = shown_row(&target.rows, &input);
                if let (Some(output), Some(input), (Some(from), Some(to))) = (output, input, nodes)
                {
                    wanted.insert((
                        egui_snarl::OutPinId { node: from, output },
                        egui_snarl::InPinId { node: to, input },
                    ));
                }
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
        // Materials have no model space.
        let space = |id: u64| {
            (self.nodes.iter())
                .find(|n| n.id == id && n.kind != Kind::Materials)
                .map(|n| n.space)
        };
        if let (Some(a), Some(b)) = (space(source), space(target))
            && a != b
        {
            self.output.push(format!(
                "Not linked: {from} is {} and {to} {}; linked analyses need the same model \
                 space.",
                a.label(),
                b.label()
            ));
            return;
        }
        if links::makes_cycle(&self.links, source, target) {
            self.output.push(format!(
                "Not linked: {to} comes before {from} already, and a link cannot go back."
            ));
            return;
        }
        let state = slot.first().is_some_and(|c| c == links::STATE);
        if let Some(chain) = (self.links.iter()).find(|l| l.target == target && links::is_state(l))
            && !state
        {
            self.output.push(format!(
                "Not linked: {to} continues {} and takes its model from there; unlink it first.",
                self.name_of(chain.source)
            ));
            return;
        }
        if state {
            self.link_state(source, item, target);
            return;
        }
        let results = item.first().is_some_and(|c| c == links::RESULTS);
        // A category or item links into the same place of the target; a step's results go
        // to the step they are dropped on.
        let slot = if results {
            // What the results are used for follows from the steps at both ends.
            let fe = |id: u64| {
                (self.nodes.iter().find(|n| n.id == id))
                    .and_then(Node::link_data)
                    .map(|d| d.fe.clone())
            };
            let (Some(source_fe), Some(target_fe)) = (fe(source), fe(target)) else {
                return;
            };
            let step = item
                .get(1)
                .and_then(|n| source_fe.steps.iter().find(|s| &s.name == n));
            let Some(step) = step else {
                return;
            };
            match links::results_slot(step, &target_fe, &slot) {
                Ok(slot) => slot,
                Err(why) => {
                    self.output.push(format!(
                        "Not linked: {} of {from} cannot go to {} of {to}: {why}.",
                        links::label(&item),
                        links::label(&slot)
                    ));
                    return;
                }
            }
        } else {
            item.clone()
        };
        // An item of a category the target takes from the same source already: taken
        // again if it was left out.
        if let [cell, category, name] = slot.as_slice()
            && cell == links::SETUP
            && !results
            && let Some(bundle) = self.links.iter_mut().find(|l| {
                l.source == source
                    && l.target == target
                    && links::is_bundle(l)
                    && links::setup_category(l) == Some(category)
            })
        {
            let text = match bundle.excluded.iter().position(|n| n == name) {
                Some(at) => {
                    bundle.excluded.remove(at);
                    format!("{to} takes {name} from {from} again, with the rest of {category}.")
                }
                None => format!("{to} takes {name} from {from} already, with all of {category}."),
            };
            self.output.push(text);
            self.sync_due = true;
            return;
        }
        // What the source has of the category a category or an item goes to.
        let source_entries = match slot.as_slice() {
            [cell, category, ..] if cell == links::SETUP && !results => (self.nodes.iter())
                .find(|n| n.id == source)
                .and_then(Node::link_data)
                .map(|d| links::entries(d.fe, category))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let usage = |link: &Link| links::ResultsUse::of(link);
        let new_usage = usage(&Link {
            source,
            target,
            item: item.clone(),
            slot: slot.clone(),
            ..Default::default()
        });
        let overlaps = |link: &Link| {
            if link.target != target || links::is_results(link) != results {
                return false;
            }
            if results {
                return usage(link) == new_usage
                    && match new_usage {
                        // A submodel reads one global results file.
                        Some(links::ResultsUse::Submodel) => {
                            link.source != source || links::slot(link) == slot.as_slice()
                        }
                        // A step takes one temperature field.
                        Some(links::ResultsUse::Temperature) => {
                            links::slot(link) == slot.as_slice()
                        }
                        // The model takes one set of eigenmodes and of initial temperatures.
                        _ => true,
                    };
            }
            let other = links::slot(link);
            // A category is a collection: links of categories and items of it from other
            // sources stay, and the later one wins an item both have.
            if let ([a, b, ..], [c, d, ..]) = (other, slot.as_slice())
                && a == links::SETUP
                && c == links::SETUP
            {
                return b == d
                    && match (other.len(), slot.len()) {
                        (3, 3) => other == slot.as_slice(),
                        (2, 2) => link.source == source,
                        (3, 2) => link.source == source || source_entries.contains(&other[2]),
                        _ => false,
                    };
            }
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
        // An item a category link of another source offers too comes from here now.
        if let [_, category, name] = slot.as_slice()
            && !results
        {
            let others: Vec<String> = (self.links.iter())
                .filter(|l| {
                    l.target == target
                        && links::is_bundle(l)
                        && links::setup_category(l) == Some(category)
                        && l.inherited.contains(name)
                })
                .map(|l| self.name_of(l.source))
                .collect();
            for other in others {
                self.output.push(format!(
                    "{to} takes {name} from {from} in place of {other}."
                ));
            }
        }
        let (what, step) = (links::label(&item), slot.get(1).map_or("", String::as_str));
        let text = match new_usage {
            Some(links::ResultsUse::Submodel) => {
                format!("{to} takes the displacements of {what} of {from} in {step} (submodel).")
            }
            Some(links::ResultsUse::Temperature) => {
                format!("{to} takes the temperatures of {what} of {from} in {step}.")
            }
            Some(links::ResultsUse::Modes) => format!(
                "{to} takes the eigenmodes of {what} of {from} for its modal and steady state \
                 dynamics steps."
            ),
            Some(links::ResultsUse::InitialTemperatures) => {
                format!("{to} starts at the temperatures of {what} of {from}.")
            }
            None => format!("{to} takes {} from {from}.", links::label(&item)),
        };
        self.output.push(text);
        self.applied.retain(|(id, _), _| *id != target);
        self.links.push(Link {
            source,
            target,
            slot: if results { slot } else { Vec::new() },
            item,
            ..Default::default()
        });
        self.sync_due = true;
    }

    /// Makes `target` continue `source` after the step whose results `item` names: it takes
    /// the source's model and its steps up to that one, in place of all it took before.
    fn link_state(&mut self, source: u64, item: Vec<String>, target: u64) {
        let (from, to) = (self.name_of(source), self.name_of(target));
        let Some(step) = item.get(1).cloned() else {
            return;
        };
        let replaced: Vec<Link> = (self.links.iter())
            .filter(|l| l.target == target)
            .cloned()
            .collect();
        self.links.retain(|l| l.target != target);
        for link in replaced {
            self.output.push(format!(
                "{to} no longer takes {} from {}.",
                links::label(&link.item),
                self.name_of(link.source)
            ));
        }
        self.output.push(format!(
            "{to} continues {from} from the state after {step}: it takes the model of {from} \
             and its steps up to {step}; its own steps follow."
        ));
        self.applied.retain(|(id, _), _| *id != target);
        self.links.push(Link {
            source,
            target,
            item: vec![links::STATE.to_owned(), step],
            slot: vec![links::STATE.to_owned()],
            ..Default::default()
        });
        self.sync_due = true;
    }

    /// A new analysis that continues `source` after `step`, with its model space and units.
    fn continue_from(&mut self, source: u64, step: String, position: egui::Pos2) -> Option<u64> {
        let properties = self.node(source)?.link_data()?.fe.properties.clone();
        let name = format!("{} continued", self.name_of(source));
        let id = self.add_analysis(&name, position, properties);
        self.output.push(format!("{} created", self.name_of(id)));
        let item = vec![links::RESULTS.to_owned(), step];
        self.link(source, item, id, vec![links::STATE.to_owned()]);
        Some(id)
    }

    /// Splits the analysis after `step`: the steps after it go to a new analysis that
    /// continues it, and what took their results takes them from there.
    fn split(&mut self, id: u64, step: String) {
        let Some(fe) = self
            .node(id)
            .and_then(|n| n.link_data())
            .map(|d| d.fe.clone())
        else {
            return;
        };
        let Some(index) = fe.steps.iter().position(|s| s.name == step) else {
            return;
        };
        if index + 1 >= fe.steps.len() {
            return;
        }
        if let Some(window) = self.node(id).and_then(|n| n.window.as_ref())
            && let Some(reason) = window.busy()
        {
            self.output.push(format!("Not split while {reason}."));
            return;
        }
        let tail: Vec<plx_model::Step> = fe.steps[index + 1..].to_vec();
        let moved: Vec<String> = tail.iter().map(|s| s.name.clone()).collect();
        let position = (self.graph_node(id))
            .and_then(|g| self.graph.get_node_info(g))
            .map_or(self.free_position(), |info| {
                info.pos + egui::vec2(420.0, 0.0)
            });
        let Some(new) = self.continue_from(id, step.clone(), position) else {
            return;
        };
        if let Some(data) = self.node(new).and_then(|n| n.data.as_mut()) {
            data.model.steps = tail;
        }
        // What took the results or state of a moved step takes them from the new analysis.
        for link in &mut self.links {
            let moved_step = link.item.get(1).is_some_and(|s| moved.contains(s));
            if link.source == id && link.target != new && moved_step {
                link.source = new;
            }
        }
        let mut kept = fe.clone();
        kept.steps.truncate(index + 1);
        let path = [links::SETUP.to_owned(), "Steps".to_owned()];
        self.set_steps(id, &kept, &path);
        self.output.push(format!(
            "{} split after {step}: {} continues it with {}.",
            self.name_of(id),
            self.name_of(new),
            moved.join(", ")
        ));
    }

    /// Gives the own steps of the analysis to the one it continues and removes it; only when
    /// it continues from the last step of that one.
    fn merge(&mut self, id: u64) {
        let Some(link) = (self.links.iter())
            .find(|l| l.target == id && links::is_state(l))
            .cloned()
        else {
            return;
        };
        let (name, into) = (self.name_of(id), self.name_of(link.source));
        let source = self
            .node(link.source)
            .and_then(|n| n.link_data())
            .map(|d| d.fe.clone());
        let target = self
            .node(id)
            .and_then(|n| n.link_data())
            .map(|d| d.fe.clone());
        let (Some(mut source), Some(target)) = (source, target) else {
            return;
        };
        if source.steps.last().map(|s| &s.name) != link.item.get(1) {
            self.output.push(format!(
                "Not merged: {name} continues {into} from a step before its last; only an \
                 analysis that continues from the last step can be merged into it."
            ));
            return;
        }
        if !self.close_window(id) {
            return;
        }
        let own = links::own_steps(&target, &link);
        let moved: Vec<String> = own.iter().map(|s| s.name.clone()).collect();
        source.steps.extend(own);
        let path = [links::SETUP.to_owned(), "Steps".to_owned()];
        self.set_steps(link.source, &source, &path);
        // What took results or states of the merged analysis takes them from the other one.
        for other in &mut self.links {
            if other.source == id {
                other.source = link.source;
            }
        }
        self.delete(id);
        self.output.push(format!(
            "{name} merged into {into}: {into} solves {} too.",
            moved.join(", ")
        ));
        self.sync_due = true;
    }

    /// Replaces the steps of an analysis with those of `fe`.
    fn set_steps(&mut self, id: u64, fe: &plx_model::FeModel, path: &[String]) {
        let Some(node) = self.node(id) else {
            return;
        };
        match &mut node.window {
            Some(window) => window.apply_setup(fe, path),
            None => {
                if let Some(data) = &mut node.data {
                    links::apply_setup(&mut data.model, fe, path);
                }
                node.refresh_closed();
            }
        }
        self.sync_due = true;
    }

    /// Leaves out an item of the categories that links take as a whole, named by the pin of
    /// its row in the source or in the target: the target keeps its copy, which is its own
    /// from then on.
    fn exclude(
        &mut self,
        source: Option<&(u64, Vec<String>)>,
        target: Option<&(u64, Vec<String>)>,
    ) {
        let item = |end: Option<&(u64, Vec<String>)>, id: u64, category: &[String]| {
            end.map(|(at, path)| {
                (*at == id && path.len() == category.len() + 1 && links::covers(category, path))
                    .then(|| path[category.len()].clone())
            })
        };
        let mut left_out = Vec::new();
        for link in self.links.iter_mut().filter(|l| links::is_bundle(l)) {
            let name = match (
                item(source, link.source, &link.item),
                item(target, link.target, &link.item),
            ) {
                (Some(Some(a)), Some(Some(b))) if a == b => a,
                (Some(Some(a)), None) | (None, Some(Some(a))) => a,
                _ => continue,
            };
            if link.inherited.contains(&name) && !link.excluded.contains(&name) {
                link.excluded.push(name.clone());
                link.inherited.retain(|n| *n != name);
                left_out.push((link.source, link.target, name));
            }
        }
        for (source, target, name) in left_out {
            self.output.push(format!(
                "{} no longer takes {name} from {}; it keeps its copy.",
                self.name_of(target),
                self.name_of(source)
            ));
            self.sync_due = true;
        }
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
            let submodel = (self.links.iter()).any(|l| {
                l.target == target && links::ResultsUse::of(l) == Some(links::ResultsUse::Submodel)
            });
            if links::is_results(&link)
                && let Some(node) = self.node(target)
            {
                match &mut node.window {
                    Some(window) => window.clear_results(&link, submodel),
                    None => {
                        if let Some(data) = &mut node.data {
                            links::clear_results(&mut data.model, &link, submodel);
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
            let mut missing = Vec::new();
            for link in &to_target {
                let source = self.name_of(link.source);
                if let Some(payload) = self.payload(link, &mut missing)
                    && let Some(inherited) = self.apply(target, &source, link, payload)
                    && let Some(own) = self.links.iter_mut().find(|l| *l == link)
                {
                    own.inherited = inherited;
                }
            }
            let incoming = self.incoming(target);
            if let Some(node) = self.node(target) {
                match &mut node.window {
                    Some(window) => window.set_links(incoming, missing),
                    None => node.refresh_closed(),
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(300));
    }

    /// What links fill in `target`, and from where it continues and where others continue
    /// it.
    fn incoming(&self, target: u64) -> links::Incoming {
        let mut incoming = links::Incoming::default();
        for link in self.links.iter().filter(|l| l.target == target) {
            let source = self.name_of(link.source);
            incoming.paths.extend(links::filled_paths(link, &source));
        }
        let fe = |id: u64| {
            (self.nodes.iter().find(|n| n.id == id))
                .and_then(Node::link_data)
                .map(|d| d.fe)
        };
        let dir =
            (self.settings.solver.work_dir()).join(crate::analysis::job_name(&self.project_name()));
        // The analyses it continues, nearest first.
        let mut at = target;
        while let Some(link) = (self.links.iter()).find(|l| l.target == at && links::is_state(l)) {
            at = link.source;
            if at == target {
                break;
            }
            let job = crate::analysis::job_name(&self.name_of(at));
            incoming.continuation.upstream.push((dir.join(&job), job));
        }
        // The own steps of those that continue it, and of those that continue them.
        if let Some(own) = fe(target) {
            for link in self
                .links
                .iter()
                .filter(|l| l.source == target && links::is_state(l))
            {
                let Some(index) = links::state_step(own, link) else {
                    continue;
                };
                let mut steps = Vec::new();
                let mut queue = vec![link.clone()];
                while let Some(link) = queue.pop() {
                    if let Some(fe) = fe(link.target) {
                        steps.extend(links::own_steps(fe, &link));
                    }
                    queue.extend(
                        (self.links.iter())
                            .filter(|l| l.source == link.target && links::is_state(l))
                            .cloned(),
                    );
                }
                incoming.continuation.dependent_steps.push((index, steps));
            }
        }
        incoming
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
                let data = data?;
                let global = links::global_step_number(data.fe, step)?;
                let usage = links::ResultsUse::of(link)?;
                if usage != links::ResultsUse::Submodel
                    && target.link_data()?.mesh.input_fingerprint() != data.mesh.input_fingerprint()
                {
                    // Temperatures and eigenmodes are taken node by node.
                    let line = format!(
                        "{} cannot take the {} of {}: their meshes differ. Link the mesh, or \
                         continue from a state, so that both have the same nodes.",
                        target.name,
                        links::label(&link.item),
                        source.name
                    );
                    if self.warned.insert(line.clone()) {
                        self.output.push(line);
                    }
                    return None;
                }
                if usage == links::ResultsUse::Modes {
                    // CalculiX reads them from the run's own folder.
                    let job = crate::analysis::job_name(&source.name);
                    let file = (self.settings.solver.work_dir())
                        .join(crate::analysis::job_name(&self.project_name()))
                        .join(&job)
                        .join(format!("{job}.eig"));
                    return Some(Payload::Results(global, file));
                }
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
                Some(Payload::Results(global, file))
            }
            Some(links::STATE) => {
                if source.bound_results().is_none() {
                    // Solved first, so that this one continues from its states.
                    missing.push(source.name.clone());
                }
                let (data, own) = (data?, target.link_data()?);
                let step = links::state_step(data.fe, link)?;
                let mut print = plx_mesh::Fnv::new();
                for cell in [links::GEOMETRY, links::MESH] {
                    let path = [cell.to_owned()];
                    print.add(&links::fingerprint(data, &path).to_le_bytes());
                }
                let print = print.finish();
                let key = (link.target, vec![links::STATE.to_owned()]);
                let mesh = (self.applied.get(&key) != Some(&print))
                    .then(|| Box::new(links::MeshCopy::of(data)));
                let fe = (!links::chain_up_to_date(data.fe, step, own.fe, link))
                    .then(|| Box::new(data.fe.clone()));
                if mesh.is_none() && fe.is_none() {
                    return None;
                }
                self.applied.insert(key, print);
                Some(Payload::Chain { mesh, fe, step })
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
                if let Some(category) = links::setup_category(link) {
                    // The links of a category and its items to the same target: a later one
                    // wins an item both offer.
                    let at = self.links.iter().position(|l| l == link)?;
                    let same = |l: &&Link| {
                        l.target == link.target && links::setup_category(l) == Some(category)
                    };
                    let offers = |l: &Link| -> Vec<String> {
                        let Some(fe) = (self.nodes.iter())
                            .find(|n| n.id == l.source)
                            .and_then(Node::link_data)
                            .map(|d| d.fe)
                        else {
                            return Vec::new();
                        };
                        let present = links::entries(fe, category);
                        (links::offered(fe, l).into_iter())
                            .filter(|name| present.contains(name))
                            .collect()
                    };
                    let later: Vec<String> = self.links[at + 1..]
                        .iter()
                        .filter(same)
                        .flat_map(offers)
                        .collect();
                    if links::is_bundle(link) {
                        let names: Vec<String> = (links::offered(data.fe, link).into_iter())
                            .filter(|name| !later.contains(name))
                            .collect();
                        if links::bundle_up_to_date(data.fe, own.fe, link, &names) {
                            return None;
                        }
                        let ahead = self.links[..at]
                            .iter()
                            .filter(same)
                            .flat_map(offers)
                            .collect();
                        return Some(Payload::Bundle {
                            fe: Box::new(data.fe.clone()),
                            names,
                            ahead,
                        });
                    }
                    if link.item.get(2).is_some_and(|name| later.contains(name)) {
                        return None;
                    }
                }
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

    /// Copies what a link takes into its target, see [`ProjectApp::payload`]; returns the
    /// steps a state link took.
    fn apply(
        &mut self,
        target: u64,
        source: &str,
        link: &Link,
        payload: Payload,
    ) -> Option<Vec<String>> {
        let node = self.node(target)?;
        let path = &link.item;
        if let Payload::Chain { mesh, fe, step } = payload {
            let mut inherited = None;
            match &mut node.window {
                Some(window) => {
                    if let Some(mesh) = mesh {
                        window.apply_mesh(*mesh);
                    }
                    if let Some(fe) = fe {
                        inherited = Some(window.apply_chain(&fe, step, link));
                    }
                    window.message(format!("Model and steps taken from {source}"));
                }
                None => {
                    let data = node.data.as_mut()?;
                    if let Some(mesh) = mesh {
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
                    if let Some(fe) = fe {
                        inherited = Some(links::apply_chain(&mut data.model, &fe, step, link));
                    }
                }
            }
            return inherited;
        }
        if let Payload::Bundle { fe, names, ahead } = payload {
            let taken = match &mut node.window {
                Some(window) => {
                    let taken = window.apply_bundle(&fe, link, &names, &ahead);
                    window.message(format!("{} taken from {source}", links::label(path)));
                    taken
                }
                None => {
                    let model = &mut node.data.as_mut()?.model;
                    links::apply_bundle(model, &fe, link, &names, &ahead)
                }
            };
            return Some(taken);
        }
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
                Payload::Results(global, file) => window.apply_results(link, global, file),
                Payload::Chain { .. } | Payload::Bundle { .. } => {}
            }
            return None;
        }
        let data = node.data.as_mut()?;
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
            Payload::Results(global, file) => {
                links::apply_results(&mut data.model, link, global, file);
            }
            Payload::Chain { .. } | Payload::Bundle { .. } => {}
        }
        None
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
        // The solve waits for the links to bring a new mesh.
        self.generate_meshes();
        let mut sources = Vec::new();
        let mut queue = vec![id];
        while let Some(target) = queue.pop() {
            for link in self.links.iter().filter(|l| l.target == target) {
                let solved_first = links::is_results(link) || links::is_state(link);
                if solved_first && !sources.contains(&link.source) {
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
            Action::New(pos) => self.ask_new_analysis(pos),
            Action::AddMaterials(pos) => {
                self.add_materials(pos);
            }
            Action::AddMesh(pos, primitive) => {
                self.add_mesh(pos, primitive);
            }
            Action::Generate(id) => self.generate_mesh(id),
            Action::Open(id, view) => self.open_window(ctx, id, view),
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
            Action::Continue {
                source,
                step,
                position,
            } => {
                self.continue_from(source, step, position);
            }
            Action::Split(id, step) => self.split(id, step),
            Action::Merge(id) => self.merge(id),
            Action::Unlink { source, target } => {
                self.exclude(source.as_ref(), target.as_ref());
                self.unlink(|link| {
                    source.as_ref().is_none_or(|(id, path)| {
                        link.source == *id && links::covers(path, &links::output_path(link))
                    }) && target.as_ref().is_none_or(|(id, path)| {
                        link.target == *id && links::covers(path, links::slot(link))
                    })
                })
            }
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
        let noun = (self.nodes.iter().find(|n| n.id == id)).map_or("analysis", |n| n.kind.noun());
        egui::Window::new(format!("Rename {}", title_case(noun)))
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
                        "Another block of the project has this name.",
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
            if let Some(editor) = self.material_set.as_mut().filter(|e| e.id == id) {
                editor.rename(&name);
            }
            if let Some(editor) = self.mesh_block.as_mut().filter(|e| e.id == id) {
                editor.rename(&name);
            }
        } else if open {
            self.rename = Some((id, name));
        }
    }

    fn confirm_delete_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete else {
            return;
        };
        let Some(node) = self.nodes.iter().find(|n| n.id == id) else {
            self.confirm_delete = None;
            return;
        };
        let (name, kind) = (node.name.clone(), node.kind);
        let what = match kind {
            Kind::Analysis => "with its model and results",
            Kind::Materials => "with its materials",
            Kind::Mesh => "with its mesh",
        };
        let users = (self.links.iter())
            .filter(|l| l.source == id)
            .map(|l| l.target)
            .collect::<BTreeSet<_>>()
            .len();
        let mut answer = None;
        egui::Window::new(format!("Delete {}", title_case(kind.noun())))
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                ui.label(format!("Delete {name} {what} from the project?"));
                if kind.is_source() && users > 0 {
                    ui.label(format!(
                        "{users} analyses link to it; they keep their copies."
                    ));
                }
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

    /// Asks for the model space, unit system and type of a new analysis, whose block goes
    /// to `position`; the last choice is proposed.
    fn ask_new_analysis(&mut self, position: egui::Pos2) {
        let properties = ModelProperties {
            kind: plx_model::ModelKind::General,
            global_results: None,
            ..self.settings.new_model.clone()
        };
        self.new_analysis = Some((position, ModelPropertiesDialog::create(properties)));
    }

    fn new_analysis_window(&mut self, ctx: &egui::Context) {
        let Some((position, dialog)) = &mut self.new_analysis else {
            return;
        };
        let position = *position;
        match dialog.show(ctx, None, None) {
            DialogResult::Open => {}
            DialogResult::Cancel => self.new_analysis = None,
            DialogResult::Ok(properties) => {
                self.new_analysis = None;
                self.settings.new_model = ModelProperties {
                    kind: plx_model::ModelKind::General,
                    global_results: None,
                    ..properties.clone()
                };
                let space = properties.space;
                let id = self.add_analysis("Analysis-1", position, properties);
                let name = self.name_of(id);
                self.output.push(format!(
                    "{name} created: {}, {}",
                    space.label(),
                    self.settings.new_model.units.label()
                ));
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
            if let Some(model) = window.model() {
                node.space = model.fe.properties.space;
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
                Request::SolveChain => self.solve_chain(id),
                Request::OpenAnalysis(name) => {
                    if let Some(source) = self.nodes.iter().find(|n| n.name == name) {
                        let source = source.id;
                        self.open_window(ctx, source, None);
                    }
                }
                // Unlinking what a state link took makes the analysis independent.
                Request::Unlink(path) => {
                    self.exclude(None, Some(&(id, path.clone())));
                    self.unlink(|l| {
                        l.target == id
                            && (links::covers(&path, links::slot(l)) || links::is_state(l))
                    });
                }
            }
        }
    }
}

/// The workspace a double-click on the row at `path` opens the analysis in: Geometry and
/// Mesh in the Geometry workspace, the setup in the FE Model workspace, Results in the
/// Results workspace.
fn workspace_of(path: &[String]) -> Option<TreeView> {
    match path.first().map(String::as_str) {
        Some(links::GEOMETRY | links::MESH) => Some(TreeView::Geometry),
        Some(links::SETUP) => Some(TreeView::FeModel),
        Some(links::RESULTS) => Some(TreeView::Results),
        _ => None,
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
        let typing = ctx.memory(|m| m.focused().is_some());
        if !typing && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Home)) {
            self.show_all = true;
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
        self.material_set_window(&ctx);
        self.mesh_block_window(&ctx);
        self.new_analysis_window(&ctx);
        self.settings_window(&ctx);
        self.updater.show(&ctx, &mut self.output);
        self.sync_links(&ctx);
        if self.update_after_sync && !self.sync_due {
            self.update_after_sync = false;
            self.update_project();
        }
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
    icon: NodeIcon,
    state: Option<CellState>,
    /// Whether the row opens to show what is in it, and whether it is open.
    expander: Option<bool>,
    /// Whether the row takes a link (left pin) and gives one (right pin).
    input: bool,
    output: bool,
    /// The analysis a link fills the row from.
    linked_from: Option<String>,
    /// Whether the row is a collection, whose pin takes any number of wires, and whether a
    /// category link to it leaves out items.
    collection: bool,
    partial: bool,
    tip: String,
}

/// A link of a whole category to a block, for its row: from where, how many items it takes
/// and how many it leaves out.
struct Bundle {
    path: Vec<String>,
    source: String,
    taken: usize,
    excluded: usize,
}

/// How a block of the schematic looks: what the viewer needs of an analysis.
struct BlockView {
    id: u64,
    name: String,
    kind: Kind,
    space: ModelSpace,
    /// Its steps, to split it after one, and the analysis it continues, to merge into.
    steps: Vec<String>,
    continues: Option<String>,
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
    bundles: &[Bundle],
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
            collection: false,
            partial: false,
            tip: format!("{name}: {}", state.text()),
            path,
        }
    };
    let state = vec![links::STATE.to_owned()];
    let continues = from(&state);
    let mut rows = vec![
        Row {
            text: match &continues {
                Some(source) => format!("State of {source}"),
                None => "State".to_owned(),
            },
            depth: 0,
            icon: NodeIcon::State,
            state: None,
            expander: None,
            input: true,
            output: false,
            tip: "Drop the results of a step of another analysis here: this analysis then \
                  continues from the state after that step. It takes the model and the steps \
                  up to it, and CalculiX solves only its own steps."
                .into(),
            linked_from: continues,
            collection: false,
            partial: false,
            path: state,
        },
        cell(links::GEOMETRY, NodeIcon::Geometry, status.geometry, true),
        cell(links::MESH, NodeIcon::Mesh, status.mesh, true),
    ];
    if !status.stale_parts.is_empty() {
        rows[1]
            .tip
            .push_str(&format!(" ({})", status.stale_parts.join(", ")));
    }
    let setup = vec![links::SETUP.to_owned()];
    let setup_open = expanded.contains(&setup);
    let mut row = cell(links::SETUP, NodeIcon::Setup, status.setup, true);
    row.expander = Some(setup_open);
    rows.push(row);
    if setup_open {
        for category in links::CATEGORIES {
            let path = vec![links::SETUP.to_owned(), category.to_owned()];
            let open = expanded.contains(&path);
            let entries = fe
                .map(|fe| links::entries(fe, category))
                .unwrap_or_default();
            let linked: Vec<&Bundle> = bundles.iter().filter(|b| b.path == path).collect();
            let mut tip = if category == "Initial Conditions" {
                "Initial Conditions: drag to another analysis to link them all, or drop the \
                 results of a thermal step here to start at its temperatures"
                    .to_owned()
            } else {
                format!("{category}: drag to another analysis to link them all")
            };
            tip.push_str(
                ".\nA collection: it takes any number of links, and its own items stay. \
                 Right-click an item's pin to leave it out of a linked category.",
            );
            for bundle in &linked {
                tip.push_str(&format!(
                    "\n{} of {} items from {}.",
                    bundle.taken,
                    bundle.taken + bundle.excluded,
                    bundle.source
                ));
            }
            let sources: Vec<&str> = linked.iter().map(|b| b.source.as_str()).collect();
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
                linked_from: from(&path)
                    .or_else(|| (!sources.is_empty()).then(|| sources.join(", "))),
                collection: links::is_collection(&path),
                partial: linked.iter().any(|b| b.excluded > 0),
                tip,
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
                        collection: false,
                        partial: false,
                        path,
                    });
                }
            }
        }
    }
    rows.push(cell("Solution", NodeIcon::Solution, status.solution, false));
    rows.push(cell("Results", NodeIcon::Results, status.results, false));
    for step in fe.map(|fe| fe.steps.as_slice()).unwrap_or_default() {
        let path = vec![links::RESULTS.to_owned(), step.name.clone()];
        let tip = if step.active {
            format!(
                "Results of {}: drag them to a step of another analysis, which takes the \
                 temperatures of a thermal step, the eigenmodes of a frequency step for modal \
                 or steady state dynamics, or else the displacements as a submodel; to the \
                 State row of another analysis, which continues from them; or to empty space \
                 to continue in a new analysis",
                step.name
            )
        } else {
            format!("{} is deactivated and has no results", step.name)
        };
        rows.push(Row {
            text: step.name.clone(),
            depth: 1,
            icon: NodeIcon::Step,
            state: None,
            expander: None,
            input: true,
            output: step.active,
            linked_from: from(&path),
            collection: false,
            partial: false,
            tip,
            path,
        });
    }
    rows
}

/// The rows of a materials block: all materials, then each of them while that row is open,
/// to link to analyses.
fn material_rows(fe: &plx_model::FeModel, expanded: &BTreeSet<Vec<String>>) -> Vec<Row> {
    let path = material_set_path();
    let open = expanded.contains(&path);
    let mut rows = vec![Row {
        text: format!("Materials ({})", fe.materials.len()),
        depth: 0,
        icon: NodeIcon::Materials,
        state: None,
        expander: (!fe.materials.is_empty()).then_some(open),
        input: false,
        output: !fe.materials.is_empty(),
        linked_from: None,
        collection: false,
        partial: false,
        tip: "All materials: drag them to the Materials row of an analysis, which then takes \
              each of them, also those added here later"
            .into(),
        path: path.clone(),
    }];
    for material in fe.materials.iter().filter(|_| open) {
        let mut path = path.clone();
        path.push(material.name.clone());
        rows.push(Row {
            tip: format!(
                "{}: drag it to the Materials row of an analysis to link it",
                material.name
            ),
            text: material.name.clone(),
            depth: 1,
            icon: NodeIcon::Materials,
            state: None,
            expander: None,
            input: false,
            output: true,
            linked_from: None,
            collection: false,
            partial: false,
            path,
        });
    }
    rows
}

/// The row of a mesh block: its mesh, to link to analyses, with its state.
fn mesh_rows(node: &Node) -> Vec<Row> {
    let state = node.mesh_state();
    let mesh = node.data.as_ref().map(|d| &d.mesh);
    let count = mesh.map_or(0, plx_mesh::FeMesh::element_count);
    let element = (mesh.and_then(|m| m.elements().first())).map_or("", |e| e.type_name.as_str());
    let sides = mesh.map_or_else(String::new, |m| {
        m.node_sets.keys().cloned().collect::<Vec<_>>().join(", ")
    });
    let mut tip = format!("Mesh: {}", state.text());
    if state == CellState::OutOfDate {
        tip = "Mesh: out of date: Update Project or Generate Mesh makes it again".into();
    }
    if count > 0 {
        tip.push_str(&format!(
            "\n{count} elements {element}, sides {sides} as node sets and surfaces.\nDrag it \
             to the Mesh row of an analysis to link it."
        ));
    }
    vec![Row {
        path: vec![links::MESH.to_owned()],
        text: links::MESH.to_owned(),
        depth: 0,
        icon: NodeIcon::Mesh,
        state: Some(state),
        expander: None,
        input: false,
        output: count > 0,
        linked_from: None,
        collection: false,
        partial: false,
        tip,
    }]
}

/// Tooltip of an Add Mesh entry.
/// A kind of block the schematic adds: the menus and the picker list these, in this order.
enum BlockKind {
    Analysis,
    Materials,
    Mesh(MeshPrimitive),
}

fn block_kinds() -> Vec<BlockKind> {
    vec![
        BlockKind::Analysis,
        BlockKind::Materials,
        BlockKind::Mesh(MeshPrimitive::default_box()),
        BlockKind::Mesh(MeshPrimitive::default_rectangle()),
    ]
}

impl BlockKind {
    fn entry(&self) -> node_picker::Entry {
        let (category, label, keywords, tip) = match self {
            Self::Analysis => (
                "Analysis",
                "Analysis",
                "new model static frequency buckle dynamic heat steps solve calculix",
                ANALYSIS_TIP,
            ),
            Self::Materials => (
                "Materials",
                "Materials",
                "library elastic density",
                MATERIALS_TIP,
            ),
            Self::Mesh(primitive) => {
                let keywords = match primitive {
                    MeshPrimitive::Box { .. } => "block cube hexahedra 3D structured",
                    MeshPrimitive::Rectangle { .. } => "square quadrilaterals 2D structured",
                };
                ("Mesh", primitive.label(), keywords, mesh_tip(primitive))
            }
        };
        node_picker::Entry {
            category,
            label,
            keywords,
            tip,
        }
    }

    /// The text of its menu item.
    fn menu_label(&self) -> &'static str {
        match self {
            Self::Analysis => "New Analysis …",
            Self::Materials => "Add Materials",
            Self::Mesh(primitive) => primitive.label(),
        }
    }

    /// Adds its block at the position.
    fn action(&self, pos: egui::Pos2) -> Action {
        match self {
            Self::Analysis => Action::New(pos),
            Self::Materials => Action::AddMaterials(pos),
            Self::Mesh(primitive) => Action::AddMesh(pos, primitive.clone()),
        }
    }
}

/// Menu items adding a block at the position: one per kind, the kinds of a group with more
/// than one in a submenu.
fn add_block_menu(ui: &mut egui::Ui, pos: egui::Pos2) -> Option<Action> {
    let kinds = block_kinds();
    let mut chosen = None;
    let mut done: Vec<&str> = Vec::new();
    for kind in &kinds {
        let category = kind.entry().category;
        if done.contains(&category) {
            continue;
        }
        done.push(category);
        let group: Vec<&BlockKind> = (kinds.iter())
            .filter(|k| k.entry().category == category)
            .collect();
        let mut item = |ui: &mut egui::Ui, kind: &BlockKind, text: &str| {
            if ui.button(text).on_hover_text(kind.entry().tip).clicked() {
                chosen = Some(kind.action(pos));
            }
        };
        if let [kind] = group[..] {
            item(ui, kind, kind.menu_label());
        } else {
            ui.menu_button(format!("Add {category}"), |ui| {
                for kind in group {
                    item(ui, kind, kind.menu_label());
                }
            });
        }
    }
    chosen
}

fn mesh_tip(primitive: &MeshPrimitive) -> &'static str {
    match primitive {
        MeshPrimitive::Box { .. } => {
            "Adds a block that meshes a box with hexahedra, for 3D analyses; its sides are \
             node sets and surfaces X-, X+, Y-, Y+, Z-, Z+."
        }
        MeshPrimitive::Rectangle { .. } => {
            "Adds a block that meshes a rectangle with quadrilaterals, for 2D analyses; its \
             sides are node sets and surfaces X-, X+, Y-, Y+."
        }
    }
}

/// The row of all materials of a materials block, open when the block is added.
fn material_set_path() -> Vec<String> {
    vec![links::SETUP.to_owned(), "Materials".to_owned()]
}

/// "materials block" as "Materials Block", for window titles.
fn title_case(text: &str) -> String {
    (text.split(' '))
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The row a path is shown in: the row itself, or the closed row it lies in.
fn shown_row(rows: &[Row], path: &[String]) -> Option<usize> {
    (rows.iter().enumerate())
        .filter(|(_, row)| links::covers(&row.path, path))
        .max_by_key(|(_, row)| row.path.len())
        .map(|(index, _)| index)
}

/// The symbol of each category of the setup.
fn category_icon(category: &str) -> NodeIcon {
    match category {
        "Features" => NodeIcon::Features,
        "Materials" => NodeIcon::Materials,
        "Sections" => NodeIcon::Sections,
        "Constraints" => NodeIcon::Constraints,
        "Contacts" => NodeIcon::Contacts,
        "Amplitudes" => NodeIcon::Amplitudes,
        "Initial Conditions" => NodeIcon::InitialConditions,
        _ => NodeIcon::Steps,
    }
}

/// Colour of the wires of a link: data in blue, results in orange, states in green.
fn wire_color(path: &[String]) -> egui::Color32 {
    if path.first().is_some_and(|c| c == links::STATE) {
        crate::style::adapt(egui::Color32::from_rgb(40, 150, 60))
    } else if path.first().is_some_and(|c| c == links::RESULTS) {
        crate::style::adapt(egui::Color32::from_rgb(230, 130, 20))
    } else {
        crate::style::adapt(egui::Color32::from_rgb(40, 110, 200))
    }
}

struct Viewer {
    nodes: Vec<BlockView>,
    actions: Vec<Action>,
    /// From the schematic to the screen.
    to_global: egui::emath::TSTransform,
    /// Where the schematic is shown, when its view is to fit all blocks.
    show_all: Option<egui::Rect>,
}

impl Viewer {
    fn block(&self, id: u64) -> Option<&BlockView> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The analysis and step whose results a wire dropped on empty space comes from.
    fn dropped_results(&self, pins: AnyPins, snarl: &Snarl<u64>) -> Option<(u64, String)> {
        let AnyPins::Out([pin]) = pins else {
            return None;
        };
        let (id, row) = self.row(snarl, pin.node, pin.output)?;
        match row.path.as_slice() {
            [cell, step] if cell == links::RESULTS && row.output => Some((id, step.clone())),
            _ => None,
        }
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
        let (rect, icon) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        match block.kind {
            Kind::Analysis | Kind::Mesh => {
                paint_space(ui.painter(), rect, block.space, ui.visuals().text_color());
                icon.on_hover_text(format!("Model space: {}", block.space.label()));
            }
            Kind::Materials => {
                let ink = ui.visuals().text_color();
                node_icons::paint(ui.painter(), rect.min, NodeIcon::Materials, ink);
                icon.on_hover_text("Materials for analyses to link");
            }
        }
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
        let (steps, continues, kind) = (block.steps.clone(), block.continues.clone(), block.kind);
        if label.double_clicked() {
            self.actions.push(Action::Open(id, None));
        }
        let tip = match kind {
            Kind::Analysis => "Double-click opens the analysis in its own window.",
            Kind::Materials => "Double-click edits the materials.",
            Kind::Mesh => "Double-click edits the size and division of the mesh.",
        };
        // The name takes the clicks of the header, so it opens the block's menu too.
        label.on_hover_text(tip).context_menu(|ui| {
            node_menu(
                ui,
                id,
                kind,
                &steps,
                continues.as_deref(),
                &mut self.actions,
            )
        });
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
            return BlockPin::single(hidden_pin());
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
        let ink = ui.visuals().text_color().gamma_multiply(0.75);
        node_icons::paint(painter, egui::pos2(x, middle - 7.0), row.icon, ink);
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
            self.actions.push(Action::Open(id, workspace_of(&row.path)));
        }
        let color = wire_color(&row.path);
        if row.path.first().is_some_and(|c| c == links::STATE) {
            BlockPin::single(PinInfo::square().with_fill(color).with_wire_color(color))
        } else if row.input {
            // A wire takes the mixed colour of its two pins: a wire from the results of a
            // step is orange on both ends, whichever row takes them.
            let results = (pin.remotes.iter()).any(|remote| {
                self.row(snarl, remote.node, remote.output)
                    .is_some_and(|(_, row)| row.path.first().is_some_and(|c| c == links::RESULTS))
            });
            let wire = if results {
                wire_color(&[links::RESULTS.to_owned()])
            } else {
                color
            };
            let info = PinInfo::circle().with_fill(color).with_wire_color(wire);
            BlockPin {
                info,
                collection: row.collection,
                partial: row.partial,
            }
        } else {
            BlockPin::single(hidden_pin())
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
                // A wire takes the mixed colour of its two pins, so the results pin of a step
                // whose wires all go to a State row gives them the green of a state link.
                let state = vec![links::STATE.to_owned()];
                let continues = !pin.remotes.is_empty()
                    && (pin.remotes.iter()).all(|remote| {
                        self.row(snarl, remote.node, remote.input)
                            .is_some_and(|(_, row)| row.path == state)
                    });
                let wire = if continues { wire_color(&state) } else { color };
                PinInfo::circle().with_fill(color).with_wire_color(wire)
            }
            _ => hidden_pin(),
        }
    }

    fn has_body(&mut self, _node: &u64) -> bool {
        false
    }

    /// A grid of dots behind the blocks, like the background of a node editor.
    fn draw_background(
        &mut self,
        _background: Option<&egui_snarl::ui::BackgroundPattern>,
        viewport: &egui::Rect,
        _snarl_style: &SnarlStyle,
        _style: &egui::Style,
        painter: &egui::Painter,
        _snarl: &Snarl<u64>,
    ) {
        let color = crate::style::schematic_dot();
        let first = |v: f32| (v / DOT_SPACING).floor() as i32;
        let last = |v: f32| (v / DOT_SPACING).ceil() as i32;
        for i in first(viewport.left())..=last(viewport.right()) {
            for j in first(viewport.top())..=last(viewport.bottom()) {
                let center = egui::pos2(i as f32 * DOT_SPACING, j as f32 * DOT_SPACING);
                painter.circle_filled(center, 1.0, color);
            }
        }
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
        if let Some(action) = add_block_menu(ui, pos) {
            self.actions.push(action);
            ui.close();
        }
    }

    fn current_transform(
        &mut self,
        to_global: &mut egui::emath::TSTransform,
        snarl: &mut Snarl<u64>,
    ) {
        if let Some(view) = self.show_all.take() {
            // The blocks, each about as large as it is drawn.
            let blocks = (snarl.nodes_pos_ids()).fold(egui::Rect::NOTHING, |all, (_, pos, id)| {
                let rows = self.block(*id).map_or(0, |b| b.rows.len() + b.steps.len());
                let size = egui::vec2(260.0, 60.0 + 22.0 * rows as f32);
                all.union(egui::Rect::from_min_size(pos, size))
            });
            if blocks.is_finite() {
                let blocks = blocks.expand(40.0);
                let scale = (view.width() / blocks.width())
                    .min(view.height() / blocks.height())
                    .clamp(MIN_SCALE, MAX_SCALE);
                *to_global = egui::emath::TSTransform::new(
                    view.center().to_vec2() - blocks.center().to_vec2() * scale,
                    scale,
                );
            }
        }
        self.to_global = *to_global;
    }

    /// Header bands tinted by the kind of block, like the node groups of Blender.
    fn header_frame(
        &mut self,
        default: egui::Frame,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<u64>,
    ) -> egui::Frame {
        match snarl
            .get_node(node)
            .and_then(|id| self.block(*id))
            .map(|b| b.kind)
        {
            Some(Kind::Materials) => default.fill(crate::style::materials_header()),
            Some(Kind::Mesh) => default.fill(crate::style::mesh_header()),
            _ => default,
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
        let block = self.block(id);
        let steps = block.map(|b| b.steps.clone()).unwrap_or_default();
        let continues = block.and_then(|b| b.continues.clone());
        let kind = block.map_or(Kind::Analysis, |b| b.kind);
        node_menu(
            ui,
            id,
            kind,
            &steps,
            continues.as_deref(),
            &mut self.actions,
        );
    }

    fn has_dropped_wire_menu(&mut self, src_pins: AnyPins, snarl: &mut Snarl<u64>) -> bool {
        self.dropped_results(src_pins, snarl).is_some()
    }

    fn show_dropped_wire_menu(
        &mut self,
        pos: egui::Pos2,
        ui: &mut egui::Ui,
        src_pins: AnyPins,
        snarl: &mut Snarl<u64>,
    ) {
        let Some((source, step)) = self.dropped_results(src_pins, snarl) else {
            ui.close();
            return;
        };
        let tip = "A new analysis that continues from the state after this step: it takes the \
                   model and the steps up to it, and its own steps are solved from there.";
        if ui.button("Continue from here").on_hover_text(tip).clicked() {
            self.actions.push(Action::Continue {
                source,
                step,
                position: pos,
            });
            ui.close();
        }
    }
}

/// A symbol of the model space: a cube for 3D, a thin plate for plane stress, a long prism
/// for plane strain and a profile turned about its axis for axisymmetric models.
fn paint_space(painter: &egui::Painter, rect: egui::Rect, space: ModelSpace, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.2, color);
    let p = |x: f32, y: f32| rect.min + egui::vec2(x, y);
    let line = |points: &[egui::Pos2]| {
        painter.add(egui::Shape::line(points.to_vec(), stroke));
    };
    match space {
        ModelSpace::ThreeD => {
            line(&[
                p(2.0, 6.0),
                p(11.0, 6.0),
                p(11.0, 15.0),
                p(2.0, 15.0),
                p(2.0, 6.0),
            ]);
            line(&[p(2.0, 6.0), p(6.0, 2.0), p(15.0, 2.0), p(11.0, 6.0)]);
            line(&[p(15.0, 2.0), p(15.0, 11.0), p(11.0, 15.0)]);
        }
        ModelSpace::PlaneStress => {
            // A thin plate, loaded in its plane.
            let face = [p(1.0, 4.0), p(13.0, 4.0), p(13.0, 14.0), p(1.0, 14.0)];
            painter.add(egui::Shape::convex_polygon(
                face.to_vec(),
                color.gamma_multiply(0.25),
                stroke,
            ));
            line(&[p(1.0, 4.0), p(3.0, 2.0), p(15.0, 2.0), p(13.0, 4.0)]);
            line(&[p(15.0, 2.0), p(15.0, 12.0), p(13.0, 14.0)]);
        }
        ModelSpace::PlaneStrain => {
            // A long body: a small section drawn out far into the depth.
            line(&[
                p(1.0, 9.0),
                p(7.0, 9.0),
                p(7.0, 15.0),
                p(1.0, 15.0),
                p(1.0, 9.0),
            ]);
            line(&[p(1.0, 9.0), p(9.0, 1.0), p(15.0, 1.0), p(7.0, 9.0)]);
            line(&[p(15.0, 1.0), p(15.0, 7.0), p(7.0, 15.0)]);
        }
        ModelSpace::Axisymmetric => {
            // The axis of revolution, the profile beside it and the turn about the axis.
            let dash = egui::Stroke::new(1.0, color);
            painter.extend(egui::Shape::dashed_line(
                &[p(3.0, 0.5), p(3.0, 15.5)],
                dash,
                2.0,
                1.5,
            ));
            let profile = [p(7.0, 5.0), p(12.0, 5.0), p(12.0, 13.0), p(7.0, 13.0)];
            painter.add(egui::Shape::convex_polygon(
                profile.to_vec(),
                color.gamma_multiply(0.25),
                stroke,
            ));
            let arc: Vec<egui::Pos2> = (0..=12)
                .map(|i| {
                    let t = std::f32::consts::PI * i as f32 / 12.0;
                    p(3.0 + 6.0 * t.cos(), 2.5 + 1.8 * t.sin())
                })
                .collect();
            line(&arc);
        }
    }
}

/// The input pin of a row: round for a single value, which takes one wire, a new one in
/// place of the old; a tall rounded rectangle for a collection, which takes any number of
/// wires, like Blender's multi-input sockets.
struct BlockPin {
    info: PinInfo,
    collection: bool,
    /// A category link to the collection leaves out items: drawn lighter.
    partial: bool,
}

impl BlockPin {
    fn single(info: PinInfo) -> Self {
        Self {
            info,
            collection: false,
            partial: false,
        }
    }
}

impl egui_snarl::ui::SnarlPin for BlockPin {
    fn pin_rect(&self, x: f32, y0: f32, y1: f32, size: f32) -> egui::Rect {
        let size = if self.collection {
            egui::vec2(size * 0.7, size * 1.4)
        } else {
            egui::vec2(size, size)
        };
        egui::Rect::from_center_size(egui::pos2(x, (y0 + y1) * 0.5), size)
    }

    fn draw(
        self,
        snarl_style: &SnarlStyle,
        style: &egui::Style,
        rect: egui::Rect,
        painter: &egui::Painter,
    ) -> egui_snarl::ui::PinWireInfo {
        if !self.collection {
            return self.info.draw(snarl_style, style, rect, painter);
        }
        let fill = self.info.get_fill(snarl_style, style);
        let stroke = self.info.get_stroke(snarl_style, style);
        let hovered = (painter.ctx().pointer_hover_pos()).is_some_and(|p| rect.contains(p));
        let shown = if hovered || self.partial {
            fill.lerp_to_gamma(egui::Color32::WHITE, 0.45)
        } else {
            fill
        };
        painter.rect(
            rect,
            rect.width() * 0.5,
            shown,
            stroke,
            egui::StrokeKind::Middle,
        );
        if self.partial {
            // A dot: not all items of the category come through the link.
            painter.circle_filled(rect.center(), rect.width() * 0.18, fill);
        }
        egui_snarl::ui::PinWireInfo {
            color: self.info.wire_color.unwrap_or(fill),
            style: (self.info.wire_style).unwrap_or(egui_snarl::ui::WireStyle::Bezier5),
        }
    }
}

/// The pin of a row that takes or gives no link: there, but not to be seen.
fn hidden_pin() -> PinInfo {
    PinInfo::circle()
        .with_fill(egui::Color32::TRANSPARENT)
        .with_stroke(egui::Stroke::NONE)
}

/// The context menu of an analysis' block.
fn node_menu(
    ui: &mut egui::Ui,
    id: u64,
    kind: Kind,
    steps: &[String],
    continues: Option<&str>,
    actions: &mut Vec<Action>,
) {
    let mut entry = |ui: &mut egui::Ui, text: &str, action: Action| {
        if ui.button(text).clicked() {
            actions.push(action);
            ui.close();
        }
    };
    if kind.is_source() {
        // Sources have no steps and are not solved.
        entry(ui, "Edit …", Action::Open(id, None));
        if kind == Kind::Mesh {
            entry(ui, "Generate Mesh", Action::Generate(id));
        }
        ui.separator();
        entry(ui, "Rename …", Action::Rename(id));
        entry(ui, "Duplicate", Action::Duplicate(id));
        entry(ui, "Delete …", Action::Delete(id));
        return;
    }
    entry(ui, "Open", Action::Open(id, None));
    entry(ui, "Solve", Action::Solve(id));
    ui.separator();
    let split = steps.len() > 1;
    ui.add_enabled_ui(split, |ui| {
        ui.menu_button("Split After", |ui| {
            for step in &steps[..steps.len().saturating_sub(1)] {
                entry(ui, step, Action::Split(id, step.clone()));
            }
        })
        .response
        .on_hover_text(
            "The steps after the one picked go to a new analysis that continues this one \
             from the state after it.",
        );
    });
    if let Some(source) = continues {
        entry(ui, &format!("Merge into {source}"), Action::Merge(id));
    }
    ui.separator();
    entry(ui, "Rename …", Action::Rename(id));
    entry(ui, "Duplicate", Action::Duplicate(id));
    entry(ui, "Delete …", Action::Delete(id));
}

/// Spacing of the dots of the schematic's background.
const DOT_SPACING: f32 = 24.0;

/// The zoom of the schematic: at most its true size, at least a third of it.
const MAX_SCALE: f32 = 1.0;
const MIN_SCALE: f32 = 0.3;

fn schematic_style() -> SnarlStyle {
    let border = egui::Stroke::new(1.0, crate::style::border());
    SnarlStyle {
        collapsible: Some(false),
        // A double-click into empty space shows the blocks to add instead.
        centering: Some(false),
        // Blocks with rounded corners and a tinted header, like the nodes of a node editor.
        node_frame: Some(
            egui::Frame::new()
                .fill(crate::style::window())
                .stroke(border)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(4, 3)),
        ),
        header_frame: Some(
            egui::Frame::new()
                .fill(crate::style::node_header())
                .corner_radius(egui::CornerRadius {
                    nw: 6,
                    ne: 6,
                    sw: 0,
                    se: 0,
                })
                .inner_margin(egui::Margin::symmetric(6, 4)),
        ),
        bg_frame: Some(egui::Frame::new().fill(crate::style::control())),
        bg_pattern: Some(egui_snarl::ui::BackgroundPattern::NoPattern),
        pin_size: Some(9.0),
        pin_stroke: Some(egui::Stroke::new(1.0, crate::style::pin_edge())),
        wire_width: Some(2.0),
        // Blocks are never magnified beyond their size, also when the view centres on them.
        max_scale: Some(MAX_SCALE),
        min_scale: Some(MIN_SCALE),
        // A row per pin, all of the same height, so that the rows line up with their pins.
        node_layout: Some(egui_snarl::ui::NodeLayout {
            kind: egui_snarl::ui::NodeLayoutKind::Coil,
            min_pin_row_height: ROW_HEIGHT,
            equal_pin_row_heights: true,
        }),
        ..SnarlStyle::new()
    }
}

/// Tooltip of Add Materials.
const ANALYSIS_TIP: &str = "Adds an analysis: a model of its own with geometry, mesh, setup \
     and steps, solved with CalculiX. Double-click its block to open it in its own window.";

const MATERIALS_TIP: &str = "Adds a block of materials kept in the project: analyses link \
     them one by one or all at once, so that they are defined in one place.";

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
    fn a_materials_block_keeps_its_materials_and_gives_them_as_setup() {
        let set = MaterialSet {
            id: 4,
            name: "Materials".into(),
            position: [5.0, 6.0],
            units: plx_model::UnitSystem::MKgSC,
            materials: vec![plx_model::Material {
                name: "Steel".into(),
                density: Some(7850.0),
                ..Default::default()
            }],
        };
        let mut node = Node::read(ProjectNode::Materials(set.clone()));
        assert_eq!(node.kind, Kind::Materials);
        assert!(node.status.is_none(), "a source is not solved");
        // Analyses link its materials like those of another analysis.
        let data = node.link_data().unwrap();
        let path = vec![links::SETUP.to_owned(), "Materials".to_owned()];
        assert!(links::exists(data, &path));
        let mut steel = path.clone();
        steel.push("Steel".into());
        assert!(links::exists(data, &steel));
        assert!(links::compatible(&steel, &steel) && links::compatible(&path, &path));
        let mut target = plx_model::FeModel::default();
        links::apply_setup(&mut target, data.fe, &steel);
        assert_eq!(target.materials, set.materials);
        let rows = material_rows(data.fe, &node.expanded);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.output && !r.input));
        assert_eq!(rows[1].path, steel);
        match node.written([5.0, 6.0]) {
            Some(ProjectNode::Materials(written)) => assert_eq!(written, set),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_mesh_block_is_out_of_date_once_its_parameters_change() {
        let block = MeshBlock {
            id: 2,
            name: "Box".into(),
            position: [0.0; 2],
            space: ModelSpace::ThreeD,
            units: plx_model::UnitSystem::MmTonSC,
            primitive: MeshPrimitive::default_box(),
            mesh: Default::default(),
            generated: None,
        };
        let mut node = Node::read(ProjectNode::Mesh(Box::new(block)));
        assert_eq!(node.kind, Kind::Mesh);
        assert_eq!(node.mesh_state(), CellState::Missing);
        assert_eq!(node.generate(), Ok(40));
        assert_eq!(node.mesh_state(), CellState::UpToDate);
        let data = node.link_data().unwrap();
        let mesh = vec![links::MESH.to_owned()];
        assert!(links::exists(data, &mesh));
        assert_eq!(links::MeshCopy::of(data).mesh.node_sets.len(), 6);
        let rows = mesh_rows(&node);
        assert!(rows[0].output && !rows[0].input);
        node.primitive = Some(MeshPrimitive::Box {
            origin: [0.0; 3],
            size: [0.1, 0.02, 0.02],
            divisions: [20, 2, 2],
            element: "C3D8I".into(),
        });
        assert_eq!(node.mesh_state(), CellState::OutOfDate);
        match node.written([1.0, 2.0]) {
            Some(ProjectNode::Mesh(written)) => {
                assert_eq!(written.mesh.element_count(), 40);
                assert_eq!(written.primitive, node.primitive.clone().unwrap());
                assert_eq!(written.position, [1.0, 2.0]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(node.generate(), Ok(80));
        assert_eq!(node.mesh_state(), CellState::UpToDate);
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
