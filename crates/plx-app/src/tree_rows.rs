//! Trees drawn from data in the lists of dialogs: the frame around them and their rows.

/// The frame around a tree.
pub(crate) fn frame() -> egui::Frame {
    egui::Frame::new()
        .fill(crate::style::window())
        .stroke(egui::Stroke::new(1.0, crate::style::border()))
        .inner_margin(2)
}

/// Row of a tree drawn from data: indentation, expand button for nodes with children and a
/// selectable label, whose response is returned.
pub(crate) fn tree_row(
    ui: &mut egui::Ui,
    depth: usize,
    expanded: Option<&mut bool>,
    selected: bool,
    label: egui::RichText,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add_space(depth as f32 * 14.0);
        let size = egui::vec2(12.0, ui.spacing().interact_size.y);
        match expanded {
            Some(expanded) => {
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                if response.clicked() {
                    *expanded = !*expanded;
                }
                let icon = egui::Rect::from_center_size(rect.center(), egui::vec2(9.0, 9.0));
                let response = response.with_new_rect(icon);
                egui::collapsing_header::paint_default_icon(
                    ui,
                    if *expanded { 1.0 } else { 0.0 },
                    &response,
                );
            }
            None => {
                ui.add_space(size.x);
            }
        }
        let response =
            ui.add(egui::Button::selectable(selected, label).wrap_mode(egui::TextWrapMode::Extend));
        if selected && response.gained_focus() {
            response.scroll_to_me(None);
        }
        response
    })
    .inner
}
