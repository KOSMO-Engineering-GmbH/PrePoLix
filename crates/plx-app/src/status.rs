//! The state of the cells of an analysis, as the blocks of Ansys Workbench's project
//! schematic show it: whether geometry, mesh, setup, solution and results are there and up to
//! date. A cell is out of date when the fingerprint of its inputs differs from the one it
//! was made from, or when a cell it is made from is out of date; Solve then makes only those
//! cells again.

use std::collections::BTreeMap;

use egui::{Color32, Painter, Rect, Shape, Stroke, pos2};
use plx_io::project::BoundResults;
use plx_mesh::FeMesh;
use plx_model::{FeModel, Geometry, fingerprint};

use crate::tree_icons::{self, TreeIcon};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellState {
    /// Not there yet: nothing imported, meshed, set up or solved.
    Missing,
    UpToDate,
    /// Made from inputs that changed since: Solve makes it again.
    OutOfDate,
    Running,
    /// CalculiX reported errors; results may still be there.
    Failed,
    /// Not part of this analysis, such as the geometry of a mesh read from an input file.
    NotUsed,
}

impl CellState {
    pub fn text(self) -> &'static str {
        match self {
            CellState::Missing => "not there yet",
            CellState::UpToDate => "up to date",
            CellState::OutOfDate => "out of date: Solve makes it again",
            CellState::Running => "running",
            CellState::Failed => "CalculiX reported errors",
            CellState::NotUsed => "not used: the mesh comes from an input file",
        }
    }
}

/// The cells of an analysis, top to bottom as the project schematic shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisStatus {
    pub geometry: CellState,
    pub mesh: CellState,
    pub setup: CellState,
    pub solution: CellState,
    pub results: CellState,
    /// Parts of the geometry whose mesh is out of date, to be meshed again.
    pub stale_parts: Vec<String>,
}

impl AnalysisStatus {
    /// The cells with their names, as the schematic lists them.
    pub fn cells(&self) -> [(&'static str, CellState); 5] {
        [
            ("Geometry", self.geometry),
            ("Mesh", self.mesh),
            ("Setup", self.setup),
            ("Solution", self.solution),
            ("Results", self.results),
        ]
    }

    /// Whether there is something to solve: a mesh, or geometry to mesh, and a step.
    pub fn solvable(&self) -> bool {
        self.setup != CellState::Missing
            && (self.mesh != CellState::Missing || self.geometry == CellState::UpToDate)
    }

    /// Whether Solve has anything to do.
    pub fn needs_solve(&self) -> bool {
        matches!(self.mesh, CellState::OutOfDate)
            || matches!(self.solution, CellState::Missing | CellState::OutOfDate)
    }
}

/// The state of each cell of an analysis with this data; `running` while CalculiX solves it.
pub fn analysis_status(
    geometry: Option<&Geometry>,
    mesh: &FeMesh,
    fe: &FeModel,
    mesh_inputs: &BTreeMap<String, u64>,
    results: Option<&BoundResults>,
    running: bool,
) -> AnalysisStatus {
    let space = fe.properties.space;
    let meshed = mesh.element_count() > 0;
    let geometry_state = match geometry {
        Some(_) => CellState::UpToDate,
        None if meshed => CellState::NotUsed,
        None => CellState::Missing,
    };
    // Only parts meshed from the geometry know their inputs; parts read from input files
    // have none and stay as they are.
    let stale_parts: Vec<String> = match geometry {
        Some(geometry) => (mesh.parts.iter())
            .filter_map(|part| {
                let made_from = mesh_inputs.get(&part.name)?;
                (fingerprint::mesh_input(geometry, space, &part.name) != *made_from)
                    .then(|| part.name.clone())
            })
            .collect(),
        None => Vec::new(),
    };
    let mesh_state = if !meshed {
        CellState::Missing
    } else if stale_parts.is_empty() {
        CellState::UpToDate
    } else {
        CellState::OutOfDate
    };
    let setup = if fe.steps.is_empty() {
        CellState::Missing
    } else {
        CellState::UpToDate
    };
    let solution = match results {
        _ if running => CellState::Running,
        None => CellState::Missing,
        Some(results) => {
            let solved = results.input == Some(fingerprint::solve_input(mesh, fe));
            if !solved || mesh_state != CellState::UpToDate {
                CellState::OutOfDate
            } else if results.with_errors {
                CellState::Failed
            } else {
                CellState::UpToDate
            }
        }
    };
    let results_state = match (results, solution) {
        (None, _) => CellState::Missing,
        (Some(_), CellState::OutOfDate | CellState::Running) => CellState::OutOfDate,
        (Some(_), _) => CellState::UpToDate,
    };
    AnalysisStatus {
        geometry: geometry_state,
        mesh: mesh_state,
        setup,
        solution,
        results: results_state,
        stale_parts,
    }
}

/// Paints the symbol of a state into a 16 × 16 square at `min`, as Ansys Workbench marks its
/// cells, with PrePoMax's icons where it has one: OK, an arrow for out of date, a question mark
/// for missing.
pub fn paint(painter: &Painter, min: egui::Pos2, state: CellState) {
    let p = |x: f32, y: f32| pos2(min.x + x, min.y + y);
    let adapt = crate::style::adapt;
    match state {
        CellState::UpToDate => tree_icons::paint(painter, min, TreeIcon::Finished),
        CellState::OutOfDate => {
            // A circular arrow: Update.
            let amber = adapt(Color32::from_rgb(220, 150, 0));
            let center = p(8.0, 8.5);
            let points: Vec<egui::Pos2> = (0..=12)
                .map(|i| {
                    let angle = (40.0 + i as f32 * 25.0_f32).to_radians();
                    center + 5.0 * egui::vec2(angle.cos(), -angle.sin())
                })
                .collect();
            let end = *points.last().unwrap_or(&center);
            painter.add(Shape::line(points, Stroke::new(2.0, amber)));
            painter.add(Shape::convex_polygon(
                vec![
                    end + egui::vec2(-3.0, -1.5),
                    end + egui::vec2(3.0, -2.5),
                    end + egui::vec2(0.5, 2.5),
                ],
                amber,
                Stroke::NONE,
            ));
        }
        CellState::Missing => {
            painter.text(
                p(8.0, 8.5),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(13.0),
                adapt(Color32::from_rgb(60, 110, 200)),
            );
        }
        CellState::Running => tree_icons::paint(painter, min, TreeIcon::Running),
        CellState::Failed => tree_icons::paint(painter, min, TreeIcon::NoResult),
        CellState::NotUsed => {
            painter.line_segment(
                [p(4.0, 8.5), p(12.0, 8.5)],
                Stroke::new(1.5, adapt(Color32::GRAY)),
            );
        }
    }
}

/// A 16 × 16 rect for a state symbol, centred in `rect`.
pub fn symbol_rect(rect: Rect) -> Rect {
    Rect::from_center_size(rect.center(), egui::Vec2::splat(tree_icons::SIZE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use plx_model::{MeshingParameters, Step};

    fn block() -> FeMesh {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        plx_io::inp::read_inp(&root.join("testdata/block_c3d20r.inp"))
            .unwrap()
            .mesh
    }

    fn results(input: u64) -> BoundResults {
        BoundResults {
            analysis: "Analysis-1".into(),
            input: Some(input),
            ..Default::default()
        }
    }

    #[test]
    fn a_changed_load_makes_only_the_solution_out_of_date() {
        let mesh = block();
        let mut fe = FeModel::default();
        fe.steps.push(Step::new_static("Step-1"));
        let solved = results(fingerprint::solve_input(&mesh, &fe));
        let status = analysis_status(None, &mesh, &fe, &BTreeMap::new(), Some(&solved), false);
        assert_eq!(status.geometry, CellState::NotUsed);
        assert_eq!(status.mesh, CellState::UpToDate);
        assert_eq!(status.solution, CellState::UpToDate);
        assert!(!status.needs_solve());
        // A result path changes nothing that CalculiX solves.
        fe.result_paths.push(plx_model::ResultPath::new("Path-1"));
        let status = analysis_status(None, &mesh, &fe, &BTreeMap::new(), Some(&solved), false);
        assert_eq!(status.solution, CellState::UpToDate);
        fe.steps[0].name = "Loading".into();
        let status = analysis_status(None, &mesh, &fe, &BTreeMap::new(), Some(&solved), false);
        assert_eq!(status.mesh, CellState::UpToDate);
        assert_eq!(status.solution, CellState::OutOfDate);
        assert_eq!(status.results, CellState::OutOfDate);
        assert!(status.needs_solve());
    }

    #[test]
    fn new_meshing_settings_make_the_mesh_of_that_part_out_of_date() {
        let mesh = block();
        let part = mesh.parts[0].name.clone();
        let mut geometry = Geometry {
            source: "block.step".into(),
            brep: "shapes".into(),
            meshing: MeshingParameters::default(),
            mesh_items: Vec::new(),
            part_names: Vec::new(),
        };
        let fe = FeModel::default();
        let space = fe.properties.space;
        let inputs = BTreeMap::from([(
            part.clone(),
            fingerprint::mesh_input(&geometry, space, &part),
        )]);
        let solved = results(fingerprint::solve_input(&mesh, &fe));
        let status = analysis_status(Some(&geometry), &mesh, &fe, &inputs, Some(&solved), false);
        assert_eq!(status.mesh, CellState::UpToDate);
        assert_eq!(status.setup, CellState::Missing);
        geometry.meshing.max_size *= 0.5;
        let status = analysis_status(Some(&geometry), &mesh, &fe, &inputs, Some(&solved), false);
        assert_eq!(status.mesh, CellState::OutOfDate);
        assert_eq!(status.stale_parts, [part]);
        // The solution follows the mesh.
        assert_eq!(status.solution, CellState::OutOfDate);
    }

    #[test]
    fn analyses_without_results_are_not_solved() {
        let status = analysis_status(
            None,
            &FeMesh::default(),
            &FeModel::default(),
            &BTreeMap::new(),
            None,
            false,
        );
        assert_eq!(status.geometry, CellState::Missing);
        assert_eq!(status.mesh, CellState::Missing);
        assert_eq!(status.solution, CellState::Missing);
        let running = analysis_status(
            None,
            &FeMesh::default(),
            &FeModel::default(),
            &BTreeMap::new(),
            None,
            true,
        );
        assert_eq!(running.solution, CellState::Running);
    }
}
