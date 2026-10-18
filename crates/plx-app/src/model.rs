use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glam::{DAffine3, DVec3, Vec3};
use plx_io::frd::{FrdImport, read_frd};
use plx_io::inp::{InpImport, read_inp};
use plx_mesh::{ElementFamily, ElementId, FeMesh, NodeId, PartSkin, extract_part_skin};
use plx_mesher::{CadEntity, GeometryDisplay};
#[cfg(test)]
use plx_model::SectionKind;
use plx_model::{FeModel, Finding, Geometry, MeshCheck, UnitSystem};
use plx_render::{
    ClipPlane, RenderMesh, SectionCells, SectionValues, ShellThickness, Vertex, lighten,
    part_color, part_render_mesh, section_mesh, section_values, thick_shell_mesh, wireframe_edges,
};

use crate::exploded::{Assembly, Explosion, Parameters, PartShape};
use crate::results::ResultsView;

/// Angle between neighbouring faces above which their common edge counts as a feature edge
/// and the shading across it stays sharp.
const FEATURE_ANGLE_DEG: f64 = 30.0;
/// Within a surface patch, coarse meshes of curved surfaces can fold more than the feature
/// angle between neighbouring faces; shading still blends across such folds.
const SMOOTH_ANGLE_DEG: f64 = 60.0;

/// Extension of prepolix project files.
pub const PROJECT_EXTENSION: &str = "plx";

/// Whether a file is a prepolix project, by its extension.
pub fn is_project_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(PROJECT_EXTENSION))
}

/// Colour of selected faces and nodes, PrePoMax's highlight red.
pub const HIGHLIGHT_COLOR: [f32; 3] = [1.0, 0.0, 0.0];

/// Colour of the second region of an item, such as the slave surface of a contact pair:
/// PrePoMax's secondary highlight colour, violet.
pub const SECONDARY_HIGHLIGHT_COLOR: [f32; 3] = [238.0 / 255.0, 130.0 / 255.0, 238.0 / 255.0];

/// Summary of one part, computed once on load so the GUI never iterates large meshes.
pub struct PartInfo {
    pub name: String,
    pub color: [f32; 3],
    pub element_count: usize,
    pub node_count: usize,
    pub element_types: Vec<(String, usize)>,
    /// Bounding box relative to [`Model::origin`].
    pub bounds: Option<(Vec3, Vec3)>,
    pub visible: bool,
    /// Whether the part takes part in the colour scale of results (PrePoMax: colour
    /// contours on); a part without them is plain grey and leaves the legend range alone.
    pub contours: bool,
    /// Dimension of the CAD shape of a geometry part: 3 solid, 2 face, 1 edge.
    pub dimension: Option<u8>,
}

/// A loaded input file with everything the GUI shows about it.
pub struct Model {
    pub path: PathBuf,
    pub mesh: FeMesh,
    pub parts: Vec<PartInfo>,
    pub warnings: Vec<String>,
    pub skipped_keywords: BTreeMap<String, usize>,
    pub included_files: usize,
    pub load_time: Duration,
    /// Results read from an `.frd` file, with what the user currently looks at. A model
    /// with results is a results file: it lives in the Results workspace and has no FE model
    /// to set up, as in PrePoMax.
    pub results: Option<ResultsView>,
    /// The analysis set up on this mesh.
    pub fe: FeModel,
    /// CAD geometry the mesh is generated from.
    pub geometry: Option<Geometry>,
    /// Results of the analysis bound to the project, saved with it.
    pub bound_results: Option<plx_io::project::BoundResults>,
    /// Fingerprint of the meshing inputs of each part of the geometry when it was last
    /// meshed; a part whose inputs differ now has an out-of-date mesh.
    pub mesh_inputs: BTreeMap<String, u64>,
    /// Where the bound results were written to be opened, once they were.
    pub bound_results_file: Option<PathBuf>,
    /// The display of CAD geometry rather than a mesh: element edges are not drawn.
    is_geometry: bool,
    /// For the display of CAD geometry, the CAD face or edge of each element; element ids
    /// count from 1.
    cad_entities: Vec<CadEntity>,
    /// Solids of the CAD geometry a geometry display shows.
    pub geometry_solids: usize,
    /// Hot spots defined on a results file, with their values.
    pub hot_spots: crate::hot_spots::HotSpots,
    /// Plots of result paths shown in the 3D view.
    pub path_plots: Vec<crate::path_plot::PathPlot>,
    /// Faces and parts drawn in the highlight colour.
    pub highlight: Highlight,
    /// The section view plane in render coordinates; picking ignores what it cuts off.
    pub clip: Option<ClipPlane>,
    /// Centre of the mesh; render positions are relative to it.
    origin: DVec3,
    skins: Vec<PartSkin>,
    /// Elements of each part prepared for section cuts, built when first needed.
    section_cells: std::sync::OnceLock<Vec<SectionCells>>,
    /// The exploded view: parts drawn and picked moved apart.
    pub explosion: Explosion,
    /// What the query tool left in the view; they refer to the mesh by ids, so a new mesh
    /// drops them.
    pub query_annotations: Vec<crate::query::Annotation>,
    /// Offsets of single nodes by index, the motion preview of a boundary condition dialog:
    /// the nodes are drawn moved, the rest of the mesh stays put.
    pub motion: Option<Vec<(usize, [f64; 3])>>,
    /// Node indices of each part and the part of each node, built when first needed.
    part_nodes: std::sync::OnceLock<PartNodes>,
    /// Lookups to the CAD entities of a mesh generated from geometry, built when first
    /// needed.
    cad_index: std::sync::OnceLock<crate::cad_selection::CadIndex>,
    /// The model checks of the last FE model checked, see [`Model::findings`].
    checks: std::sync::Mutex<Option<Checks>>,
    /// What the user changed, to undo and redo it.
    pub history: crate::undo::History,
    /// Counts the changes of the FE model, the meshing settings and the mesh, see
    /// [`crate::undo::record`]; the project brings its links up to date when it changes.
    pub revision: u64,
    /// Shell and membrane sections whose elements are drawn as the solid of their
    /// thickness and offset, see [`Model::shell_sections`]; empty draws them as surfaces.
    pub thick_shells: Vec<plx_model::Section>,
}

/// Findings of the model checks with what they were computed for.
struct Checks {
    mesh: MeshCheck,
    fe: FeModel,
    findings: Vec<Finding>,
}

/// Which nodes belong to which part; a node shared by parts belongs to the first of them.
struct PartNodes {
    nodes: Vec<Vec<usize>>,
    part_of: Vec<u32>,
}

impl PartNodes {
    fn new(mesh: &FeMesh) -> Self {
        let mut part_of = vec![u32::MAX; mesh.node_count()];
        let nodes = (mesh.parts.iter().enumerate())
            .map(|(index, part)| {
                let mut nodes: Vec<usize> = (part.elements.iter())
                    .filter_map(|&id| mesh.element(id))
                    .flat_map(|e| e.nodes.iter())
                    .filter_map(|&id| mesh.node_index(id))
                    .collect();
                nodes.sort_unstable();
                nodes.dedup();
                for &node in &nodes {
                    if part_of[node] == u32::MAX {
                        part_of[node] = index as u32;
                    }
                }
                nodes
            })
            .collect();
        Self { nodes, part_of }
    }
}

/// What is shown selected: whole parts, element faces and nodes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Highlight {
    pub parts: HashSet<usize>,
    /// Parts drawn with a red outline only, as PrePoMax shows a selected part.
    pub outlines: HashSet<usize>,
    /// Element faces as (element, CalculiX face number).
    pub faces: HashSet<(ElementId, u8)>,
    /// Element faces in the secondary highlight colour, e.g. slave surfaces.
    pub secondary_faces: HashSet<(ElementId, u8)>,
    /// Nodes, drawn as points over the scene.
    pub nodes: Vec<NodeId>,
    /// Lines between two nodes drawn over the scene, such as the edges of 2D elements.
    pub lines: Vec<[NodeId; 2]>,
    /// Lines in the secondary highlight colour, e.g. the slave edges of a 2D contact.
    pub secondary_lines: Vec<[NodeId; 2]>,
}

impl Highlight {
    /// Whole parts, such as those selected in the tree or clicked in the 3D view.
    pub fn parts(parts: impl IntoIterator<Item = usize>) -> Self {
        Self {
            outlines: parts.into_iter().collect(),
            ..Self::default()
        }
    }
}

/// A visible face or line under the mouse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub part: usize,
    /// Index into the part's skin faces, or into its skin lines when `line` is set.
    pub face: usize,
    /// Hit point relative to the model origin.
    pub point: Vec3,
    /// A segment of a line element (beam, truss, CAD edge) was hit instead of a face.
    pub line: bool,
}

/// Node coordinates as drawn, normalized contour values and whether the shape is deformed.
type ShownState<'a> = (std::borrow::Cow<'a, [[f64; 3]]>, Option<Vec<f32>>, bool);

/// Result of loading on a worker thread: the model plus one GPU-ready mesh per part of what
/// is shown first, the geometry if the model has no mesh yet.
pub struct LoadedModel {
    pub model: Model,
    pub render_meshes: Vec<RenderMesh>,
    /// Display of the model's CAD geometry.
    pub geometry_view: Option<Model>,
}

/// Imports one or several CAD files into one geometry, in the length unit of `units`. The
/// model takes the path of the first file.
pub fn load_geometry(paths: &[PathBuf], units: UnitSystem) -> Result<LoadedModel, String> {
    let start = Instant::now();
    let path = paths.first().ok_or("No file selected")?;
    let import = plx_mesher::import_cad_files(paths, units).map_err(|e| e.to_string())?;
    let mut model = Model::new(path, FeMesh::default());
    model.fe.properties.units = units;
    model.geometry = Some(import.geometry);
    model.warnings = import.warnings;
    let view = Model::geometry_view(path, import.display);
    let render_meshes = view.render_meshes();
    model.load_time = start.elapsed();
    Ok(LoadedModel {
        model,
        render_meshes,
        geometry_view: Some(view),
    })
}

/// The input file CalculiX ran for a results file: named like it, next to it.
fn companion_input(results: &Path) -> Option<PathBuf> {
    ["inp", "INP"]
        .map(|e| results.with_extension(e))
        .into_iter()
        .find(|p| p.is_file())
}

/// Groups the elements of results into the parts of the input file they were computed from,
/// with the part names as the input writes them; returns whether the input defines contact.
fn adopt_input_parts(mesh: &mut FeMesh, input: &Path) -> bool {
    match read_inp(input) {
        Ok(import) => {
            let parts: Vec<_> = (import.mesh.parts.into_iter())
                .map(|mut part| {
                    if let Some(name) = import.part_names.get(&part.name) {
                        part.name = name.clone();
                    }
                    part
                })
                .collect();
            mesh.adopt_parts(&parts);
            ["CONTACTPAIR", "CONTACTINCLUSION"]
                .iter()
                .any(|k| import.skipped_keywords.contains_key(*k))
        }
        Err(_) => plx_io::inp::defines_contact(input).unwrap_or(false),
    }
}

/// Loads an input, results or CAD file for a model shown in `units`: the mesh of an input file
/// and CAD geometry are stored in SI units, see [`plx_model::STORED`], and results are
/// converted from them into `units`. Project files hold several analyses and are
/// opened by the project window.
pub fn load(path: &Path, units: UnitSystem) -> Result<LoadedModel, String> {
    let start = Instant::now();
    let extension = |e: &str| path.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
    if plx_mesher::is_cad_file(path) {
        return load_geometry(&[path.to_path_buf()], units);
    }
    if extension(PROJECT_EXTENSION) {
        return Err(format!(
            "{} is a project; it opens in the project window",
            path.display()
        ));
    }
    let mut fe = FeModel::default();
    let mut contact = None;
    let (mesh, warnings, skipped_keywords, included_files, increments) = if extension("frd") {
        let FrdImport {
            mut mesh,
            mut increments,
            warnings,
            date,
            time,
            ..
        } = read_frd(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // CalculiX ran in the stored SI units; the results show in the analysis' units.
        crate::results::convert_results(&mut mesh, &mut increments, units);
        // The results group the elements by material; the input it ran, named like the
        // results, knows the parts.
        contact = companion_input(path).map(|input| adopt_input_parts(&mut mesh, &input));
        (
            mesh,
            warnings,
            BTreeMap::new(),
            0,
            Some((increments, date, time)),
        )
    } else {
        let InpImport {
            mut mesh,
            warnings,
            skipped_keywords,
            files,
            ..
        } = read_inp(path).map_err(|e| e.to_string())?;
        // The numbers of an input file are in the analysis' units; the mesh is stored in SI
        // units.
        let length = units.factor_to(plx_model::Quantity::Length, plx_model::STORED);
        if length != 1.0 {
            mesh.scale(length);
        }
        // Plane stress, plane strain or axisymmetric elements make a 2D model.
        fe.properties.space = plx_model::ModelSpace::of_mesh(&mesh);
        let included = files.len().saturating_sub(1);
        (mesh, warnings, skipped_keywords, included, None)
    };
    if mesh.element_count() == 0 {
        return Err(format!(
            "{} contains no elements that can be displayed",
            path.display()
        ));
    }
    let mut model = Model::new(path, mesh);
    model.fe = fe;
    model.warnings = warnings;
    model.skipped_keywords = skipped_keywords;
    model.included_files = included_files;
    model.results = increments.map(|(increments, date, time)| {
        let mut view = ResultsView::new(increments, model.mesh.bounds());
        // CalculiX writes contact into the results only on request; the input it ran tells.
        if let Some(contact) = contact {
            view.contact |= contact;
            view.follow_default_deformation();
        }
        view.date = date;
        view.time = time;
        view
    });
    // History outputs CalculiX printed next to the results, as PrePoMax reads them.
    if let Some(view) = &mut model.results {
        let dat = ["dat", "DAT"]
            .map(|e| path.with_extension(e))
            .into_iter()
            .find(|p| p.is_file());
        if let Some(dat) = dat {
            match plx_io::dat::read_dat(&dat) {
                Ok(mut import) => {
                    crate::results::convert_history(&mut import.sets, units);
                    let warnings = view.set_file_history(import.sets, &model.mesh);
                    model.warnings.extend(import.warnings);
                    model.warnings.extend(warnings);
                }
                Err(error) => model.warnings.push(format!("{}: {error}", dat.display())),
            }
        }
    }
    let render_meshes = model.render_meshes();
    for (part, render) in model.parts.iter_mut().zip(&render_meshes) {
        part.bounds = render.bounds();
    }
    model.load_time = start.elapsed();
    Ok(LoadedModel {
        model,
        render_meshes,
        geometry_view: None,
    })
}

/// Builds the model of an analysis of a project; `path` is the project file. An analysis
/// without a mesh opens on its geometry, if it has one.
pub fn load_analysis(path: &Path, analysis: plx_io::project::ProjectAnalysis) -> LoadedModel {
    let start = Instant::now();
    let plx_io::project::ProjectAnalysis {
        geometry,
        mesh,
        model: fe,
        mesh_inputs,
        results,
        ..
    } = analysis;
    let mut warnings = Vec::new();
    // The geometry is shown on the Geometry tab; without Gmsh only the mesh is.
    let geometry_view = match &geometry {
        Some(geometry) => match plx_mesher::tessellate(geometry) {
            Ok(display) => Some(Model::geometry_view(path, display)),
            Err(error) => {
                warnings.push(format!("Geometry cannot be displayed: {error}"));
                None
            }
        },
        None => None,
    };
    let mut model = Model::new(path, mesh);
    model.fe = fe;
    model.geometry = geometry;
    model.bound_results = results;
    model.mesh_inputs = mesh_inputs;
    model.warnings = warnings;
    let render_meshes = match &geometry_view {
        Some(view) if model.mesh.element_count() == 0 => view.render_meshes(),
        _ => model.render_meshes(),
    };
    for (part, render) in model.parts.iter_mut().zip(&render_meshes) {
        part.bounds = render.bounds();
    }
    model.load_time = start.elapsed();
    LoadedModel {
        model,
        render_meshes,
        geometry_view,
    }
}

impl Model {
    /// What the user defined on these results, for results bound to a project.
    pub fn results_settings(&self) -> Option<plx_results::ResultsSettings> {
        let view = self.results.as_ref()?;
        Some(plx_results::ResultsSettings {
            hot_spots: self.hot_spots.definitions.clone(),
            reference_points: self.fe.reference_points.clone(),
            coordinate_systems: self.fe.coordinate_systems.clone(),
            planes: self.fe.planes.clone(),
            result_paths: self.fe.result_paths.clone(),
            result_planes: self.fe.result_planes.clone(),
            plain_part_names: (self.parts.iter())
                .filter(|part| !part.contours)
                .map(|part| part.name.clone())
                .collect(),
            mesh: Some(self.mesh.fingerprint()),
            ..view.settings()
        })
        .map(|mut settings| {
            let saved = &mut settings.view;
            saved.hidden_parts = (self.parts.iter())
                .filter(|p| !p.visible)
                .map(|p| p.name.clone())
                .collect();
            saved.annotations = self.query_annotations.iter().map(Into::into).collect();
            saved.path_plots = self.path_plots.iter().map(|p| p.saved()).collect();
            settings
        })
    }

    /// Takes saved settings of bound results; returns the outputs that failed. History
    /// outputs may put the positions of `moving_loads`, those of the FE model, beside their
    /// values.
    pub fn restore_results_settings(
        &mut self,
        settings: &plx_results::ResultsSettings,
        moving_loads: Vec<plx_model::LoadStations>,
    ) -> Vec<String> {
        self.fe.reference_points = settings.reference_points.clone();
        self.fe.coordinate_systems = settings.coordinate_systems.clone();
        self.fe.planes = settings.planes.clone();
        self.fe.result_paths = settings.result_paths.clone();
        self.fe.result_planes = settings.result_planes.clone();
        self.hot_spots.definitions = settings.hot_spots.clone();
        let view = &settings.view;
        for part in &mut self.parts {
            part.visible = !view.hidden_parts.contains(&part.name);
        }
        self.query_annotations = view.annotations.iter().cloned().map(Into::into).collect();
        self.path_plots = (view.path_plots.iter())
            .map(crate::path_plot::PathPlot::from_saved)
            .collect();
        let plain: Vec<usize> = if settings.plain_part_names.is_empty()
            && settings.mesh == Some(self.mesh.fingerprint())
        {
            settings.plain_parts.clone()
        } else {
            (self.parts.iter().enumerate())
                .filter(|(_, part)| settings.plain_part_names.contains(&part.name))
                .map(|(index, _)| index)
                .collect()
        };
        self.set_contours(&plain, false);
        // History outputs of hot spots need their values first.
        let reports = crate::hot_spots::evaluate(self);
        match &mut self.results {
            Some(view) => {
                view.hot_spot_reports = reports;
                view.moving_loads = moving_loads;
                view.restore(settings, &self.mesh)
            }
            None => Vec::new(),
        }
    }

    /// A model of the mesh with summaries and skins of its parts.
    pub fn new(path: &Path, mesh: FeMesh) -> Self {
        let skins = (mesh.parts.iter())
            .map(|part| extract_part_skin(&mesh, part, FEATURE_ANGLE_DEG))
            .collect();
        Self::with_skins(path, mesh, skins)
    }

    fn with_skins(path: &Path, mesh: FeMesh, skins: Vec<PartSkin>) -> Self {
        let origin = mesh.bounds().map_or(DVec3::ZERO, |(min, max)| {
            (DVec3::from(min) + DVec3::from(max)) * 0.5
        });
        let mut parts = Vec::with_capacity(mesh.parts.len());
        for (index, part) in mesh.parts.iter().enumerate() {
            let mut types: BTreeMap<&str, usize> = BTreeMap::new();
            let mut nodes = std::collections::HashSet::new();
            for element in part.elements.iter().filter_map(|&id| mesh.element(id)) {
                *types.entry(&element.type_name).or_default() += 1;
                nodes.extend(element.nodes.iter().copied());
            }
            parts.push(PartInfo {
                name: part.name.clone(),
                color: part_color(index),
                element_count: part.elements.len(),
                node_count: nodes.len(),
                element_types: types.into_iter().map(|(t, n)| (t.to_string(), n)).collect(),
                bounds: None,
                visible: true,
                contours: true,
                dimension: None,
            });
        }
        let mut model = Self {
            path: path.to_path_buf(),
            mesh,
            parts,
            warnings: Vec::new(),
            skipped_keywords: BTreeMap::new(),
            included_files: 0,
            load_time: Duration::ZERO,
            results: None,
            fe: FeModel::default(),
            geometry: None,
            bound_results: None,
            mesh_inputs: BTreeMap::new(),
            bound_results_file: None,
            is_geometry: false,
            cad_entities: Vec::new(),
            geometry_solids: 0,
            hot_spots: Default::default(),
            path_plots: Vec::new(),
            highlight: Highlight::default(),
            clip: None,
            origin,
            skins,
            section_cells: Default::default(),
            explosion: Explosion::default(),
            query_annotations: Vec::new(),
            motion: None,
            part_nodes: Default::default(),
            checks: Default::default(),
            cad_index: Default::default(),
            history: Default::default(),
            revision: 0,
            thick_shells: Vec::new(),
        };
        let meshes = model.render_meshes();
        for (part, render) in model.parts.iter_mut().zip(&meshes) {
            part.bounds = render.bounds();
        }
        model
    }

    /// The CAD geometry for the Geometry tab: its faces shaded each on its own, outlined by
    /// its edges, without the triangles of the display mesh.
    pub fn geometry_view(path: &Path, display: GeometryDisplay) -> Self {
        let mesh = display.mesh;
        let skins = (mesh.parts.iter())
            .map(|part| {
                // Only the CAD edges, drawn as line elements, outline the geometry.
                let mut skin = extract_part_skin(&mesh, part, 180.0);
                for face in &mut skin.faces {
                    let id = mesh.elements()[face.element].id;
                    if let Some(CadEntity::Face(tag)) = display.entities.get(id as usize - 1) {
                        face.region = *tag as usize;
                    }
                }
                skin
            })
            .collect();
        let mut model = Self::with_skins(path, mesh, skins);
        model.is_geometry = true;
        for (part, dimension) in model.parts.iter_mut().zip(display.part_dimensions) {
            part.dimension = Some(dimension);
        }
        model.cad_entities = display.entities;
        model.geometry_solids = display.solids;
        model
    }

    /// The CAD face or edge an element of the geometry display shows.
    pub fn cad_entity(&self, element: ElementId) -> Option<CadEntity> {
        let index = usize::try_from(element).ok()?.checked_sub(1)?;
        self.cad_entities.get(index).copied()
    }

    /// Changes the mesh in place, e.g. by a mesh tool, and rebuilds what the GUI derives
    /// from it; the parts keep their colours and visibility.
    pub fn edit_mesh(&mut self, edit: impl FnOnce(&mut FeMesh, &mut FeModel)) {
        // A copy if the history follows the model, so that it keeps the mesh as it was.
        let mut mesh = if self.history.follows() {
            self.mesh.clone()
        } else {
            std::mem::take(&mut self.mesh)
        };
        edit(&mut mesh, &mut self.fe);
        let kept: Vec<([f32; 3], bool)> = self.parts.iter().map(|p| (p.color, p.visible)).collect();
        self.set_mesh(mesh);
        for (part, (color, visible)) in self.parts.iter_mut().zip(kept) {
            part.color = color;
            part.visible = visible;
        }
    }

    /// Replaces the mesh, e.g. by a newly generated one; the FE model stays.
    pub fn set_mesh(&mut self, mut mesh: FeMesh) {
        // The user's sets are found on the new mesh by their selections, and its parts get
        // the element types chosen for them, also when they were meshed anew.
        self.fe.write_mesh_sets(&mut mesh);
        self.fe.apply_element_types(&mut mesh);
        let mut fresh = Self::new(&self.path, mesh);
        std::mem::swap(&mut self.mesh, &mut fresh.mesh);
        self.history.mesh_replaced(std::mem::take(&mut fresh.mesh));
        self.revision += 1;
        self.parts = fresh.parts;
        self.skins = fresh.skins;
        self.origin = fresh.origin;
        self.highlight = Highlight::default();
        self.section_cells = Default::default();
        self.part_nodes = Default::default();
        self.checks = Default::default();
        self.cad_index = Default::default();
        self.query_annotations.clear();
        // Reference points at the centre of a selection follow the new mesh.
        self.fe.update_reference_points(&self.mesh);
        // The exploded view stays on and is laid out anew for the new mesh.
        self.explosion.clear_layout();
        if let Some(parameters) = self.explosion.applied.clone() {
            let offsets = self.explosion_offsets(&parameters);
            self.explosion.show(offsets, false);
        } else {
            self.explosion.show(Vec::new(), false);
        }
    }

    /// Problems of the FE model that make CalculiX abort or give useless results. The checks
    /// run again when the FE model changed; the part that depends on the mesh only once per
    /// mesh.
    pub fn findings(&self) -> Vec<Finding> {
        let mut cache = self.checks.lock().unwrap_or_else(|e| e.into_inner());
        let space = self.fe.properties.space;
        if let Some(checks) = cache.as_ref()
            && checks.fe == self.fe
            && checks.mesh.is_current(&self.mesh, space)
        {
            return checks.findings.clone();
        }
        let mesh = match cache.take() {
            Some(checks) if checks.mesh.is_current(&self.mesh, space) => checks.mesh,
            _ => MeshCheck::new(&self.mesh, space),
        };
        let findings = self.fe.check(&self.mesh, &mesh);
        *cache = Some(Checks {
            mesh,
            fe: self.fe.clone(),
            findings: findings.clone(),
        });
        findings
    }

    pub fn cad_index(&self) -> &crate::cad_selection::CadIndex {
        (self.cad_index).get_or_init(|| crate::cad_selection::CadIndex::new(&self.mesh))
    }

    /// Whether the mesh was generated from CAD geometry and knows where its entities lie.
    pub fn has_cad(&self) -> bool {
        !self.mesh.cad.is_empty()
    }

    fn part_nodes(&self) -> &PartNodes {
        self.part_nodes.get_or_init(|| PartNodes::new(&self.mesh))
    }

    /// The offsets of all parts for an exploded view; the layout is kept until parameters
    /// it depends on change.
    pub fn explosion_offsets(&mut self, parameters: &Parameters) -> Vec<glam::DVec3> {
        self.explosion_layout(parameters)
            .offsets(parameters.scale(), parameters.sequential)
    }

    /// The layout of an exploded view, computed when first needed.
    pub fn explosion_layout(&mut self, parameters: &Parameters) -> &crate::exploded::Layout {
        let part_nodes = self.part_nodes.get_or_init(|| PartNodes::new(&self.mesh));
        let assembly = Assembly {
            mesh: &self.mesh,
            parts: (self.mesh.parts.iter().enumerate())
                .map(|(i, part)| PartShape {
                    name: &part.name,
                    nodes: &part_nodes.nodes[i],
                    skin: &self.skins[i],
                })
                .collect(),
        };
        self.explosion.layout(&assembly, parameters)
    }

    /// How far a node is moved by the exploded view.
    pub fn explosion_offset(&self, index: usize) -> DVec3 {
        let offsets = self.explosion.offsets();
        if offsets.is_empty() {
            return DVec3::ZERO;
        }
        let part = self
            .part_nodes()
            .part_of
            .get(index)
            .copied()
            .unwrap_or(u32::MAX);
        offsets.get(part as usize).copied().unwrap_or(DVec3::ZERO)
    }

    /// Node coordinates moved by the exploded view, without any deformation.
    pub fn exploded_coords(&self) -> std::borrow::Cow<'_, [[f64; 3]]> {
        let coords = self.mesh.coords();
        if !self.explosion.is_shown() {
            return std::borrow::Cow::Borrowed(coords);
        }
        let offsets = self.explosion.offsets();
        let part_of = &self.part_nodes().part_of;
        std::borrow::Cow::Owned(
            (coords.iter().zip(part_of))
                .map(|(p, &part)| {
                    let o = offsets.get(part as usize).copied().unwrap_or(DVec3::ZERO);
                    [p[0] + o.x, p[1] + o.y, p[2] + o.z]
                })
                .collect(),
        )
    }

    /// Whether this shows CAD geometry rather than a mesh.
    pub fn is_geometry(&self) -> bool {
        self.is_geometry
    }

    /// Whether this is a results file rather than an FE model.
    pub fn is_results(&self) -> bool {
        self.results.is_some()
    }

    /// Node coordinates as drawn: moved by the exploded view and with the shown deformation
    /// of results.
    pub fn shown_coords(&self) -> std::borrow::Cow<'_, [[f64; 3]]> {
        self.deformed_coords().0
    }

    /// Node coordinates as drawn and whether a deformation is shown.
    fn deformed_coords(&self) -> (std::borrow::Cow<'_, [[f64; 3]]>, bool) {
        let mut coords = self.exploded_coords();
        if let Some(motion) = &self.motion {
            let moved = coords.to_mut();
            for &(node, offset) in motion {
                if let Some(p) = moved.get_mut(node) {
                    *p = [0, 1, 2].map(|k| p[k] + offset[k]);
                }
            }
        }
        let Some(view) = &self.results else {
            return (coords, false);
        };
        let scale = (view.scale() * view.amplitude()) as f64;
        match view.shown_displacements() {
            Some(displacements) if scale != 0.0 => (
                std::borrow::Cow::Owned(
                    coords
                        .iter()
                        .zip(displacements.iter())
                        .map(|(p, d)| [0, 1, 2].map(|k| p[k] + scale * d[k] as f64))
                        .collect(),
                ),
                true,
            ),
            _ => (coords, false),
        }
    }

    /// Turns the colour contours of parts on or off and refreshes the nodes the results leave
    /// out of the legend: those used by plain parts only.
    pub fn set_contours(&mut self, parts: &[usize], contours: bool) {
        for &part in parts {
            if let Some(info) = self.parts.get_mut(part) {
                info.contours = contours;
            }
        }
        self.update_plain_nodes();
    }

    fn update_plain_nodes(&mut self) {
        let Some(view) = &mut self.results else {
            return;
        };
        if self.parts.iter().all(|p| p.contours) {
            view.plain_nodes.clear();
            return;
        }
        let count = self.mesh.coords().len();
        let mut plain = vec![false; count];
        let mut coloured = vec![false; count];
        for (index, info) in self.parts.iter().enumerate() {
            let target = if info.contours {
                &mut coloured
            } else {
                &mut plain
            };
            for id in self.mesh.part_nodes(&[index]) {
                if let Some(node) = self.mesh.node_index(id) {
                    target[node] = true;
                }
            }
        }
        // A node shared with a part that has contours keeps its colour.
        view.plain_nodes = plain
            .into_iter()
            .zip(coloured)
            .map(|(plain, coloured)| plain && !coloured)
            .collect();
    }

    /// Node coordinates as drawn and the normalized contour values, if a result is shown;
    /// the flag tells whether the shape is deformed and the undeformed outline is wanted.
    fn shown_state(&self) -> ShownState<'_> {
        let (coords, is_deformed) = self.deformed_coords();
        let mut scalars = None;
        let mut deformed = false;
        if let Some(view) = &self.results {
            if is_deformed {
                deformed = view.show_undeformed;
            }
            if let (Some(values), Some(legend)) = (view.shown_values(), view.legend()) {
                scalars = Some(legend.normalize(&values));
            }
        }
        (coords, scalars, deformed)
    }

    /// The transformed copies of the results ([`ResultsView::transformations`]): per copy
    /// its transformation and its normalized contour values.
    fn transformed_copies(&self) -> Vec<(DAffine3, Option<Vec<f32>>)> {
        let Some(view) = &self.results else {
            return Vec::new();
        };
        let legend = view.legend();
        (view.instances().into_iter().skip(1))
            .map(|instance| {
                let scalars = (view.shown_values_on(&instance))
                    .zip(legend.as_ref())
                    .map(|(values, legend)| legend.normalize(&values));
                (instance, scalars)
            })
            .collect()
    }

    /// GPU-ready meshes of all parts, deformed and coloured by the selected result if any,
    /// with the transformed copies of the results.
    pub fn render_meshes(&self) -> Vec<RenderMesh> {
        let (coords, scalars, deformed) = self.shown_state();
        let mut meshes = self.part_meshes(&coords, scalars.as_deref(), deformed);
        for (instance, scalars) in self.transformed_copies() {
            let moved = transform_coords(&coords, &instance);
            let undeformed = deformed.then(|| transform_coords(&self.exploded_coords(), &instance));
            for ((mesh, info), skin) in meshes.iter_mut().zip(&self.parts).zip(&self.skins) {
                let mut copy = part_render_mesh(
                    &moved,
                    skin,
                    self.origin,
                    info.color,
                    |_| None,
                    SMOOTH_ANGLE_DEG,
                    scalars.as_deref(),
                );
                if let Some(undeformed) = &undeformed {
                    copy.wireframe_edges = wireframe_edges(undeformed, skin, self.origin);
                }
                mesh.append(copy);
            }
        }
        meshes
    }

    fn part_meshes(
        &self,
        coords: &[[f64; 3]],
        scalars: Option<&[f32]>,
        deformed: bool,
    ) -> Vec<RenderMesh> {
        let undeformed = if deformed {
            self.exploded_coords()
        } else {
            std::borrow::Cow::Borrowed(&[][..])
        };
        let shells = self.thick_shell_elements();
        self.mesh
            .parts
            .iter()
            .zip(&self.parts)
            .zip(&self.skins)
            .enumerate()
            .map(|(index, ((_, info), skin))| {
                let mut mesh = part_render_mesh(
                    coords,
                    skin,
                    self.origin,
                    info.color,
                    self.face_highlight(index, skin),
                    SMOOTH_ANGLE_DEG,
                    scalars,
                );
                self.highlight_outline(index, &mut mesh);
                if let Some(thick) = shells.as_ref() {
                    mesh.append(thick_shell_mesh(
                        coords,
                        skin,
                        self.origin,
                        info.color,
                        self.face_highlight(index, skin),
                        |face| thick.get(&skin.faces[face].element).copied(),
                        SMOOTH_ANGLE_DEG,
                    ));
                }
                if deformed {
                    mesh.wireframe_edges = wireframe_edges(&undeformed, skin, self.origin);
                }
                if self.is_geometry {
                    mesh.mesh_edges.clear();
                }
                mesh
            })
            .collect()
    }

    /// The shell and membrane sections of a 3D FE model, whose elements can be drawn as the
    /// solid they stand for.
    pub fn shell_sections(&self) -> impl Iterator<Item = &plx_model::Section> {
        let three_d = self.fe.properties.space == plx_model::ModelSpace::ThreeD;
        let model = !self.is_geometry && self.results.is_none();
        (self.fe.sections.iter())
            .filter(move |_| three_d && model)
            .filter(|s| {
                matches!(
                    s.kind,
                    plx_model::SectionKind::Shell { .. } | plx_model::SectionKind::Membrane { .. }
                )
            })
    }

    /// Thickness and offset of the surface elements of [`Model::thick_shells`] by element
    /// index; `None` when there are none.
    fn thick_shell_elements(&self) -> Option<HashMap<usize, ShellThickness>> {
        if self.thick_shells.is_empty() || self.is_geometry || self.results.is_some() {
            return None;
        }
        let mut elements = HashMap::new();
        for section in &self.thick_shells {
            let (plx_model::SectionKind::Shell { thickness, offset }
            | plx_model::SectionKind::Membrane { thickness, offset }) = section.kind
            else {
                continue;
            };
            let shell = ShellThickness { thickness, offset };
            for id in section.region.elements(&self.mesh) {
                if let Some(index) = self.mesh.element_index(id)
                    && self.mesh.elements()[index].shape.family() == ElementFamily::Surface
                {
                    elements.insert(index, shell);
                }
            }
        }
        (!elements.is_empty()).then_some(elements)
    }

    /// Section faces of all parts where a plane in model coordinates cuts them, deformed and
    /// coloured like the parts.
    pub fn section_meshes(
        &self,
        point: DVec3,
        normal: DVec3,
        lighten_colors: bool,
    ) -> Vec<RenderMesh> {
        let cells = self.section_cells.get_or_init(|| {
            self.mesh
                .parts
                .iter()
                .map(|part| SectionCells::new(&self.mesh, part))
                .collect()
        });
        let (coords, scalars, _) = self.shown_state();
        let color = |info: &PartInfo| {
            if lighten_colors {
                lighten(info.color)
            } else {
                info.color
            }
        };
        let mut meshes: Vec<RenderMesh> = cells
            .iter()
            .zip(&self.parts)
            .map(|(cells, info)| {
                let color = color(info);
                section_mesh(
                    cells,
                    &coords,
                    self.origin,
                    point,
                    normal,
                    color,
                    scalars.as_deref(),
                )
            })
            .collect();
        // The copies are cut by the same plane.
        for (instance, scalars) in self.transformed_copies() {
            let moved = transform_coords(&coords, &instance);
            for ((mesh, cells), info) in meshes.iter_mut().zip(cells).zip(&self.parts) {
                let color = color(info);
                let scalars = scalars.as_deref();
                mesh.append(section_mesh(
                    cells,
                    &moved,
                    self.origin,
                    point,
                    normal,
                    color,
                    scalars,
                ));
            }
        }
        meshes
    }

    /// The shown result values where a plane in model coordinates cuts the visible parts:
    /// of the parts as shown, or of the undeformed mesh, for values at true positions.
    pub fn section_values(
        &self,
        point: DVec3,
        normal: DVec3,
        shown: bool,
    ) -> Option<SectionValues> {
        let values = self.results.as_ref()?.shown_values()?;
        let cells = self.section_cells.get_or_init(|| {
            self.mesh
                .parts
                .iter()
                .map(|part| SectionCells::new(&self.mesh, part))
                .collect()
        });
        let coords = if shown {
            self.deformed_coords().0
        } else {
            std::borrow::Cow::Borrowed(self.mesh.coords())
        };
        (cells.iter().zip(&self.parts))
            .filter(|(_, info)| info.visible)
            .map(|(cells, _)| section_values(cells, &coords, point, normal, &values))
            .reduce(|mut all, part| {
                all.merge(part);
                all
            })
    }

    /// The model origin in global coordinates; render positions are relative to it.
    pub fn origin(&self) -> DVec3 {
        self.origin
    }

    /// Recolours the vertices of highlighted faces; vertices are laid out face by face.
    fn highlight_outline(&self, part: usize, mesh: &mut RenderMesh) {
        if self.highlight.outlines.contains(&part) {
            mesh.wide_edges = (mesh.feature_edges.iter())
                .map(|&vertex| Vertex {
                    color: HIGHLIGHT_COLOR,
                    ..vertex
                })
                .collect();
        }
    }

    /// The colour of each highlighted skin face of a part, by its index in the skin.
    fn face_highlight(&self, part: usize, skin: &PartSkin) -> impl Fn(usize) -> Option<[f32; 3]> {
        let whole = self.highlight.parts.contains(&part);
        let none = self.highlight.faces.is_empty() && self.highlight.secondary_faces.is_empty();
        let elements = self.mesh.elements();
        move |index| {
            if whole {
                return Some(HIGHLIGHT_COLOR);
            }
            if none {
                return None;
            }
            let face = &skin.faces[index];
            let key = (elements[face.element].id, face.face as u8 + 1);
            if self.highlight.faces.contains(&key) {
                Some(HIGHLIGHT_COLOR)
            } else {
                (self.highlight.secondary_faces.contains(&key)).then_some(SECONDARY_HIGHLIGHT_COLOR)
            }
        }
    }

    /// All element faces on the surface of the parts.
    pub fn skin_faces(&self) -> impl Iterator<Item = (ElementId, u8)> + '_ {
        let elements = self.mesh.elements();
        self.skins
            .iter()
            .flat_map(|skin| &skin.faces)
            .map(|f| (elements[f.element].id, f.face as u8 + 1))
    }

    /// All element faces on the surface with the ids of their corner nodes.
    pub fn skin_faces_with_corners(
        &self,
    ) -> impl Iterator<Item = ((ElementId, u8), Vec<NodeId>)> + '_ {
        let (elements, ids) = (self.mesh.elements(), self.mesh.node_ids());
        self.skins
            .iter()
            .flat_map(|skin| &skin.faces)
            .map(move |f| {
                let corners = f.corners.iter().map(|&n| ids[n]).collect();
                ((elements[f.element].id, f.face as u8 + 1), corners)
            })
    }

    pub fn skin(&self, part: usize) -> &PartSkin {
        &self.skins[part]
    }

    /// Skins of all parts, in the order of the mesh's parts.
    pub fn skins(&self) -> &[PartSkin] {
        &self.skins
    }

    /// Position of a node relative to the model origin as drawn, where picking happens.
    pub fn render_position(&self, index: usize) -> Vec3 {
        self.node_position(index).unwrap_or(Vec3::ZERO)
    }

    /// All nodes of the visible parts.
    pub fn visible_nodes(&self) -> BTreeSet<NodeId> {
        let elements = self.mesh.elements();
        self.parts
            .iter()
            .zip(&self.mesh.parts)
            .filter(|(info, _)| info.visible)
            .flat_map(|(_, part)| &part.elements)
            .filter_map(|&id| self.mesh.element_index(id))
            .flat_map(|index| elements[index].nodes.iter().copied())
            .collect()
    }

    /// All surface faces of the visible parts.
    pub fn visible_faces(&self) -> BTreeSet<(ElementId, u8)> {
        let elements = self.mesh.elements();
        self.skins
            .iter()
            .zip(&self.parts)
            .filter(|(_, info)| info.visible)
            .flat_map(|(skin, _)| &skin.faces)
            .map(|f| (elements[f.element].id, f.face as u8 + 1))
            .collect()
    }

    /// Edges of 2D elements and shells on the outline of their part, as (part, (element,
    /// CalculiX face number), corner and midside nodes): the faces of 2D models, and the
    /// free edges of shells.
    pub fn outline_edges(&self) -> Vec<(usize, (ElementId, u8), Vec<NodeId>)> {
        let mut edges = Vec::new();
        for (index, part) in self.mesh.parts.iter().enumerate() {
            let mut count: std::collections::HashMap<(NodeId, NodeId), usize> =
                std::collections::HashMap::new();
            let mut found = Vec::new();
            for element in part.elements.iter().filter_map(|&id| self.mesh.element(id)) {
                for (number, edge) in element.edge_faces() {
                    let [a, b] = [edge.corners[0], edge.corners[1]].map(|l| element.nodes[l]);
                    *count.entry((a.min(b), a.max(b))).or_default() += 1;
                    let mut nodes = vec![a, b];
                    if element.shape.is_quadratic() {
                        nodes.extend(edge.mids.iter().map(|&l| element.nodes[l]));
                    }
                    found.push(((element.id, number), nodes));
                }
            }
            for (face, nodes) in found {
                let key = (nodes[0].min(nodes[1]), nodes[0].max(nodes[1]));
                if count[&key] == 1 {
                    edges.push((index, face, nodes));
                }
            }
        }
        edges
    }

    /// Whether the model consists of 2D elements, whose faces are their edges.
    pub fn is_plane(&self) -> bool {
        self.mesh.elements().first().is_some_and(|e| e.is_plane())
    }

    /// Renames a part as CalculiX needs it: upper case, unique among parts, sets and
    /// surfaces. Sections, boundary conditions and loads on the part follow.
    pub fn rename_part(&mut self, index: usize, name: &str) -> Result<(), String> {
        let name = name.trim().to_ascii_uppercase();
        if name.is_empty() {
            return Err("The name must not be empty.".into());
        }
        if name.len() > 80 {
            return Err("The name must be at most 80 characters long.".into());
        }
        if !(name.chars()).all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err("Allowed are letters without accents, digits, _ and -.".into());
        }
        let current = self.parts.get(index).ok_or("The part no longer exists.")?;
        if current.name == name {
            return Ok(());
        }
        if self.mesh.name_in_use(&name) {
            return Err(format!("The name {name} is already used."));
        }
        if self.history.follows() {
            self.history.mesh_replaced(self.mesh.clone());
        }
        if let Some(old) = self.mesh.rename_part(index, &name) {
            self.fe.rename_part(&old, &name);
        }
        self.parts[index].name = name;
        Ok(())
    }

    /// Gives the elements of the part that have the shape of `type_name` this type, as
    /// PrePoMax's part properties do; the choice is kept for the part when it is meshed anew.
    pub fn set_element_type(&mut self, index: usize, type_name: &str) -> Result<(), String> {
        let part = self.parts.get(index).ok_or("The part no longer exists.")?;
        let part = part.name.clone();
        if !self.fe.set_element_type(&part, type_name) {
            return Err(format!("{type_name} is not an element type of a part"));
        }
        self.edit_mesh(|_, _| {});
        Ok(())
    }

    /// The nearest visible face hit by a ray, both relative to the model origin.
    pub fn pick(&self, origin: Vec3, direction: Vec3) -> Option<Hit> {
        let coords = self.shown_coords();
        let position = |node: usize| (DVec3::from(coords[node]) - self.origin).as_vec3();
        let mut best: Option<(f32, Hit)> = None;
        for (part, skin) in self.skins.iter().enumerate() {
            if !self.parts[part].visible {
                continue;
            }
            for (index, face) in skin.faces.iter().enumerate() {
                let a = position(face.corners[0]);
                for pair in face.corners[1..].windows(2) {
                    let (b, c) = (position(pair[0]), position(pair[1]));
                    let Some(t) = ray_triangle(origin, direction, a, b, c) else {
                        continue;
                    };
                    let point = origin + direction * t;
                    if self.clip.is_some_and(|clip| clip.distance(point) < 0.0) {
                        continue;
                    }
                    if best.is_none_or(|(nearest, _)| t < nearest) {
                        best = Some((
                            t,
                            Hit {
                                part,
                                face: index,
                                point,
                                line: false,
                            },
                        ));
                    }
                }
            }
        }
        best.map(|(_, hit)| hit)
    }

    /// The nearest visible line segment within the pick tolerance of a ray; `tolerance`
    /// gives the tolerance at a point of the ray. Returns the distance along the ray too.
    pub fn pick_line(
        &self,
        origin: Vec3,
        direction: Vec3,
        tolerance: impl Fn(Vec3) -> f32,
    ) -> Option<(f32, Hit)> {
        let coords = self.shown_coords();
        let position = |node: usize| (DVec3::from(coords[node]) - self.origin).as_vec3();
        let u = direction.normalize_or_zero();
        let mut best: Option<(f32, Hit)> = None;
        for (part, skin) in self.skins.iter().enumerate() {
            if !self.parts[part].visible {
                continue;
            }
            for (index, &[a, b]) in skin.lines.iter().enumerate() {
                let (a, b) = (position(a), position(b));
                let Some((t, s)) = ray_segment(origin, u, a, b) else {
                    continue;
                };
                let on_ray = origin + u * t;
                let on_line = a + (b - a) * s;
                if on_ray.distance(on_line) > tolerance(on_ray)
                    || self.clip.is_some_and(|clip| clip.distance(on_line) < 0.0)
                {
                    continue;
                }
                if best.is_none_or(|(nearest, _)| t < nearest) {
                    best = Some((
                        t,
                        Hit {
                            part,
                            face: index,
                            point: on_line,
                            line: true,
                        },
                    ));
                }
            }
        }
        best
    }

    /// What a click hits: the nearest face, or a line in front of it or where no face is.
    pub fn pick_click(&self, click: &crate::viewport::Click) -> Option<Hit> {
        let face = self.pick(click.origin, click.direction);
        let line = self.pick_line(click.origin, click.direction, |p| click.precision_at(p));
        match (face, line) {
            (Some(face), Some((t, line))) => {
                let face_t = (face.point - click.origin).dot(click.direction.normalize_or_zero());
                // A line on a face, such as a CAD edge, wins by the pick tolerance.
                Some(if t <= face_t + click.precision_at(line.point) {
                    line
                } else {
                    face
                })
            }
            (face, line) => face.or(line.map(|(_, hit)| hit)),
        }
    }

    /// The node of the hit face or line segment nearest to the hit point.
    pub fn hit_node(&self, hit: &Hit) -> NodeId {
        let coords = self.shown_coords();
        let distance = |node: usize| {
            ((DVec3::from(coords[node]) - self.origin).as_vec3() - hit.point).length_squared()
        };
        let skin = &self.skins[hit.part];
        let nearest = if hit.line {
            let [a, b] = skin.lines[hit.face];
            if distance(a) <= distance(b) { a } else { b }
        } else {
            let face = &skin.faces[hit.face];
            face.corners
                .iter()
                .chain(&face.mids)
                .copied()
                .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
                .unwrap_or(face.corners[0])
        };
        self.mesh.node_ids()[nearest]
    }

    /// The element (by index) whose face or line segment was hit.
    pub fn hit_element(&self, hit: &Hit) -> Option<usize> {
        let skin = self.skins.get(hit.part)?;
        if hit.line {
            skin.line_elements.get(hit.face).copied()
        } else {
            skin.faces.get(hit.face).map(|f| f.element)
        }
    }

    /// The edges of the hit face, or the hit line segment, in render coordinates.
    pub fn hit_outline(&self, hit: &Hit) -> Vec<[Vec3; 2]> {
        let Some(skin) = self.skins.get(hit.part) else {
            return Vec::new();
        };
        if hit.line {
            return (skin.lines.get(hit.face).iter())
                .filter_map(|&&[a, b]| Some([self.node_position(a)?, self.node_position(b)?]))
                .collect();
        }
        skin.faces
            .get(hit.face)
            .map_or_else(Vec::new, |face| self.face_outline(&face.corners))
    }

    /// The visible edges of an element in render coordinates: its skin faces and line
    /// segments.
    pub fn element_outline(&self, element: usize) -> Vec<[Vec3; 2]> {
        let mut edges = Vec::new();
        for skin in &self.skins {
            for face in skin.faces.iter().filter(|f| f.element == element) {
                edges.extend(self.face_outline(&face.corners));
            }
            for (segment, &owner) in skin.lines.iter().zip(&skin.line_elements) {
                if owner == element
                    && let (Some(a), Some(b)) = (
                        self.node_position(segment[0]),
                        self.node_position(segment[1]),
                    )
                {
                    edges.push([a, b]);
                }
            }
        }
        edges
    }

    fn face_outline(&self, corners: &[usize]) -> Vec<[Vec3; 2]> {
        (0..corners.len())
            .filter_map(|i| {
                let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
                Some([self.node_position(a)?, self.node_position(b)?])
            })
            .collect()
    }

    /// Where a node is drawn, in model coordinates: with the exploded view and the shown
    /// deformation.
    pub fn shown_node(&self, id: NodeId) -> Option<[f64; 3]> {
        let index = self.mesh.node_index(id)?;
        self.shown_coords().get(index).copied()
    }

    /// The unit system the model shows its values in: of the results, or of the FE model.
    pub fn units(&self) -> UnitSystem {
        self.results
            .as_ref()
            .map_or(self.fe.properties.units, |v| v.units)
    }

    /// The unit system the model's values are stored in: SI units for an FE model; results
    /// are converted into the units they show when they are read.
    pub fn stored_units(&self) -> UnitSystem {
        match &self.results {
            Some(view) => view.units,
            None => plx_model::STORED,
        }
    }

    /// A value of the model as it is shown.
    pub fn shown(&self, value: f64, quantity: plx_model::Quantity) -> f64 {
        self.stored_units().convert(value, quantity, self.units())
    }

    /// Where a node is drawn, relative to the model origin, including the shown deformation.
    pub fn node_position(&self, index: usize) -> Option<Vec3> {
        let mut p = DVec3::from(*self.mesh.coords().get(index)?) + self.explosion_offset(index);
        if let Some(view) = &self.results {
            let scale = (view.scale() * view.amplitude()) as f64;
            let displacement = view.shown_displacement(index);
            if let (Some(d), true) = (displacement, scale != 0.0) {
                p += scale * DVec3::new(d[0] as f64, d[1] as f64, d[2] as f64);
            }
        }
        Some((p - self.origin).as_vec3())
    }

    /// Where a node is drawn on the given item of the results ([`ResultsView::instances`]).
    pub fn node_position_on(&self, index: usize, item: usize) -> Option<Vec3> {
        let position = self.node_position(index)?;
        if item == 0 {
            return Some(position);
        }
        let instance = *self.results.as_ref()?.instances().get(item)?;
        let global = position.as_dvec3() + self.origin;
        Some((instance.transform_point3(global) - self.origin).as_vec3())
    }

    /// A point given in model coordinates, in render coordinates.
    pub fn to_render(&self, point: [f64; 3]) -> Vec3 {
        (DVec3::from(point) - self.origin).as_vec3()
    }

    /// A point in render coordinates, in model coordinates.
    pub fn model_point(&self, point: Vec3) -> DVec3 {
        point.as_dvec3() + self.origin
    }

    /// The global origin in render coordinates.
    pub fn global_origin(&self) -> Vec3 {
        (-self.origin).as_vec3()
    }

    pub fn file_name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Bounding box of all visible parts, relative to the model origin.
    pub fn visible_bounds(&self) -> Option<(Vec3, Vec3)> {
        self.parts
            .iter()
            .filter(|p| p.visible)
            .filter_map(|p| p.bounds)
            .reduce(|(min_a, max_a), (min_b, max_b)| (min_a.min(min_b), max_a.max(max_b)))
    }
}

fn transform_coords(coords: &[[f64; 3]], instance: &DAffine3) -> Vec<[f64; 3]> {
    (coords.iter())
        .map(|&p| instance.transform_point3(DVec3::from(p)).to_array())
        .collect()
}

/// Closest approach of a ray (unit direction `u`, `t >= 0`) and a segment `a`-`b`: the
/// distance `t` along the ray and the parameter `s` in 0..=1 along the segment. `None` when
/// the closest point of the segment lies behind the ray's origin.
fn ray_segment(origin: Vec3, u: Vec3, a: Vec3, b: Vec3) -> Option<(f32, f32)> {
    let v = b - a;
    let w = origin - a;
    let (uv, vv, uw, vw) = (u.dot(v), v.dot(v), u.dot(w), v.dot(w));
    let denominator = vv - uv * uv;
    let s = if denominator <= f32::EPSILON * vv.max(f32::MIN_POSITIVE) {
        // Parallel: the segment's start is as close as any point of it.
        0.0
    } else {
        ((vw - uv * uw) / denominator).clamp(0.0, 1.0)
    };
    let t = (a + v * s - origin).dot(u);
    (t >= 0.0).then_some((t, s))
}

/// Distance along the ray to a triangle (Möller-Trumbore), seen from either side.
fn ray_triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (ab, ac) = (b - a, c - a);
    let p = direction.cross(ac);
    let det = ab.dot(p);
    if det.abs() < f32::EPSILON * ab.length() * ac.length() {
        return None;
    }
    let s = origin - a;
    let u = s.dot(p) / det;
    let q = s.cross(ab);
    let v = direction.dot(q) / det;
    let t = ac.dot(q) / det;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t >= 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn testdata(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata")
            .join(name)
    }

    #[test]
    fn loads_testdata_with_part_summaries() {
        let loaded = load(&testdata("platte_mit_stuetzen.inp"), UnitSystem::Unitless).unwrap();
        let model = &loaded.model;
        assert_eq!(model.included_files, 1);
        assert_eq!(model.parts.len(), 2);
        assert_eq!(model.parts[0].element_types, [("S4R".to_string(), 16)]);
        assert_eq!(model.parts[0].node_count, 25);
        assert_eq!(model.parts[1].node_count, 4);
        assert_eq!(loaded.render_meshes.len(), 2);
        let (min, max) = model.visible_bounds().unwrap();
        assert_eq!(max - min, Vec3::new(20.0, 20.0, 15.0));
    }

    #[test]
    fn renamed_parts_keep_their_references() {
        let mut model = load(&testdata("platte_mit_stuetzen.inp"), UnitSystem::Unitless)
            .unwrap()
            .model;
        let (first, second) = (model.parts[0].name.clone(), model.parts[1].name.clone());
        model.fe.sections.push(plx_model::Section {
            name: "Section-1".into(),
            material: "Steel".into(),
            region: plx_model::Region::Parts(vec![first.clone()]),
            thickness: 1.0,
            kind: SectionKind::Solid,
        });
        assert!(model.rename_part(0, "").is_err());
        assert!(model.rename_part(0, "zwei wörter").is_err());
        assert!(model.rename_part(0, &second.to_lowercase()).is_err());
        assert_eq!(model.rename_part(0, " Platte-1 "), Ok(()));
        assert_eq!(model.parts[0].name, "PLATTE-1");
        assert_eq!(model.mesh.parts[0].name, "PLATTE-1");
        assert!(model.mesh.element_sets.contains_key("PLATTE-1"));
        assert!(!model.mesh.element_sets.contains_key(&first));
        assert_eq!(
            model.fe.sections[0].region,
            plx_model::Region::Parts(vec!["PLATTE-1".into()])
        );
        assert_eq!(model.rename_part(0, "platte-1"), Ok(()));
    }

    #[test]
    fn chosen_element_types_survive_a_new_mesh() {
        let mut model = load(&testdata("kragbalken_c3d8.inp"), UnitSystem::Unitless)
            .unwrap()
            .model;
        let original = model.mesh.clone();
        assert!(model.set_element_type(0, "C3D10").is_ok());
        assert!(model.set_element_type(0, "XYZ").is_err());
        assert_eq!(model.set_element_type(0, "c3d8i"), Ok(()));
        assert_eq!(model.parts[0].element_types, [("C3D8I".to_string(), 40)]);
        // Meshed anew with the type the mesher gives, the part gets its choice again.
        model.set_mesh(original);
        assert_eq!(model.parts[0].element_types, [("C3D8I".to_string(), 40)]);
    }

    /// Saves the model as the only analysis of a project and opens that analysis again.
    fn save_and_reopen(path: &Path, model: &Model) -> LoadedModel {
        let project = plx_io::project::Project {
            nodes: vec![plx_io::project::ProjectNode::Analysis(Box::new(
                plx_io::project::ProjectAnalysis {
                    id: 1,
                    name: "Analysis-1".into(),
                    geometry: model.geometry.clone(),
                    mesh: model.mesh.clone(),
                    model: model.fe.clone(),
                    ..Default::default()
                },
            ))],
            ..Default::default()
        };
        plx_io::project::save_project(path, &project).unwrap();
        let project = plx_io::project::read_project(path).unwrap();
        load_analysis(path, project.analyses().next().unwrap().clone())
    }

    #[test]
    fn projects_open_with_their_fe_model() {
        let mut model = load(&testdata("platte_mit_stuetzen.inp"), UnitSystem::Unitless)
            .unwrap()
            .model;
        model.fe.steps.push(plx_model::Step::new_static("Step-1"));
        let dir = std::env::temp_dir().join(format!("prepolix-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("platte.plx");
        let loaded = save_and_reopen(&path, &model);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(loaded.model.path, path);
        assert!(!loaded.model.is_results());
        assert_eq!(loaded.model.fe, model.fe);
        assert_eq!(loaded.model.parts.len(), 2);
    }

    #[test]
    fn hidden_parts_do_not_count_for_fitting() {
        let mut model = load(&testdata("platte_mit_stuetzen.inp"), UnitSystem::Unitless)
            .unwrap()
            .model;
        model.parts[1].visible = false;
        let (min, max) = model.visible_bounds().unwrap();
        assert_eq!(max.z - min.z, 0.0);
    }

    #[test]
    fn loads_frd_results_with_deformation() {
        let loaded = load(&testdata("kragbalken_c3d8.frd"), UnitSystem::Unitless).unwrap();
        let model = &loaded.model;
        // The part of the input next to the results, not the material the file groups by.
        assert_eq!(model.parts[0].name, "EALL");
        let view = model.results.as_ref().unwrap();
        assert_eq!(view.current().unwrap().1.name, "ALL");
        assert!(view.scale() > 1.0);
        // The beam bends downwards, so the deformed bounds reach below the undeformed ones.
        let undeformed_min_z = -5.0;
        let (min, _) = model.visible_bounds().unwrap();
        assert!(min.z < undeformed_min_z, "{min}");
        assert!(
            loaded.render_meshes[0]
                .vertices
                .iter()
                .all(|v| v.scalar >= 0.0)
        );
    }

    #[test]
    fn contact_in_the_input_next_to_the_results_opens_them_in_true_scale() {
        let dir = std::env::temp_dir().join("prepolix_contact_results");
        std::fs::create_dir_all(&dir).unwrap();
        let frd = dir.join("job.frd");
        std::fs::copy(testdata("kragbalken_c3d8.frd"), &frd).unwrap();
        std::fs::write(dir.join("job.inp"), "*CONTACT PAIR, INTERACTION=I\nA, B\n").unwrap();
        let model = load(&frd, UnitSystem::Unitless).unwrap().model;
        std::fs::remove_dir_all(&dir).ok();
        let view = model.results.unwrap();
        assert!(view.contact);
        assert_eq!(view.deformation, crate::results::Deformation::TrueScale);
    }

    #[test]
    fn transformed_copies_are_drawn_with_mirrored_values() {
        use plx_results::transformation::{SymmetryPlane, Transformation};
        let mut model = load(&testdata("kragbalken_c3d8.frd"), UnitSystem::Unitless)
            .unwrap()
            .model;
        let single = model.render_meshes()[0].vertices.len();
        let view = model.results.as_mut().unwrap();
        let fields = &view.current_increment().unwrap().fields;
        let field = fields.iter().position(|f| f.name == "DISP").unwrap();
        let components = &fields[field].components;
        let component = components.iter().position(|c| c.name == "U3").unwrap();
        (view.field, view.component) = (field, component);
        let (min, max) = view.legend().map(|l| (l.min, l.max)).unwrap();
        view.transformations = vec![Transformation::symmetry(SymmetryPlane::Z)];
        // Mirrored at z, the beam bending down shows a copy bending up.
        let legend = view.legend().unwrap();
        assert_eq!((legend.min, legend.max), (min.min(-max), max.max(-min)));
        assert_eq!(view.maximum().map(|m| (m.1, m.2)), Some((-min, 1)));
        assert_eq!(model.render_meshes()[0].vertices.len(), 2 * single);
        let node = model.mesh.coords().len() - 1;
        let (a, b) = (
            model.node_position_on(node, 0).unwrap(),
            model.node_position_on(node, 1).unwrap(),
        );
        let global_z = |p: Vec3| p.z as f64 + model.origin().z;
        assert!((global_z(a) + global_z(b)).abs() < 1e-3, "{a} {b}");
    }

    #[test]
    fn picking_finds_faces_and_nodes() {
        let model = load(&testdata("kragbalken_c3d8.inp"), UnitSystem::Unitless)
            .unwrap()
            .model;
        // The beam spans 0..100 x 0..10 x 0..10; look straight down at x = 95, y = 5.
        let origin = Vec3::new(95.0, 5.0, 50.0) - model.origin.as_vec3();
        let hit = model.pick(origin, Vec3::NEG_Z).unwrap();
        assert!((hit.point.z + model.origin.z as f32 - 10.0).abs() < 1e-4);
        let node = model.mesh.node(model.hit_node(&hit)).unwrap();
        assert_eq!(node[2], 10.0);
        assert!(model.pick(origin, Vec3::Z).is_none());
    }

    #[test]
    fn picking_finds_lines_within_the_tolerance() {
        use plx_mesh::{Element, ElementShape, Part};
        // A beam of two quadratic lines along x from 0 to 100 at y = z = 0.
        let mut mesh = FeMesh::default();
        for i in 0..5u32 {
            mesh.set_node(i + 1, [25.0 * f64::from(i), 0.0, 0.0]);
        }
        for (id, nodes) in [(1, vec![1, 2, 3]), (2, vec![3, 4, 5])] {
            mesh.add_element(Element {
                id,
                type_name: "B32".into(),
                shape: ElementShape::Line3,
                nodes,
            })
            .unwrap();
        }
        mesh.parts.push(Part {
            name: "BEAM".into(),
            elements: vec![1, 2],
        });
        let model = Model::new(Path::new("balken.inp"), mesh);
        assert!(
            model.pick(Vec3::ZERO, Vec3::NEG_Z).is_none(),
            "lines have no faces"
        );
        // Looking down 2 units beside the beam at x = 60: within a tolerance of 3, not 1.
        let origin = Vec3::new(60.0, 2.0, 50.0) - model.origin.as_vec3();
        let (t, hit) = model
            .pick_line(origin, Vec3::NEG_Z, |_| 3.0)
            .expect("line within tolerance");
        assert!(hit.line && hit.part == 0);
        assert!((t - 50.0).abs() < 1e-4, "{t}");
        assert!((hit.point.x + model.origin.x as f32 - 60.0).abs() < 1e-4);
        // Node 3 at x = 50 is the nearest node of the hit segment (50..75).
        assert_eq!(model.hit_node(&hit), 3);
        assert!(model.pick_line(origin, Vec3::NEG_Z, |_| 1.0).is_none());
        // Behind the origin nothing is hit.
        assert!(model.pick_line(origin, Vec3::Z, |_| 3.0).is_none());
    }

    /// Whether Gmsh can be used; tests that need it pass with a note otherwise, unless
    /// `PREPOLIX_REQUIRE_GMSH` is set.
    fn gmsh_available() -> bool {
        match plx_mesher::self_test() {
            Ok(_) => true,
            Err(error) if std::env::var_os("PREPOLIX_REQUIRE_GMSH").is_none() => {
                eprintln!("Gmsh not available, test skipped: {error}");
                false
            }
            Err(error) => panic!("Gmsh is required: {error}"),
        }
    }

    #[test]
    fn step_files_open_as_geometry_and_mesh_into_the_project() {
        if !gmsh_available() {
            return;
        }
        let loaded = load(&testdata("zwei_bloecke.step"), UnitSystem::Unitless).unwrap();
        let view = loaded.geometry_view.unwrap();
        assert!(view.is_geometry());
        assert_eq!(view.parts.len(), 2);
        // The display shows the CAD edges but no triangle edges, and each CAD face is its
        // own patch: a block has six.
        assert_eq!(loaded.render_meshes.len(), 2);
        assert!(loaded.render_meshes[0].mesh_edges.is_empty());
        assert!(!loaded.render_meshes[0].feature_edges.is_empty());
        let patches: BTreeSet<usize> = view.skin(0).faces.iter().map(|f| f.region).collect();
        assert_eq!(patches.len(), 6);

        let mut model = loaded.model;
        assert_eq!(model.mesh.element_count(), 0);
        let geometry = model.geometry.clone().unwrap();
        let mesh = plx_mesher::generate_mesh(&geometry).unwrap().mesh;
        model.set_mesh(mesh);
        assert_eq!(model.parts.len(), 2);
        assert!(model.visible_bounds().is_some());

        // The project keeps geometry and mesh; it opens on its mesh with the geometry beside.
        let dir = std::env::temp_dir().join(format!("prepolix-geometry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bloecke.plx");
        let reopened = save_and_reopen(&path, &model);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(reopened.model.geometry, Some(geometry));
        assert!(!model.mesh.cad.is_empty());
        assert_eq!(reopened.model.mesh.cad, model.mesh.cad);
        assert_eq!(
            reopened.model.mesh.element_count(),
            model.mesh.element_count()
        );
        assert!(reopened.geometry_view.is_some());
        assert!(!reopened.render_meshes[0].mesh_edges.is_empty());
    }

    /// Meshes the plate with a hole, sets up a tension test and checks CalculiX's stress.
    #[test]
    fn generated_meshes_solve_with_calculix() {
        if !gmsh_available()
            || std::process::Command::new("ccx")
                .arg("-v")
                .output()
                .is_err()
        {
            eprintln!("Gmsh or ccx missing, test skipped");
            return;
        }
        use plx_model::{
            BoundaryCondition, BoundaryKind, Elastic, Material, Region, Section, Step,
        };
        let mut model = load(&testdata("platte_mit_loch.step"), UnitSystem::Unitless)
            .unwrap()
            .model;
        let mut geometry = model.geometry.clone().unwrap();
        geometry.meshing.max_size = 4.0;
        model.set_mesh(plx_mesher::generate_mesh(&geometry).unwrap().mesh);
        let nodes_at = |x: f64| -> Vec<NodeId> {
            (model.mesh.node_ids().iter().zip(model.mesh.coords()))
                .filter(|(_, c)| (c[0] - x).abs() < 1e-6)
                .map(|(id, _)| *id)
                .collect()
        };
        let (fixed, pulled) = (nodes_at(0.0), nodes_at(100.0));
        let mut step = Step::new_static("Step-1");
        step.boundary_conditions.push(BoundaryCondition {
            name: "Fix".into(),
            active: true,
            visible: true,
            region: Region::Nodes(fixed),
            kind: BoundaryKind::Displacement([Some(0.0); 6]),
            amplitude: None,
        });
        step.boundary_conditions.push(BoundaryCondition {
            name: "Pull".into(),
            active: true,
            visible: true,
            region: Region::Nodes(pulled),
            kind: BoundaryKind::Displacement([Some(0.1), None, None, None, None, None]),
            amplitude: None,
        });
        model.fe = FeModel {
            properties: Default::default(),
            materials: vec![Material {
                name: "Steel".into(),
                density: None,
                elastic: Some(Elastic {
                    young: 210000.0,
                    poisson: 0.3,
                }),
                ..Default::default()
            }],
            sections: vec![Section {
                name: "Section-1".into(),
                material: "Steel".into(),
                region: Region::Parts(vec!["SOLID-1".into()]),
                thickness: 1.0,
                kind: SectionKind::Solid,
            }],
            steps: vec![step],
            user_keywords: Vec::new(),
            ..FeModel::default()
        };
        let inp = plx_io::inp::write_inp(&model.mesh, &model.fe, "Platte").unwrap();
        let dir = std::env::temp_dir().join(format!("prepolix-ccx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("platte.inp"), inp).unwrap();
        let status = std::process::Command::new("ccx")
            .args(["-i", "platte"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(status.status.success(), "ccx: {status:?}");
        let results = read_frd(&dir.join("platte.frd")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        // A strain of 0.1 / 100 gives about 210 MPa in the net section away from the hole;
        // the hole raises the peak, CalculiX's extrapolation keeps it below a few times that.
        let stress = results.increments.last().unwrap().field("STRESS").unwrap();
        let sxx = &stress.component("S11").unwrap().values;
        let max = sxx.iter().copied().fold(f32::MIN, f32::max);
        assert!(max > 300.0 && max < 1500.0, "largest stress {max}");
    }

    /// Shells of a shell section are drawn as the solid of its thickness, placed by its
    /// offset, in 3D FE models only.
    #[test]
    fn shell_sections_are_drawn_thick() {
        let mut mesh = FeMesh::default();
        for (id, c) in [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
        ]
        .into_iter()
        .enumerate()
        {
            mesh.set_node(id as u32 + 1, c);
        }
        mesh.add_element(plx_mesh::Element {
            id: 1,
            type_name: "S4".into(),
            shape: plx_mesh::ElementShape::Quad4,
            nodes: vec![1, 2, 3, 4],
        })
        .unwrap();
        mesh.parts.push(plx_mesh::Part {
            name: "Plate".into(),
            elements: vec![1],
        });
        let mut model = Model::new(Path::new("plate.inp"), mesh);
        model.fe.sections.push(plx_model::Section {
            name: "Shell_Section-1".into(),
            material: "Steel".into(),
            region: plx_model::Region::Parts(vec!["Plate".into()]),
            thickness: 1.0,
            kind: SectionKind::Shell {
                thickness: 2.0,
                offset: 0.5,
            },
        });
        let z_range = |model: &Model| {
            let (min, max) = model.render_meshes()[0].bounds().unwrap();
            let origin = model.origin.z as f32;
            (min.z + origin, max.z + origin)
        };
        assert_eq!(z_range(&model), (0.0, 0.0));
        model.thick_shells = model.shell_sections().cloned().collect();
        assert_eq!(model.thick_shells.len(), 1);
        // The nodes lie on the top face, the solid below them.
        let (low, high) = z_range(&model);
        assert!(
            (low + 2.0).abs() < 1e-5 && high.abs() < 1e-5,
            "{low} {high}"
        );
        model.fe.properties.space = plx_model::ModelSpace::PlaneStress;
        assert_eq!(model.shell_sections().count(), 0);
    }

    /// A flat disk in a 3D model is meshed as shells and thickened by a shell section; thin,
    /// clamped at its rim and loaded by its weight it sags as a Kirchhoff plate does.
    #[test]
    fn surfaces_in_3d_models_solve_as_shells() {
        if !gmsh_available()
            || std::process::Command::new("ccx")
                .arg("-v")
                .output()
                .is_err()
        {
            eprintln!("Gmsh or ccx missing, test skipped");
            return;
        }
        use plx_model::{
            BoundaryCondition, BoundaryKind, Elastic, Load, LoadKind, Material, ModelSpace, Region,
            Section, Step,
        };
        let mut model = load(&testdata("membran.step"), UnitSystem::Unitless)
            .unwrap()
            .model;
        let mut geometry = model.geometry.clone().unwrap();
        geometry.meshing.max_size = 2.0;
        geometry.meshing.quad_dominated = true;
        let mut mesh = plx_mesher::generate_mesh(&geometry).unwrap().mesh;
        ModelSpace::ThreeD
            .prepare_generated_mesh(&mut mesh)
            .unwrap();
        assert!(mesh.elements().iter().all(|e| e.type_name == "S8"));
        model.set_mesh(mesh);
        let (radius, thickness, young, poisson) = (20.0, 0.25, 210000.0, 0.3);
        let (density, gravity) = (7.85e-9, 9810.0);
        let rim: Vec<NodeId> = (model.mesh.node_ids().iter().zip(model.mesh.coords()))
            .filter(|(_, c)| (c[0].hypot(c[1]) - radius).abs() < 1e-6)
            .map(|(id, _)| *id)
            .collect();
        assert!(rim.len() > 20);
        let part = model.mesh.parts[0].name.clone();
        let mut step = Step::new_static("Step-1");
        step.boundary_conditions.push(BoundaryCondition {
            name: "Clamped".into(),
            active: true,
            visible: true,
            region: Region::Nodes(rim),
            kind: BoundaryKind::Displacement([Some(0.0); 6]),
            amplitude: None,
        });
        step.loads.push(Load {
            name: "Weight".into(),
            active: true,
            visible: true,
            region: Region::Parts(vec![part.clone()]),
            kind: LoadKind::Gravity([0.0, 0.0, -gravity]),
            amplitude: None,
            factor_amplitude: None,
        });
        let solid = Section {
            name: "Section-1".into(),
            material: "Steel".into(),
            region: Region::Parts(vec![part]),
            thickness: 1.0,
            kind: SectionKind::Solid,
        };
        model.fe = FeModel {
            materials: vec![Material {
                name: "Steel".into(),
                density: Some(density),
                elastic: Some(Elastic { young, poisson }),
                ..Default::default()
            }],
            sections: vec![solid.clone()],
            steps: vec![step],
            ..FeModel::default()
        };
        // A solid section would leave the shells without a thickness.
        assert!(plx_io::inp::write_inp(&model.mesh, &model.fe, "Membran").is_err());
        model.fe.sections[0].kind = SectionKind::Shell {
            thickness,
            offset: 0.0,
        };
        let inp = plx_io::inp::write_inp(&model.mesh, &model.fe, "Membran").unwrap();
        assert!(inp.contains("*Shell section"), "{inp}");
        let dir = std::env::temp_dir().join(format!("prepolix-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("membran.inp"), inp).unwrap();
        let status = std::process::Command::new("ccx")
            .args(["-i", "membran"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(status.status.success(), "ccx: {status:?}");
        let results = read_frd(&dir.join("membran.frd")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let disp = results.increments.last().unwrap().field("DISP").unwrap();
        let sag = (disp.component("U3").unwrap().values.iter())
            .copied()
            .fold(0.0f32, f32::min);
        // Centre deflection of a clamped circular plate: q a^4 / (64 D).
        let q = density * gravity * thickness;
        let stiffness = young * thickness.powi(3) / (12.0 * (1.0 - poisson * poisson));
        let expected = -q * radius.powi(4) / (64.0 * stiffness);
        assert!(
            (f64::from(sag) / expected - 1.0).abs() < 0.02,
            "sag {sag}, plate theory {expected}"
        );
    }

    /// Parts sharing a section share a material, which is all an `.frd` file groups the
    /// elements by; the results take the parts of the input they were computed from.
    #[test]
    fn results_list_the_parts_of_their_input() {
        if std::process::Command::new("ccx")
            .arg("-v")
            .output()
            .is_err()
        {
            eprintln!("ccx missing, test skipped");
            return;
        }
        use plx_model::{
            BoundaryCondition, BoundaryKind, Elastic, Material, Region, Section, Step,
        };
        // Three cubes in a row, one part each, sharing the nodes where they touch.
        let mut mesh = FeMesh::default();
        let mut id = 0;
        for x in 0..=3 {
            for (y, z) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                id += 1;
                mesh.set_node(id, [x as f64, y as f64, z as f64]);
            }
        }
        let names = ["Solid-4", "SOLID-5", "SOLID-6"];
        for (index, name) in names.iter().enumerate() {
            let first = 4 * index as NodeId;
            let nodes = (1..=8).map(|n| first + n).collect();
            let id = index as ElementId + 1;
            (mesh.add_element(plx_mesh::Element {
                id,
                type_name: "C3D8".into(),
                shape: plx_mesh::ElementShape::Hex8,
                nodes,
            }))
            .unwrap();
            mesh.parts.push(plx_mesh::Part {
                name: name.to_string(),
                elements: vec![id],
            });
        }
        let mut step = Step::new_static("Step-1");
        for (name, nodes, kind) in [
            (
                "Fix",
                vec![1, 2, 3, 4],
                BoundaryKind::Displacement([Some(0.0); 6]),
            ),
            (
                "Pull",
                vec![13, 14, 15, 16],
                BoundaryKind::Displacement([Some(0.01), None, None, None, None, None]),
            ),
        ] {
            step.boundary_conditions.push(BoundaryCondition {
                name: name.into(),
                active: true,
                visible: true,
                region: Region::Nodes(nodes),
                kind,
                amplitude: None,
            });
        }
        let fe = FeModel {
            materials: vec![Material {
                name: "S235".into(),
                elastic: Some(Elastic {
                    young: 210000.0,
                    poisson: 0.3,
                }),
                ..Default::default()
            }],
            sections: vec![Section {
                name: "Section-1".into(),
                material: "S235".into(),
                region: Region::Parts(names.map(String::from).to_vec()),
                thickness: 1.0,
                kind: SectionKind::Solid,
            }],
            steps: vec![step],
            ..FeModel::default()
        };
        let inp = plx_io::inp::write_inp(&mesh, &fe, "Bloecke").unwrap();
        let dir = std::env::temp_dir().join(format!("prepolix-parts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bloecke.inp"), inp).unwrap();
        let status = std::process::Command::new("ccx")
            .args(["-i", "bloecke"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(status.status.success(), "ccx: {status:?}");
        let frd = dir.join("bloecke.frd");
        let alone = read_frd(&frd).unwrap().mesh;
        let parts = |mesh: &FeMesh| -> Vec<(String, Vec<ElementId>)> {
            (mesh.parts.iter())
                .map(|p| (p.name.clone(), p.elements.clone()))
                .collect()
        };
        assert_eq!(parts(&alone), [("S235".to_string(), vec![1, 2, 3])]);
        let results = load(&frd, UnitSystem::Unitless).unwrap().model;
        // Without the input the results keep the grouping of the file.
        std::fs::remove_file(dir.join("bloecke.inp")).unwrap();
        let without_input = load(&frd, UnitSystem::Unitless).unwrap().model;
        std::fs::remove_dir_all(&dir).unwrap();
        let expected: Vec<_> = (names.iter().zip(1..))
            .map(|(name, id)| (name.to_string(), vec![id]))
            .collect();
        assert_eq!(parts(&results.mesh), expected);
        assert_eq!(results.parts.len(), 3);
        assert_eq!(parts(&without_input.mesh), parts(&alone));
    }

    #[test]
    fn file_without_elements_is_rejected() {
        assert!(load(&testdata("platte_knoten.inp"), UnitSystem::Unitless).is_err());
    }

    #[test]
    fn bound_results_open_with_what_was_defined_on_them() {
        let mut model = load(&testdata("kragbalken_c3d8.frd"), UnitSystem::Unitless)
            .unwrap()
            .model;
        let settings = plx_results::ResultsSettings {
            field_outputs: vec![plx_results::FieldOutput {
                name: "SCALED".into(),
                kind: plx_results::FieldOutputKind::Equation {
                    equation: "=DISP.ALL * 1000".into(),
                    unit: "um".into(),
                },
            }],
            transformations: vec![plx_results::Transformation::symmetry(
                plx_results::transformation::SymmetryPlane::X,
            )],
            hot_spots: vec![plx_results::hot_spot::HotSpot::new("Hot_Spot-1")],
            reference_points: vec![plx_model::ReferencePoint::new("RP-1", [1.0, 2.0, 3.0])],
            result_paths: vec![plx_model::ResultPath::new("Path-1")],
            plain_part_names: vec![model.parts[0].name.clone()],
            mesh: Some(model.mesh.fingerprint()),
            increment: 0,
            shown: Some(("SCALED".into(), "VALUE".into())),
            view: plx_results::saved_view::SavedView {
                deformation: Some(plx_results::saved_view::SavedDeformation::TrueScale),
                user_scale: Some(2.5),
                show_undeformed: Some(false),
                levels: Some(12),
                color_scale: Some(plx_model::ColorScale {
                    spectrum: plx_model::ColorSpectrum::CoolWarm,
                    levels: 12,
                    manual: Some([0.5, 3.0]),
                    ..Default::default()
                }),
                hidden_parts: vec![model.parts[0].name.clone()],
                annotations: vec![
                    plx_results::saved_view::SavedAnnotation::Node(99),
                    plx_results::saved_view::SavedAnnotation::Distance([1, 99]),
                ],
                path_plots: vec![
                    crate::path_plot::PathPlot::new("Path-1".into()).saved(),
                    crate::path_plot::PathPlot::hot_spot("Hot_Spot-1".into()).saved(),
                    crate::path_plot::PathPlot::table(crate::xy_plot::XyData {
                        title: "SP_A.STRESSES.MISES".into(),
                        x_label: "Increment".into(),
                        x: vec![1.0, 2.0],
                        curves: vec![("12_1".into(), vec![0.5, 1.5])],
                        guides: Vec::new(),
                        y_max: None,
                    })
                    .saved(),
                ],
                // Camera and mesh edges belong to the 3D view, not to the results.
                camera: None,
                mesh_edges: None,
            },
            ..Default::default()
        };
        assert_eq!(
            model.restore_results_settings(&settings, Vec::new()),
            Vec::<String>::new()
        );
        assert!(!model.parts[0].contours);
        let view = model.results.as_ref().unwrap();
        let (field, component) = view.current().unwrap();
        assert_eq!(
            (field.name.as_str(), component.name.as_str()),
            ("SCALED", "VALUE")
        );
        assert_eq!(model.results_settings(), Some(settings.clone()));
        // Projects saved before part names were kept name parts by index, which only fit
        // results of the same mesh.
        let mut legacy = settings;
        legacy.plain_part_names.clear();
        legacy.plain_parts = vec![0];
        legacy.mesh = Some(model.mesh.fingerprint() ^ 1);
        model.set_contours(&[0], true);
        model.restore_results_settings(&legacy, Vec::new());
        assert!(model.parts[0].contours);
        legacy.mesh = Some(model.mesh.fingerprint());
        model.restore_results_settings(&legacy, Vec::new());
        assert!(!model.parts[0].contours);
    }
}
