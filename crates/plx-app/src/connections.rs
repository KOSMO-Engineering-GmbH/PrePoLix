//! The forms of contact regions and of the connections they share: the user picks two sides
//! and a type, and everything CalculiX needs besides is derived ("Auto"), shown and can be
//! overridden one setting at a time. Side A shows in the primary, side B in the secondary
//! highlight colour.

use egui::{RichText, Ui};
use plx_model::{
    BondedMethod, ContactBehavior, ContactCheck, ContactDefaults, ContactDetection, ContactMethod,
    ContactRegion, ContactStatus, DetectionGrouping, FeModel, InterfaceTreatment, Quantity,
    Resolved, ResolvedContacts, ResolvedKind, STORED, SideRoles, Stiffness, UnitSystem,
    auto_detection_tolerance,
};

use crate::contacts::MasterSlave;
use crate::model::Model;
use crate::numeric;
use crate::tree::{TreeItem, TreeState, TreeView};

/// What the region dialog keeps besides the region: whether the advanced settings show and
/// the derivation of the region as last edited, computed again only when it changes.
#[derive(Clone, Debug, Default)]
pub struct RegionView {
    advanced: bool,
    /// Boxed, as the dialog holds it by value.
    resolved: Option<Box<(ContactRegion, ContactDefaults, Resolved)>>,
    /// The contact check of the region as last edited.
    check: Option<Box<(ContactRegion, ContactDefaults, Option<ContactCheck>)>>,
}

impl RegionView {
    fn resolved(&mut self, model: &Model, region: &ContactRegion) -> &Resolved {
        let defaults = &model.fe.contact_defaults;
        let stale =
            !matches!(self.resolved.as_deref(), Some((r, d, _)) if r == region && d == defaults);
        if stale {
            let resolved = model.fe.resolve_contact(region, &model.mesh);
            self.resolved = Some(Box::new((region.clone(), defaults.clone(), resolved)));
        }
        &self.resolved.as_ref().expect("resolved above").2
    }

    fn check(&mut self, model: &Model, region: &ContactRegion) -> Option<&ContactCheck> {
        let defaults = &model.fe.contact_defaults;
        let stale =
            !matches!(self.check.as_deref(), Some((r, d, _)) if r == region && d == defaults);
        if stale {
            let check = model.fe.check_contact(region, &model.mesh);
            self.check = Some(Box::new((region.clone(), defaults.clone(), check)));
        }
        self.check.as_ref().expect("checked above").2.as_ref()
    }
}

/// A value of a quantity in the model's units with the unit, e.g. "2.1e5 N/mm³".
pub fn with_unit(value: f64, units: UnitSystem, quantity: Quantity) -> String {
    let shown = STORED.convert(value, quantity, units);
    let unit = units.unit(quantity);
    let number = if shown != 0.0 && !(1e-2..1e5).contains(&shown.abs()) {
        format!("{shown:.3e}")
    } else {
        format!("{}", (shown * 1e4).round() / 1e4)
    };
    if unit.is_empty() {
        number
    } else {
        format!("{number} {unit}")
    }
}

/// The contact region dialog below the name: type, sides, advanced settings and what is
/// exported.
pub fn region_form(
    ui: &mut Ui,
    model: &Model,
    region: &mut ContactRegion,
    sides: &mut MasterSlave,
    view: &mut RegionView,
) {
    (region.side_a, region.side_b) = sides.regions();
    behavior_rows(ui, "contact region type", "Type", &mut region.behavior);
    sides.ui_named(ui, model, ["Side A", "Side B"]);
    (region.side_a, region.side_b) = sides.regions();
    ui.label("Side A color");
    ui.color_edit_button_srgb(&mut region.color_a);
    ui.end_row();
    ui.label("Side B color");
    ui.color_edit_button_srgb(&mut region.color_b);
    ui.end_row();
    let resolved = view.resolved(model, region).clone();
    ui.label("");
    ui.toggle_value(&mut view.advanced, "Advanced settings")
        .on_hover_text("Settings that are derived (Auto) unless you choose them.");
    ui.end_row();
    if view.advanced {
        advanced_rows(ui, model, region, &resolved);
    }
    ui.label("Exported as");
    ui.add(egui::Label::new(exported_as(model, &resolved)).wrap());
    ui.end_row();
    if !resolved.why.is_empty() {
        ui.label("");
        let text = resolved.why.join("\n");
        ui.add(egui::Label::new(RichText::new(text).weak()).wrap());
        ui.end_row();
    }
    if let Some(check) = view.check(model, region) {
        ui.label("Contact check").on_hover_text(
            "How the sides lie to each other in the mesh before the analysis: closed where \
             the slave nodes are within the tolerance of the master side.",
        );
        ui.vertical(|ui| {
            ui.label(status_text(check));
            let text = check_summary(check, model.fe.properties.units);
            ui.add(egui::Label::new(RichText::new(text).weak()).wrap());
        });
        ui.end_row();
    }
}

/// The type of a region with its friction coefficient and what it means.
fn behavior_rows(ui: &mut Ui, id: &str, label: &str, behavior: &mut ContactBehavior) {
    ui.label(label);
    egui::ComboBox::from_id_salt(id)
        .selected_text(behavior.name())
        .width(200.0)
        .show_ui(ui, |ui| {
            for choice in ContactBehavior::ALL {
                let selected = choice.name() == behavior.name();
                let response = ui.selectable_label(selected, choice.name());
                if response.on_hover_text(choice.summary()).clicked() && !selected {
                    *behavior = choice;
                }
            }
        });
    ui.end_row();
    if let ContactBehavior::Frictional { mu } = behavior {
        ui.label("Friction coefficient");
        ui.add(numeric::drag_value(mu).speed(0.01).range(0.0..=5.0));
        ui.end_row();
    }
    ui.label("");
    ui.add(egui::Label::new(RichText::new(behavior.summary()).weak()).wrap());
    ui.end_row();
}

/// The contact detection dialog below the name: which parts it searches, how close and
/// how parallel surfaces must be to touch, and the type of the regions it finds.
pub fn detection_form(ui: &mut Ui, model: &Model, detection: &mut ContactDetection) {
    let units = model.fe.properties.units;
    ui.label("Parts");
    ui.vertical(|ui| {
        let mut all = detection.parts.is_empty();
        if ui
            .checkbox(&mut all, "All visible parts")
            .on_hover_text("Untick to choose the parts to search.")
            .changed()
        {
            detection.parts = if all {
                Vec::new()
            } else {
                (model.parts.iter())
                    .filter(|p| p.visible)
                    .map(|p| p.name.clone())
                    .collect()
            };
        }
        if !all {
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .id_salt("detection parts")
                .show(ui, |ui| {
                    for part in &model.mesh.parts {
                        let mut on = detection.parts.contains(&part.name);
                        if ui.checkbox(&mut on, &part.name).changed() {
                            if on {
                                detection.parts.push(part.name.clone());
                            } else {
                                detection.parts.retain(|p| *p != part.name);
                            }
                        }
                    }
                });
        }
    });
    ui.end_row();
    ui.label("Tolerance").on_hover_text(
        "Largest gap between surfaces that touch. Auto: 0.5 % of the size of the parts \
         searched, at least half their element size.",
    );
    ui.horizontal(|ui| {
        let auto = auto_tolerance(ui, model, &detection.parts);
        let mut set = detection.tolerance.is_some();
        ui.checkbox(&mut set, "").on_hover_text(format!(
            "Untick for Auto ({})",
            with_unit(auto, units, Quantity::Length)
        ));
        if set {
            let mut value = detection.tolerance.unwrap_or(auto);
            ui.add(numeric::physical(&mut value, units, Quantity::Length));
            detection.tolerance = Some(value);
        } else {
            detection.tolerance = None;
            ui.weak(format!(
                "Auto ({})",
                with_unit(auto, units, Quantity::Length)
            ));
        }
    });
    ui.end_row();
    ui.label("Angle").on_hover_text(
        "Largest angle between the normals of surfaces facing each other, in degrees.",
    );
    ui.add(
        numeric::drag_value(&mut detection.angle_deg)
            .speed(1.0)
            .range(0.0..=90.0)
            .suffix(" °"),
    );
    ui.end_row();
    if !model.fe.properties.space.is_2d() {
        ui.label("Surfaces");
        ui.horizontal(|ui| {
            let filter = &mut detection.filter;
            ui.checkbox(&mut filter.solid, "Solid")
                .on_hover_text("Outer faces of solids.");
            ui.checkbox(&mut filter.shell, "Shell")
                .on_hover_text("Top and bottom of shells and membranes.");
            ui.checkbox(&mut filter.shell_edge, "Shell edge")
                .on_hover_text("Free edges of shells and membranes.");
        });
        ui.end_row();
    }
    ui.label("Group by");
    egui::ComboBox::from_id_salt("detection grouping")
        .selected_text(detection.grouping.name())
        .show_ui(ui, |ui| {
            for (grouping, hint) in [
                (
                    DetectionGrouping::Parts,
                    "One region per pair of touching parts.",
                ),
                (
                    DetectionGrouping::Faces,
                    "One region per pair of touching faces.",
                ),
                (
                    DetectionGrouping::Graph,
                    "Touching faces sharing nodes are merged, so that every node is on the \
                     slave side of one region only.",
                ),
            ] {
                ui.selectable_value(&mut detection.grouping, grouping, grouping.name())
                    .on_hover_text(hint);
            }
        });
    ui.end_row();
    behavior_rows(ui, "detection type", "Type", &mut detection.behavior);
    ui.label("");
    ui.add(
        egui::Label::new(
            RichText::new(
                "OK searches the parts and replaces the regions found before, except those \
                 you changed since.",
            )
            .weak(),
        )
        .wrap(),
    );
    ui.end_row();
}

/// The derived tolerance of a detection of `parts`, kept while they and the mesh stay the
/// same: it walks the skins of all parts searched.
fn auto_tolerance(ui: &Ui, model: &Model, parts: &[String]) -> f64 {
    let searched: Vec<bool> = (model.mesh.parts.iter().zip(&model.parts))
        .map(|(part, shown)| {
            if parts.is_empty() {
                shown.visible
            } else {
                parts.contains(&part.name)
            }
        })
        .collect();
    let key = (
        searched.clone(),
        model.mesh.coords().len(),
        model.mesh.elements().len(),
    );
    let id = egui::Id::new("detection auto tolerance");
    let cached: Option<(Vec<bool>, usize, usize, f64)> = ui.data(|d| d.get_temp(id));
    if let Some((s, n, e, value)) = cached
        && (s, n, e) == key
    {
        return value;
    }
    let value = auto_detection_tolerance(&model.mesh, model.skins(), &searched);
    ui.data_mut(|d| d.insert_temp(id, (key.0, key.1, key.2, value)));
    value
}

/// Why the detection cannot be applied as it is.
pub fn validate_detection(detection: &ContactDetection) -> Result<(), String> {
    if detection
        .tolerance
        .is_some_and(|t| !(t.is_finite() && t > 0.0))
    {
        return Err("The tolerance must be positive.".into());
    }
    let filter = detection.filter;
    if !(filter.solid || filter.shell || filter.shell_edge) {
        return Err("Choose at least one kind of surface to search.".into());
    }
    if let ContactBehavior::Frictional { mu } = detection.behavior
        && !(mu.is_finite() && mu >= 0.0)
    {
        return Err("The friction coefficient must not be negative.".into());
    }
    Ok(())
}

/// One line on what the region becomes, e.g. "tie constraint (linear), slave: side B,
/// tolerance 1.25 mm".
fn exported_as(model: &Model, resolved: &Resolved) -> String {
    let units = model.fe.properties.units;
    let mut text = format!(
        "{}, master: side {}",
        resolved.kind.describe(),
        if resolved.a_is_master { "A" } else { "B" }
    );
    if let Some(k) = resolved.normal_stiffness() {
        text += &format!(
            ", stiffness {}",
            with_unit(k, units, Quantity::ForcePerVolume)
        );
    }
    if let ResolvedKind::Tie {
        tolerance: Some(t), ..
    } = resolved.kind
    {
        text += &format!(", tolerance {}", with_unit(t, units, Quantity::Length));
    }
    if resolved.adjusts() {
        text += ", adjusted to touch";
    }
    text
}

/// A choice that is derived unless the user picks one of `options`.
fn auto_combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    label: &str,
    value: &mut Option<T>,
    auto: String,
    options: &[(T, &str)],
) {
    ui.label(label);
    let text = match *value {
        None => format!("Auto ({auto})"),
        Some(v) => (options.iter())
            .find(|(o, _)| *o == v)
            .map_or(String::new(), |(_, name)| (*name).to_owned()),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(200.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, format!("Auto ({auto})"));
            for &(option, name) in options {
                ui.selectable_value(value, Some(option), name);
            }
        });
    ui.end_row();
}

/// A stiffness that is derived, a factor on E / h or a value.
fn stiffness_row(
    ui: &mut Ui,
    id: &str,
    label: &str,
    value: &mut Option<Stiffness>,
    auto: Option<f64>,
    units: UnitSystem,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        let auto_text = auto.map_or("CalculiX".into(), |k| {
            with_unit(k, units, Quantity::ForcePerVolume)
        });
        let kind = match value {
            None => 0,
            Some(Stiffness::Factor(_)) => 1,
            Some(Stiffness::Value(_)) => 2,
        };
        let names = [
            format!("Auto ({auto_text})"),
            "Factor of E/h".into(),
            "Value".into(),
        ];
        let mut chosen = kind;
        egui::ComboBox::from_id_salt(id)
            .selected_text(names[kind].clone())
            .width(170.0)
            .show_ui(ui, |ui| {
                for (k, name) in names.iter().enumerate() {
                    ui.selectable_value(&mut chosen, k, name);
                }
            });
        if chosen != kind {
            *value = match chosen {
                1 => Some(Stiffness::Factor(plx_model::STIFFNESS_FACTOR)),
                2 => Some(Stiffness::Value(auto.unwrap_or(1e12))),
                _ => None,
            };
        }
        match value {
            Some(Stiffness::Factor(f)) => {
                ui.add(numeric::drag_value(f).speed(0.5).range(1e-6..=1e6));
            }
            Some(Stiffness::Value(k)) => {
                ui.add(numeric::physical(k, units, Quantity::ForcePerVolume));
            }
            None => {}
        }
    });
    ui.end_row();
}

fn advanced_rows(ui: &mut Ui, model: &Model, region: &mut ContactRegion, resolved: &Resolved) {
    let units = model.fe.properties.units;
    let settings = &mut region.settings;
    let master = if resolved.a_is_master {
        "side A"
    } else {
        "side B"
    };
    auto_combo(
        ui,
        "region master",
        "Master",
        &mut settings.sides,
        master.into(),
        &[
            (SideRoles::AIsMaster, "Side A"),
            (SideRoles::BIsMaster, "Side B"),
        ],
    );
    let tie = matches!(resolved.kind, ResolvedKind::Tie { .. });
    if region.behavior == ContactBehavior::Bonded {
        let auto = if tie { "constraint" } else { "contact" };
        auto_combo(
            ui,
            "region bonded",
            "Bonded as",
            &mut settings.bonded_method,
            auto.into(),
            &BondedMethod::ALL.map(|m| (m, m.name())),
        );
    }
    if !tie {
        stiffness_row(
            ui,
            "region normal",
            "Normal stiffness",
            &mut settings.normal_stiffness,
            resolved.normal_stiffness(),
            units,
        );
        let friction = matches!(
            &resolved.kind,
            ResolvedKind::Pair {
                friction: Some(_),
                ..
            }
        );
        if friction {
            stiffness_row(
                ui,
                "region tangential",
                "Tangential stiffness",
                &mut settings.tangential_stiffness,
                resolved.tangential_stiffness(),
                units,
            );
        }
    }
    let auto = if resolved.adjusts() {
        "adjust to touch"
    } else {
        "as is"
    };
    auto_combo(
        ui,
        "region interface",
        "Initial gap",
        &mut settings.interface,
        auto.into(),
        &InterfaceTreatment::ALL.map(|t| (t, t.name())),
    );
    ui.label("Tolerance").on_hover_text(
        "Distance within which the sides count as touching: tied by a tie, closed by \
         the adjustment.",
    );
    ui.horizontal(|ui| {
        let mut set = settings.tolerance.is_some();
        let auto = resolved
            .tolerance()
            .or(resolved.mesh_size.map(|h| 0.25 * h));
        let auto_text = auto.map_or("CalculiX".into(), |t| with_unit(t, units, Quantity::Length));
        ui.checkbox(&mut set, "")
            .on_hover_text(format!("Untick for Auto ({auto_text})"));
        if set {
            let mut value = settings.tolerance.or(auto).unwrap_or(0.0);
            ui.add(numeric::physical(&mut value, units, Quantity::Length));
            settings.tolerance = Some(value);
        } else {
            settings.tolerance = None;
            ui.weak(format!("Auto ({auto_text})"));
        }
    });
    ui.end_row();
    if model.fe.contact_formulation().0 == ContactMethod::NodeToSurface && !tie {
        let auto = match &resolved.kind {
            ResolvedKind::Pair { small_sliding, .. } if *small_sliding => "on",
            _ => "off",
        };
        auto_combo(
            ui,
            "region small sliding",
            "Small sliding",
            &mut settings.small_sliding,
            auto.into(),
            &[(true, "On"), (false, "Off")],
        );
    }
}

/// The settings all contact regions share, at the Connections of the tree.
pub fn connections_form(ui: &mut Ui, fe: &FeModel, defaults: &mut ContactDefaults) {
    let mut derived = fe.clone();
    derived.contact_defaults.formulation = None;
    let (auto, reason) = derived.contact_formulation();
    ui.label("Formulation");
    egui::ComboBox::from_id_salt("connections formulation")
        .selected_text(match defaults.formulation {
            None => format!("Auto ({})", auto.name()),
            Some(m) => m.name().to_owned(),
        })
        .width(220.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut defaults.formulation,
                None,
                format!("Auto ({})", auto.name()),
            )
            .on_hover_text(reason);
            for method in [
                ContactMethod::SurfaceToSurface,
                ContactMethod::NodeToSurface,
                ContactMethod::Mortar,
            ] {
                ui.selectable_value(&mut defaults.formulation, Some(method), method.name())
                    .on_hover_text(method.summary());
            }
        });
    ui.end_row();
    ui.label("");
    let description = defaults.formulation.unwrap_or(auto).description();
    ui.add(
        egui::Label::new(
            RichText::new(format!(
                "{description}\n\nCalculiX takes one formulation for all contacts of a model. \
             Bonded regions written as ties are not affected."
            ))
            .weak(),
        )
        .wrap(),
    );
    ui.end_row();
    ui.label("Stiffness factor").on_hover_text(
        "Normal stiffness of the contact regions as a factor on E / h, E the Young's \
         modulus of the softer side and h its mesh size. Larger is stiffer: less \
         penetration, but harder to converge.",
    );
    ui.add(
        numeric::drag_value(&mut defaults.stiffness_factor)
            .speed(0.5)
            .range(1e-3..=1e4),
    );
    ui.end_row();
}

/// Why the region cannot be applied as it is.
pub fn validate_region(region: &ContactRegion, sides: &MasterSlave) -> Result<(), String> {
    sides.validate_named(["Side A", "Side B"])?;
    if let ContactBehavior::Frictional { mu } = region.behavior
        && !(mu.is_finite() && mu >= 0.0)
    {
        return Err("The friction coefficient must not be negative.".into());
    }
    for stiffness in [
        region.settings.normal_stiffness,
        region.settings.tangential_stiffness,
    ]
    .into_iter()
    .flatten()
    {
        let (Stiffness::Factor(v) | Stiffness::Value(v)) = stiffness;
        if !(v.is_finite() && v > 0.0) {
            return Err("Stiffnesses must be greater than 0.".into());
        }
    }
    if region
        .settings
        .tolerance
        .is_some_and(|t| !(t.is_finite() && t >= 0.0))
    {
        return Err("The tolerance must not be negative.".into());
    }
    Ok(())
}

/// The derivation of all contact regions of the model, kept until the model changes.
fn resolved_contacts(ctx: &egui::Context, model: &Model) -> std::sync::Arc<ResolvedContacts> {
    type Cache = std::sync::Arc<(FeModel, std::sync::Arc<ResolvedContacts>)>;
    let id = egui::Id::new("contact worksheet resolved");
    let cached: Option<Cache> = ctx.data(|d| d.get_temp(id));
    if let Some(cache) = cached
        && cache.0 == model.fe
    {
        return cache.1.clone();
    }
    let resolved = std::sync::Arc::new(model.fe.resolve_contacts(&model.mesh));
    let cache: Cache = std::sync::Arc::new((model.fe.clone(), resolved.clone()));
    ctx.data_mut(|d| d.insert_temp(id, cache));
    resolved
}

/// The contact check of all regions, computed again when the model or the mesh changes.
fn contact_checks(ctx: &egui::Context, model: &Model) -> std::sync::Arc<Vec<Option<ContactCheck>>> {
    type Key = (FeModel, usize, usize);
    type Cache = std::sync::Arc<(Key, std::sync::Arc<Vec<Option<ContactCheck>>>)>;
    let id = egui::Id::new("contact worksheet checks");
    let key = (
        model.fe.clone(),
        model.mesh.coords().len(),
        model.mesh.elements().len(),
    );
    let cached: Option<Cache> = ctx.data(|d| d.get_temp(id));
    if let Some(cache) = cached
        && cache.0 == key
    {
        return cache.1.clone();
    }
    let checks = std::sync::Arc::new(model.fe.check_contacts(&model.mesh));
    let cache: Cache = std::sync::Arc::new((key, checks.clone()));
    ctx.data_mut(|d| d.insert_temp(id, cache));
    checks
}

/// The status of a contact check in its colour: green closed, yellow partly closed or near
/// open, red far open.
pub fn status_text(check: &ContactCheck) -> RichText {
    let color = match check.status {
        ContactStatus::Closed => egui::Color32::from_rgb(0, 150, 60),
        ContactStatus::PartlyClosed | ContactStatus::NearOpen => {
            egui::Color32::from_rgb(200, 140, 0)
        }
        ContactStatus::FarOpen => crate::style::error(),
    };
    RichText::new(check.status.name()).color(color)
}

/// Gap and overlap of a contact check in a few words, e.g. "gap 0.01 mm".
pub fn gap_text(check: &ContactCheck, units: UnitSystem) -> String {
    let length = |v: f64| with_unit(v, units, Quantity::Length);
    // Below a thousandth of the tolerance is rounding of the coordinates.
    let noise = 1e-3 * check.tolerance;
    let mut parts = Vec::new();
    if let Some(p) = check.max_penetration.filter(|&p| p > noise) {
        parts.push(format!("overlap {}", length(p)));
    }
    match (check.max_gap, check.min_gap) {
        (Some(g), _) if g > noise => parts.push(format!("gap up to {}", length(g))),
        (_, Some(g)) if check.touching == 0 => parts.push(format!("gap {}", length(g))),
        _ => {}
    }
    parts.join(", ")
}

/// The contact check in a sentence for the region dialog and the worksheet's tooltip.
pub fn check_summary(check: &ContactCheck, units: UnitSystem) -> String {
    let mut text = format!(
        "{} of {} slave nodes within {} of the master side",
        check.touching,
        check.slave_nodes,
        with_unit(check.tolerance, units, Quantity::Length)
    );
    let gap = gap_text(check, units);
    if !gap.is_empty() {
        text += &format!("; {gap}");
    }
    if check.status == ContactStatus::FarOpen {
        text += "; nothing within ten times that";
    }
    text
}

/// What CalculiX found in a contact region, from the contact results (`CONTACT`) of the
/// last increment that has them, at the slave nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct SolvedContact {
    pub slave_nodes: usize,
    /// Slave nodes with contact pressure, and of those the ones that slide: whose shear
    /// reaches the friction coefficient times the pressure.
    pub in_contact: usize,
    pub sliding: usize,
    /// Largest contact pressure and largest opening, in the units of the results.
    pub max_pressure: f32,
    pub max_open: f32,
}

impl SolvedContact {
    /// "Sticking", "Sliding", "Sticking and sliding" or "Open".
    pub fn status(&self) -> &'static str {
        match (self.in_contact, self.sliding) {
            (0, _) => "Open",
            (_, 0) => "Sticking",
            (n, s) if s == n => "Sliding",
            _ => "Sticking and sliding",
        }
    }

    pub fn summary(&self, units: UnitSystem) -> String {
        let unit = crate::results::field_unit("CONTACT", "CPRESS", units).unwrap_or_default();
        let length = crate::results::field_unit("CONTACT", "COPEN", units).unwrap_or_default();
        let mut text = format!(
            "{}: {} of {} slave nodes in contact",
            self.status(),
            self.in_contact,
            self.slave_nodes
        );
        if self.sliding > 0 {
            text += &format!(", {} sliding", self.sliding);
        }
        if self.in_contact > 0 {
            text += &format!(
                ", max pressure {} {unit}",
                crate::results::format_value(self.max_pressure)
            );
        }
        if self.max_open > 0.0 {
            text += &format!(
                ", max opening {} {length}",
                crate::results::format_value(self.max_open)
            );
        }
        text
    }
}

/// The contact results of every region of `fe` in `results`; `None` for a region written
/// as a tie, which has none, or where the results hold no contact field.
pub fn solved_contacts(
    fe: &FeModel,
    mesh: &plx_mesh::FeMesh,
    resolved: &ResolvedContacts,
    results: &Model,
) -> Vec<Option<SolvedContact>> {
    let field = (results.results.as_ref()).and_then(|view| {
        view.increments
            .iter()
            .rev()
            .find_map(|i| i.field("CONTACT"))
    });
    let component = |name: &str| field.and_then(|f| f.component(name)).map(|c| &c.values);
    let (pressure, open) = (component("CPRESS"), component("COPEN"));
    let shear = [component("CSHEAR1"), component("CSHEAR2")];
    (fe.contact_regions.iter().zip(&resolved.items))
        .map(|(region, resolved)| {
            let ResolvedKind::Pair { friction, .. } = &resolved.kind else {
                return None;
            };
            let pressure = pressure?;
            let slave = if resolved.a_is_master {
                &region.side_b
            } else {
                &region.side_a
            };
            let mu = friction.as_ref().map(|f| f.coefficient as f32);
            let mut solved = SolvedContact {
                slave_nodes: 0,
                in_contact: 0,
                sliding: 0,
                max_pressure: 0.0,
                max_open: 0.0,
            };
            for id in slave.nodes(mesh) {
                let Some(index) = results.mesh.node_index(id) else {
                    continue;
                };
                solved.slave_nodes += 1;
                let value = |values: Option<&Vec<f32>>| {
                    values
                        .and_then(|v| v.get(index).copied())
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0)
                };
                let p = value(Some(pressure));
                if p > 0.0 {
                    solved.in_contact += 1;
                    solved.max_pressure = solved.max_pressure.max(p);
                    let tau = value(shear[0]).hypot(value(shear[1]));
                    if mu.is_some_and(|mu| tau >= 0.98 * mu * p) {
                        solved.sliding += 1;
                    }
                } else {
                    solved.max_open = solved.max_open.max(value(open));
                }
            }
            Some(solved)
        })
        .collect()
}

/// What the worksheet does to the selected regions.
enum Change {
    Behavior(ContactBehavior),
    Friction(f64),
    Flip,
    Active(bool),
    Rename,
    Delete,
}

/// Ansys' contact worksheet: a row per contact region with its type, sides and what it is
/// exported as. Clicking rows selects the regions in the tree (Ctrl and Shift for several),
/// which shows them in the 3D view; the buttons change all selected regions at once.
pub fn worksheet(
    ctx: &egui::Context,
    open: &mut bool,
    model: &mut Model,
    results: Option<&Model>,
    tree: &mut TreeState,
) {
    let view = TreeView::FeModel;
    let rows: Vec<TreeItem> = (0..model.fe.contact_regions.len())
        .map(TreeItem::ContactRegion)
        .collect();
    let selected: Vec<usize> = (tree.selected_items(view).into_iter())
        .filter_map(|item| match item {
            TreeItem::ContactRegion(i) if !tree.linked.contains_key(&item) => Some(i),
            _ => None,
        })
        .collect();
    let resolved = resolved_contacts(ctx, model);
    let checks = contact_checks(ctx, model);
    let solved = results.map(|r| solved_contacts(&model.fe, &model.mesh, &resolved, r));
    let units = model.fe.properties.units;
    let mut change = None;
    let mu_id = egui::Id::new("contact worksheet mu");
    let mut mu: f64 = ctx.data(|d| d.get_temp(mu_id)).unwrap_or(0.15);
    egui::Window::new("Contact Worksheet")
        .id(crate::ids::window(ctx, "contact worksheet"))
        .open(open)
        .default_width(900.0)
        .default_height(360.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(format!(
                "Formulation of all contacts: {}",
                resolved.formulation_reason
            ));
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(!selected.is_empty(), |ui| {
                    ui.label(format!("{} selected:", selected.len()));
                    ui.menu_button("Set Type", |ui| {
                        for behavior in ContactBehavior::ALL {
                            if ui.button(behavior.name()).clicked() {
                                change = Some(Change::Behavior(behavior));
                                ui.close();
                            }
                        }
                    });
                    ui.add(numeric::drag_value(&mut mu).speed(0.01).range(0.0..=5.0));
                    if ui
                        .button("Set Friction")
                        .on_hover_text(
                            "Makes the selected regions frictional with this coefficient.",
                        )
                        .clicked()
                    {
                        change = Some(Change::Friction(mu));
                    }
                    if ui.button("Flip Sides").clicked() {
                        change = Some(Change::Flip);
                    }
                    if ui.button("Activate").clicked() {
                        change = Some(Change::Active(true));
                    }
                    if ui.button("Deactivate").clicked() {
                        change = Some(Change::Active(false));
                    }
                    if ui
                        .button("Name from Definition")
                        .on_hover_text(
                            "Names the regions by their type and the parts of their sides.",
                        )
                        .clicked()
                    {
                        change = Some(Change::Rename);
                    }
                    if ui.button("Delete").clicked() {
                        change = Some(Change::Delete);
                    }
                });
            });
            ui.separator();
            if rows.is_empty() {
                ui.weak("No contact regions. Create one in the Interaction menu.");
                return;
            }
            egui::ScrollArea::both()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("contact worksheet grid")
                        .striped(true)
                        .num_columns(if solved.is_some() { 12 } else { 11 })
                        .spacing([14.0, 4.0])
                        .show(ui, |ui| {
                            let after = solved.as_ref().map(|_| "After solving");
                            for title in [
                                "Name",
                                "Detection",
                                "Type",
                                "Side A",
                                "Side B",
                                "Master",
                                "Initial status",
                                "Touching",
                                "Gap",
                                "Exported as",
                                "Active",
                            ]
                            .into_iter()
                            .chain(after)
                            {
                                ui.strong(title);
                            }
                            ui.end_row();
                            for (i, region) in model.fe.contact_regions.iter().enumerate() {
                                let item = TreeItem::ContactRegion(i);
                                let is_selected = tree.selected_items(view).contains(&item);
                                let response = ui.selectable_label(is_selected, &region.name);
                                if response.clicked() {
                                    let modifiers = ui.input(|i| i.modifiers);
                                    if modifiers.command || modifiers.shift {
                                        tree.select_sibling(
                                            view,
                                            item.clone(),
                                            &rows,
                                            modifiers.command,
                                            modifiers.shift,
                                        );
                                    } else {
                                        tree.select_item(view, item.clone());
                                    }
                                }
                                if response.double_clicked() {
                                    tree.reveal = true;
                                }
                                match &region.detected_by {
                                    Some(name) if region.edited => {
                                        ui.label(format!("{name}, changed")).on_hover_text(
                                            "Changed by you: detecting again keeps it.",
                                        );
                                    }
                                    Some(name) => {
                                        ui.label(name);
                                    }
                                    None => {
                                        ui.weak("by hand");
                                    }
                                }
                                let behavior = match region.behavior {
                                    ContactBehavior::Frictional { mu } => {
                                        format!("Frictional ({mu})")
                                    }
                                    other => other.name().to_owned(),
                                };
                                ui.label(behavior);
                                ui.label(region.side_a.describe());
                                ui.label(region.side_b.describe());
                                let master = resolved.items.get(i).map(|r| r.a_is_master);
                                ui.label(match master {
                                    Some(true) => "A",
                                    Some(false) => "B",
                                    None => "",
                                });
                                match checks.get(i).and_then(Option::as_ref) {
                                    Some(check) => {
                                        let summary = check_summary(check, units);
                                        ui.label(status_text(check)).on_hover_text(&summary);
                                        ui.label(format!(
                                            "{} %",
                                            (100.0 * check.touching_share()).round()
                                        ))
                                        .on_hover_text(&summary);
                                        ui.label(gap_text(check, units));
                                    }
                                    None => {
                                        ui.weak("-")
                                            .on_hover_text("The sides have no element faces.");
                                        ui.label("");
                                        ui.label("");
                                    }
                                }
                                match resolved.items.get(i) {
                                    Some(r) => {
                                        ui.label(r.kind.describe())
                                            .on_hover_text(exported_hover(r, units));
                                    }
                                    None => {
                                        ui.label("");
                                    }
                                }
                                ui.label(if region.active { "Yes" } else { "No" });
                                if let Some(solved) = &solved {
                                    match solved.get(i).and_then(Option::as_ref) {
                                        Some(s) => {
                                            ui.label(s.status()).on_hover_text(s.summary(units));
                                        }
                                        None => {
                                            ui.weak("-").on_hover_text(
                                                "A tie, or no contact results in the results \
                                                 file.",
                                            );
                                        }
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
        });
    ctx.data_mut(|d| d.insert_temp(mu_id, mu));
    let Some(change) = change else {
        return;
    };
    let fe = &mut model.fe;
    if let Change::Delete = change {
        let mut indices = selected;
        indices.sort_unstable();
        for i in indices.into_iter().rev() {
            if i < fe.contact_regions.len() {
                fe.contact_regions.remove(i);
            }
        }
        tree.selected = None;
        return;
    }
    for &i in &selected {
        let Some(region) = fe.contact_regions.get_mut(i) else {
            continue;
        };
        match change {
            Change::Behavior(behavior) => {
                // Frictional keeps its coefficient.
                if behavior.name() != region.behavior.name() {
                    region.behavior = behavior;
                }
            }
            Change::Friction(mu) => region.behavior = ContactBehavior::Frictional { mu },
            Change::Flip => region.flip(),
            Change::Active(active) => region.active = active,
            Change::Rename => region.name = region.name_from_definition(&model.mesh),
            Change::Delete => {}
        }
        // A detected region the user changed is kept when detecting again.
        region.edited |= region.detected_by.is_some();
    }
    if let Change::Rename = change {
        // Names stay unique: a repeated one gets a number.
        for i in 0..fe.contact_regions.len() {
            let name = fe.contact_regions[i].name.clone();
            let taken =
                (fe.contact_regions[..i].iter()).any(|r| r.name.eq_ignore_ascii_case(&name));
            if taken {
                let others = fe.contact_regions.iter().map(|r| r.name.as_str());
                fe.contact_regions[i].name = plx_model::next_name(&name, others);
            }
        }
    }
}

/// The derived values of a region with units and the reasons, for the worksheet's tooltip.
fn exported_hover(resolved: &Resolved, units: UnitSystem) -> String {
    let mut lines = Vec::new();
    if let Some(k) = resolved.normal_stiffness() {
        lines.push(format!(
            "Normal stiffness {}",
            with_unit(k, units, Quantity::ForcePerVolume)
        ));
    }
    if let Some(k) = resolved.tangential_stiffness() {
        lines.push(format!(
            "Tangential stiffness {}",
            with_unit(k, units, Quantity::ForcePerVolume)
        ));
    }
    if let Some(t) = resolved.tolerance() {
        lines.push(format!(
            "Tolerance {}",
            with_unit(t, units, Quantity::Length)
        ));
    }
    lines.extend(resolved.why.iter().cloned());
    lines.join("\n")
}
