//! Sketches of the model spaces for their hover help: what a plane stress, plane strain or
//! axisymmetric model stands for in 3D, drawn with the painter.

use egui::{Color32, FontId, Pos2, Shape, Stroke, vec2};
use plx_model::ModelSpace;

use crate::style;

/// Size of a sketch: the picture on the left, its notes on the right.
const SIZE: egui::Vec2 = egui::vec2(440.0, 132.0);
/// Left edge of the notes.
const NOTES_X: f32 = 205.0;

/// The hover help of a model space: its hint with a sketch of the principle below.
pub fn hover(ui: &mut egui::Ui, space: ModelSpace) {
    ui.label(space.tooltip());
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(SIZE, egui::Sense::hover());
    let sketch = Sketch::new(ui, rect);
    match space {
        ModelSpace::ThreeD => sketch.three_d(),
        ModelSpace::PlaneStress => sketch.plane_stress(),
        ModelSpace::PlaneStrain => sketch.plane_strain(),
        ModelSpace::Axisymmetric => sketch.axisymmetric(),
    }
}

/// Mix of two colours, `t` of the way from `a` to `b`, opaque so that faces drawn over one
/// another do not add up.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

struct Sketch {
    painter: egui::Painter,
    rect: egui::Rect,
    /// Origin of the oblique view, set by each sketch.
    origin: Pos2,
    line: Stroke,
    body: Color32,
    /// Faces turned away from the light.
    shade: Color32,
    /// The part that is modelled.
    model: Color32,
    model_line: Stroke,
    load: Stroke,
    text: Color32,
    background: Color32,
}

impl Sketch {
    fn new(ui: &egui::Ui, rect: egui::Rect) -> Self {
        let background = ui.visuals().window_fill;
        let text = style::text();
        let highlight = style::lift(style::highlight());
        Self {
            painter: ui.painter_at(rect),
            rect,
            origin: rect.min,
            line: Stroke::new(1.0, mix(background, text, 0.75)),
            body: mix(background, text, 0.12),
            shade: mix(background, text, 0.22),
            model: mix(background, highlight, 0.45),
            model_line: Stroke::new(1.5, highlight),
            load: Stroke::new(1.5, Color32::from_rgb(230, 110, 40)),
            text,
            background,
        }
    }

    /// A point of the drawing's plane, `x` to the right and `y` up from the origin.
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, -y)
    }

    /// A point in space in an oblique view: X to the right, Y up, Z out of the screen, so
    /// that the depth behind the X-Y plane runs up to the right.
    fn p(&self, x: f32, y: f32, z: f32) -> Pos2 {
        self.origin + vec2(x - 0.5 * z, -y + 0.35 * z)
    }

    fn face(&self, points: Vec<Pos2>, fill: Color32, stroke: Stroke) {
        self.painter
            .add(Shape::convex_polygon(points, fill, stroke));
    }

    /// The faces of a box seen from the front, the top and the right; the front face in
    /// `front`, the others in the body's colours.
    fn block(&self, [x0, x1]: [f32; 2], [y0, y1]: [f32; 2], [z0, z1]: [f32; 2], front: Color32) {
        let p = |x, y, z| self.p(x, y, z);
        let top = vec![p(x0, y1, z1), p(x1, y1, z1), p(x1, y1, z0), p(x0, y1, z0)];
        let right = vec![p(x1, y0, z1), p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1)];
        let face = vec![p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)];
        self.face(top, self.body, self.line);
        self.face(right, self.shade, self.line);
        self.face(face, front, self.line);
    }

    /// The band between two lines of as many points, filled as one mesh so that no seams
    /// show between its pieces.
    fn strip(&self, a: &[Pos2], b: &[Pos2], fill: Color32) {
        let mut mesh = egui::Mesh::default();
        for (&pa, &pb) in a.iter().zip(b) {
            mesh.colored_vertex(pa, fill);
            mesh.colored_vertex(pb, fill);
        }
        for i in 0..a.len().saturating_sub(1) as u32 {
            let k = 2 * i;
            mesh.add_triangle(k, k + 1, k + 3);
            mesh.add_triangle(k, k + 3, k + 2);
        }
        self.painter.add(mesh);
    }

    fn arrow(&self, from: Pos2, to: Pos2, stroke: Stroke) {
        self.painter.line_segment([from, to], stroke);
        self.head(from, to, stroke);
    }

    /// An arrow head at `tip`, pointing away from `from`.
    fn head(&self, from: Pos2, tip: Pos2, stroke: Stroke) {
        let dir = (tip - from).normalized();
        let side = vec2(-dir.y, dir.x);
        let back = tip - dir * 6.0;
        self.painter.add(Shape::convex_polygon(
            vec![tip, back + side * 3.0, back - side * 3.0],
            stroke.color,
            Stroke::NONE,
        ));
    }

    fn label(&self, pos: Pos2, align: egui::Align2, text: &str, color: Color32) {
        self.painter
            .text(pos, align, text, FontId::proportional(12.0), color);
    }

    /// The notes beside the picture, one per line; the first in the model's colour.
    fn notes(&self, lines: &[&str]) {
        for (i, line) in lines.iter().enumerate() {
            let pos = self.rect.min + vec2(NOTES_X, 8.0 + 17.0 * i as f32);
            let color = if i == 0 {
                self.model_line.color
            } else {
                self.text
            };
            self.label(pos, egui::Align2::LEFT_TOP, line, color);
        }
    }

    /// The global axes in the corner of the notes, with Z when the sketch is spatial.
    fn axes(&self, with_z: bool) {
        let o = self.rect.min + vec2(NOTES_X + 22.0, SIZE.y - 24.0);
        let axis = Stroke::new(1.2, self.text);
        let mut ends = vec![(o + vec2(26.0, 0.0), "X"), (o + vec2(0.0, -26.0), "Y")];
        if with_z {
            ends.push((o + vec2(-13.0, 9.1), "Z"));
        }
        for (end, name) in ends {
            self.arrow(o, end, axis);
            let away = (end - o).normalized() * 8.0;
            self.label(end + away, egui::Align2::CENTER_CENTER, name, self.text);
        }
    }

    /// An L-shaped bracket, held and loaded in space.
    fn three_d(mut self) {
        self.origin = self.rect.min + vec2(28.0, 118.0);
        let depth = [-60.0, 0.0];
        self.block([0.0, 100.0], [0.0, 22.0], depth, self.model);
        self.block([0.0, 24.0], [22.0, 88.0], depth, self.model);
        for z in [-45.0, -15.0] {
            self.arrow(self.p(85.0, 52.0, z), self.p(85.0, 22.0, z), self.load);
        }
        self.arrow(
            self.p(54.0, 75.0, -30.0),
            self.p(24.0, 75.0, -30.0),
            self.load,
        );
        self.notes(&["The whole body is modelled.", "All six stresses can occur."]);
        self.axes(true);
    }

    /// A thin plate with a hole, pulled in its plane; its thickness is free to change.
    fn plane_stress(mut self) {
        self.origin = self.rect.min + vec2(24.0, 108.0);
        let (w, h, t) = (140.0, 80.0, 8.0);
        self.block([0.0, w], [0.0, h], [-t, 0.0], self.model);
        let hole = self.at(w / 2.0, h / 2.0);
        self.painter
            .circle(hole, 16.0, self.background, self.model_line);
        self.painter.rect_stroke(
            egui::Rect::from_two_pos(self.at(0.0, 0.0), self.at(w, h)),
            0.0,
            self.model_line,
            egui::StrokeKind::Middle,
        );
        for y in [14.0, 40.0, 66.0] {
            self.arrow(self.at(0.0, y), self.at(-18.0, y), self.load);
            self.arrow(self.at(w + 4.0, y), self.at(w + 22.0, y), self.load);
        }
        let corner = self.p(w, h, -t / 2.0);
        self.label(
            corner + vec2(6.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            "t",
            self.text,
        );
        self.notes(&[
            "Model: the plate in X-Y, thickness t",
            "Thin, loaded only in its plane",
            "S33 = 0: free to grow thinner",
        ]);
        self.axes(false);
    }

    /// A long prism under a load along its whole length; one slice stands for it.
    fn plane_strain(mut self) {
        self.origin = self.rect.min + vec2(40.0, 120.0);
        let (w, h, length) = (40.0, 34.0, 170.0);
        // The long body, see-through, so that the slice inside shows.
        let p = |x, y, z| self.p(x, y, z);
        let ghost = |c: Color32| c.gamma_multiply(0.5);
        let back = vec![
            p(0.0, 0.0, -length),
            p(w, 0.0, -length),
            p(w, h, -length),
            p(0.0, h, -length),
        ];
        self.face(
            back,
            Color32::TRANSPARENT,
            Stroke::new(1.0, ghost(self.line.color)),
        );
        let top = vec![
            p(0.0, h, 0.0),
            p(w, h, 0.0),
            p(w, h, -length),
            p(0.0, h, -length),
        ];
        let right = vec![
            p(w, 0.0, 0.0),
            p(w, 0.0, -length),
            p(w, h, -length),
            p(w, h, 0.0),
        ];
        self.face(top, ghost(self.body), self.line);
        self.face(right, ghost(self.shade), self.line);
        let slice = -length / 2.0;
        self.block([0.0, w], [0.0, h], [slice - 6.0, slice], self.model);
        let front = vec![
            p(0.0, 0.0, 0.0),
            p(w, 0.0, 0.0),
            p(w, h, 0.0),
            p(0.0, h, 0.0),
        ];
        self.face(front, Color32::TRANSPARENT, self.line);
        for i in 0..7 {
            let z = -10.0 - 25.0 * i as f32;
            self.arrow(p(w / 2.0, h + 16.0, z), p(w / 2.0, h, z), self.load);
        }
        self.notes(&[
            "Model: one slice in X-Y, thickness t",
            "Long, same section and load all along",
            "E33 = 0: held in its length",
        ]);
        self.axes(true);
    }

    /// A tube cut open along its axis; the profile on the right is turned about Y.
    fn axisymmetric(mut self) {
        let axis = 80.0;
        self.origin = self.rect.min + vec2(axis, 108.0);
        let (inner, outer, h, flat) = (22.0, 52.0, 70.0, 0.28);
        // The far half of an ellipse about the axis at height y, from +X to -X.
        let arc = |r: f32, y: f32, steps: usize| -> Vec<Pos2> {
            (0..=steps)
                .map(|i| {
                    let a = std::f32::consts::PI * i as f32 / steps as f32;
                    self.at(r * a.cos(), y + r * flat * a.sin())
                })
                .collect()
        };
        // The inside of the far wall, seen through the cut.
        let (wall_top, wall_foot) = (arc(inner, h, 16), arc(inner, 0.0, 16));
        self.strip(&wall_foot, &wall_top, self.shade);
        self.painter.add(Shape::line(wall_foot, self.line));
        // The cut ring at the top, as strips between the inner and outer arc.
        let (rim_in, rim_out) = (arc(inner, h, 16), arc(outer, h, 16));
        self.strip(&rim_in, &rim_out, self.body);
        self.painter.add(Shape::line(rim_in, self.line));
        self.painter.add(Shape::line(rim_out, self.line));
        // The cut faces: the profile at X >= 0 is the model, its mirror image is not.
        let left = vec![
            self.at(-outer, 0.0),
            self.at(-inner, 0.0),
            self.at(-inner, h),
            self.at(-outer, h),
        ];
        self.face(left, self.body, self.line);
        let profile = vec![
            self.at(inner, 0.0),
            self.at(outer, 0.0),
            self.at(outer, h),
            self.at(inner, h),
        ];
        self.face(profile, self.model, self.model_line);
        // The axis of revolution and the turn about it.
        let dash = Stroke::new(1.2, self.text);
        self.painter.extend(Shape::dashed_line(
            &[self.at(0.0, -8.0), self.at(0.0, h + 30.0)],
            dash,
            6.0,
            3.0,
        ));
        self.label(
            self.at(-22.0, h + 24.0),
            egui::Align2::RIGHT_CENTER,
            "Y",
            self.text,
        );
        let turn: Vec<Pos2> = (0..=24)
            .map(|i| {
                let a = 3.8 + 4.6 * i as f32 / 24.0;
                self.at(16.0 * a.cos(), h + 20.0 - 5.0 * a.sin())
            })
            .collect();
        self.head(turn[22], turn[24], Stroke::new(1.2, self.text));
        self.painter
            .add(Shape::line(turn, Stroke::new(1.2, self.text)));
        // Pressure inside, and the radius along X.
        for y in [14.0, 35.0, 56.0] {
            self.arrow(self.at(4.0, y), self.at(inner, y), self.load);
        }
        self.arrow(
            self.at(0.0, -14.0),
            self.at(outer + 26.0, -14.0),
            Stroke::new(1.2, self.text),
        );
        self.label(
            self.at(outer + 30.0, -14.0),
            egui::Align2::LEFT_CENTER,
            "X = radius",
            self.text,
        );
        self.notes(&[
            "Model: one profile at X >= 0",
            "turned once about the Y axis",
            "Loads and supports alike all round",
        ]);
    }
}
