//! Ids of windows, modals and areas. egui takes these ids as they are, and all
//! OS windows (the project and every analysis) share one egui context, so a
//! fixed id would make the same dialog open in two analyses share its scroll
//! offset, size and other state. These ids include the OS window.

use std::fmt::Debug;
use std::hash::Hash;

/// The id of a window, modal or area shown in the current OS window.
pub fn window(ctx: &egui::Context, salt: impl Hash + Debug) -> egui::Id {
    egui::Id::new((ctx.viewport_id(), salt))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("ids.rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path.display().to_string(), text));
            }
        }
    }

    /// Every window, modal and area takes its id from [`super::window`], so the same
    /// dialog in two analyses keeps its own state.
    #[test]
    fn windows_modals_and_areas_have_ids_per_os_window() {
        let mut files = Vec::new();
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let mut missing = Vec::new();
        for (file, text) in &files {
            let lines: Vec<_> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let opens = [
                    "egui::Window::new(",
                    "egui::Modal::new(",
                    "egui::Area::new(",
                ]
                .iter()
                .any(|p| line.contains(p));
                if !opens || line.trim_start().starts_with("//") {
                    continue;
                }
                let builder = lines[i..lines.len().min(i + 3)].join("\n");
                if !builder.contains("ids::window(") && !builder.contains(".id(id)") {
                    missing.push(format!("{file}:{}", i + 1));
                }
            }
        }
        assert!(missing.is_empty(), "ids without the OS window: {missing:?}");
    }

    fn first() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("analysis 1")
    }

    /// One pass of a window with a long list in the OS window `viewport`; the first
    /// OS window scrolls at step 2. Returns the scroll offset.
    fn pass(
        ctx: &egui::Context,
        viewport: egui::ViewportId,
        id: &dyn Fn(&egui::Context) -> egui::Id,
        step: u32,
    ) -> f32 {
        let mut input = egui::RawInput {
            viewport_id: viewport,
            time: Some(f64::from(step) / 60.0),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        input.viewports.insert(viewport, Default::default());
        let mut offset = 0.0;
        let mut output = ctx.run_ui(input, |ui| {
            egui::Window::new("Table")
                .id(id(ui.ctx()))
                .fixed_pos(egui::pos2(0.0, 0.0))
                .fixed_size(egui::vec2(400.0, 300.0))
                .show(ui.ctx(), |ui| {
                    let output = egui::ScrollArea::vertical().show(ui, |ui| {
                        for row in 0..200 {
                            ui.label(format!("Row {row}"));
                        }
                        if step == 2 && viewport == first() {
                            ui.scroll_with_delta(egui::vec2(0.0, -200.0));
                        }
                    });
                    offset = output.state.offset.y;
                });
        });
        output.textures_delta.clear();
        offset
    }

    /// Scrolls the window in the first of two OS windows; returns both offsets.
    fn scroll_first(id: &dyn Fn(&egui::Context) -> egui::Id) -> (f32, f32) {
        let ctx = egui::Context::default();
        let second = egui::ViewportId::from_hash_of("analysis 2");
        let mut offsets = (0.0, 0.0);
        for step in 0..30 {
            offsets.0 = pass(&ctx, first(), id, step);
            offsets.1 = pass(&ctx, second, id, step);
        }
        offsets
    }

    /// The bug this module fixes: with one fixed id, scrolling the window in one
    /// analysis scrolled the same window in another analysis.
    #[test]
    fn scrolling_one_os_window_leaves_the_other_alone() {
        let (shared_a, shared_b) = scroll_first(&|_| egui::Id::new("table"));
        assert!(shared_a > 0.0);
        assert_eq!(shared_a, shared_b, "a fixed id shares the scroll offset");
        let (a, b) = scroll_first(&|ctx| super::window(ctx, "table"));
        assert!(a > 0.0, "the scrolled window moves");
        assert_eq!(b, 0.0, "the other OS window keeps its offset");
    }
}
