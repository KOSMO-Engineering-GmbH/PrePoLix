//! Flat, single-colour symbols of the rows of a block in the project schematic, drawn with
//! strokes in the style of a node editor like Blender's: simple outlines in the text colour
//! that read at a glance without competing with the coloured pins and wires.

use crate::tree_icons::{self, StepGlyph, TreeIcon};
use egui::{Color32, Painter, Pos2, Shape, Stroke, pos2, vec2};

/// What a row of a block stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeIcon {
    /// Where the analysis starts: an arrow into a circle.
    State,
    /// The CAD geometry: a cube.
    Geometry,
    /// The mesh: a grid.
    Mesh,
    /// The model setup: sliders.
    Setup,
    /// Features such as reference points: a crosshair.
    Features,
    /// Materials: a shaded sphere.
    Materials,
    /// Sections: an I profile.
    Sections,
    /// Constraints: a support.
    Constraints,
    /// Contacts: two bodies touching.
    Contacts,
    /// Amplitudes: a ramp over time.
    Amplitudes,
    /// Initial conditions: a clock at the start.
    InitialConditions,
    /// The list of steps.
    Steps,
    /// One step: a play sign.
    Step,
    /// The CalculiX run: a gear.
    Solution,
    /// The results: bars.
    Results,
    /// Taken over from another analysis: two chain links.
    Link,
    /// A file written by an export block: an arrow out of a tray.
    Export,
    /// A step of this type, drawn like in the tree of the FE model.
    StepType(StepGlyph),
}

/// Paints `icon` into the 14 x 14 square at `min` in `color`.
pub fn paint(painter: &Painter, min: Pos2, icon: NodeIcon, color: Color32) {
    if let NodeIcon::StepType(glyph) = icon {
        tree_icons::paint(painter, min, TreeIcon::StepType(glyph));
        return;
    }
    let stroke = Stroke::new(1.2, color);
    let p = |x: f32, y: f32| pos2(min.x + x, min.y + y);
    let line = |points: &[Pos2]| {
        painter.add(Shape::line(points.to_vec(), stroke));
    };
    let closed = |points: &[Pos2]| {
        painter.add(Shape::closed_line(points.to_vec(), stroke));
    };
    let circle = |center: Pos2, radius: f32| {
        painter.circle_stroke(center, radius, stroke);
    };
    let dot = |center: Pos2, radius: f32| {
        painter.circle_filled(center, radius, color);
    };
    match icon {
        NodeIcon::StepType(_) => {}
        NodeIcon::Link => {
            let link = |x0: f32, y0: f32| {
                let points: Vec<Pos2> = [
                    (0.0, 0.0),
                    (4.5, 0.0),
                    (6.0, 1.5),
                    (6.0, 3.0),
                    (4.5, 4.5),
                    (0.0, 4.5),
                    (-1.5, 3.0),
                    (-1.5, 1.5),
                ]
                .iter()
                .map(|(x, y)| p(x0 + x, y0 + y))
                .collect();
                closed(&points);
            };
            link(1.5, 8.0);
            link(6.0, 1.5);
        }
        NodeIcon::Export => {
            line(&[p(1.5, 8.0), p(1.5, 12.5), p(12.5, 12.5), p(12.5, 8.0)]);
            line(&[p(7.0, 9.5), p(7.0, 1.5)]);
            line(&[p(4.2, 4.3), p(7.0, 1.5), p(9.8, 4.3)]);
        }
        NodeIcon::State => {
            line(&[p(0.5, 7.0), p(5.0, 7.0)]);
            line(&[p(3.0, 4.8), p(5.2, 7.0), p(3.0, 9.2)]);
            circle(p(9.5, 7.0), 3.3);
        }
        NodeIcon::Geometry => {
            closed(&[p(1.5, 4.5), p(9.0, 4.5), p(9.0, 12.0), p(1.5, 12.0)]);
            line(&[p(1.5, 4.5), p(5.0, 1.5), p(12.5, 1.5), p(9.0, 4.5)]);
            line(&[p(12.5, 1.5), p(12.5, 9.0), p(9.0, 12.0)]);
        }
        NodeIcon::Mesh => {
            closed(&[p(1.5, 1.5), p(12.5, 1.5), p(12.5, 12.5), p(1.5, 12.5)]);
            for v in [5.2, 8.8] {
                line(&[p(v, 1.5), p(v, 12.5)]);
                line(&[p(1.5, v), p(12.5, v)]);
            }
        }
        NodeIcon::Setup => {
            for (y, knob) in [(3.5, 9.0), (7.0, 4.5), (10.5, 8.0)] {
                line(&[p(1.5, y), p(12.5, y)]);
                dot(p(knob, y), 1.8);
            }
        }
        NodeIcon::Features => {
            circle(p(7.0, 7.0), 4.0);
            line(&[p(7.0, 0.8), p(7.0, 3.5)]);
            line(&[p(7.0, 10.5), p(7.0, 13.2)]);
            line(&[p(0.8, 7.0), p(3.5, 7.0)]);
            line(&[p(10.5, 7.0), p(13.2, 7.0)]);
            dot(p(7.0, 7.0), 1.1);
        }
        NodeIcon::Materials => {
            circle(p(7.0, 7.0), 5.5);
            let shade: Vec<Pos2> = (0..=8)
                .map(|i| {
                    let angle = (-10.0 + 110.0 * i as f32 / 8.0_f32).to_radians();
                    p(7.0 + 3.3 * angle.cos(), 7.0 + 3.3 * angle.sin())
                })
                .collect();
            line(&shade);
        }
        NodeIcon::Sections => closed(&[
            p(2.0, 2.0),
            p(12.0, 2.0),
            p(12.0, 4.5),
            p(8.5, 4.5),
            p(8.5, 9.5),
            p(12.0, 9.5),
            p(12.0, 12.0),
            p(2.0, 12.0),
            p(2.0, 9.5),
            p(5.5, 9.5),
            p(5.5, 4.5),
            p(2.0, 4.5),
        ]),
        NodeIcon::Constraints => {
            closed(&[p(7.0, 1.5), p(2.5, 9.0), p(11.5, 9.0)]);
            line(&[p(1.0, 11.0), p(13.0, 11.0)]);
            for x in [3.0, 6.0, 9.0] {
                line(&[p(x + 2.0, 11.0), p(x, 13.5)]);
            }
        }
        NodeIcon::Contacts => {
            closed(&[p(2.0, 1.5), p(12.0, 1.5), p(12.0, 6.0), p(2.0, 6.0)]);
            closed(&[p(2.0, 8.0), p(12.0, 8.0), p(12.0, 12.5), p(2.0, 12.5)]);
        }
        NodeIcon::Amplitudes => {
            line(&[p(2.0, 1.5), p(2.0, 12.5), p(13.0, 12.5)]);
            line(&[p(2.0, 10.5), p(5.5, 10.5), p(9.0, 3.5), p(13.0, 3.5)]);
        }
        NodeIcon::InitialConditions => {
            circle(p(7.0, 7.0), 5.5);
            line(&[p(7.0, 3.5), p(7.0, 7.0), p(10.0, 7.0)]);
        }
        NodeIcon::Steps => {
            for y in [3.5, 7.0, 10.5] {
                dot(p(3.0, y), 1.2);
                line(&[p(6.0, y), p(12.5, y)]);
            }
        }
        NodeIcon::Step => closed(&[p(4.0, 2.5), p(11.5, 7.0), p(4.0, 11.5)]),
        NodeIcon::Solution => {
            let gear: Vec<Pos2> = (0..16)
                .map(|i| {
                    let angle = std::f32::consts::TAU * i as f32 / 16.0;
                    let radius = if i % 2 == 0 { 6.0 } else { 4.3 };
                    p(7.0, 7.0) + radius * vec2(angle.cos(), angle.sin())
                })
                .collect();
            closed(&gear);
            circle(p(7.0, 7.0), 1.8);
        }
        NodeIcon::Results => {
            line(&[p(1.5, 12.5), p(12.5, 12.5)]);
            for (x, height) in [(3.0, 5.0), (6.5, 9.0), (10.0, 7.0)] {
                painter.rect_filled(
                    egui::Rect::from_min_size(p(x, 12.5 - height), vec2(2.5, height)),
                    0.5,
                    color,
                );
            }
        }
    }
}
