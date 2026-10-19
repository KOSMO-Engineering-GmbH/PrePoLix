//! The Math and Switch blocks of the project schematic, like Blender's Math node: numbers
//! computed from the numbers value links give them, for parameters derived from others
//! (Lz = 2 Lx) and variants picked by an index, also as inputs and outputs of a parameter
//! study.

use super::*;

/// The row of a Math block that chooses its operation, the row of both that chooses the kind
/// of their numbers, and the row of a Switch block that adds and removes choices.
const OPERATION: &str = "Operation";
const KIND: &str = "Kind";
const CHOICES: &str = "Choices";

/// The most choices a Switch block holds.
const MAX_CHOICES: usize = 20;

/// A number as a block shows it: in the unit system, with its unit for a quantity.
pub(super) fn shown_number(value: f64, kind: Option<ValueKind>, units: UnitSystem) -> String {
    match kind.and_then(ValueKind::quantity) {
        Some(quantity) => format!(
            "{} {}",
            crate::results::format_value(units.shown(value, quantity) as f32),
            units.unit(quantity)
        )
        .trim()
        .to_owned(),
        None => crate::results::format_value(value as f32),
    }
}

/// The label of the kind of the numbers of a block, `None` while it takes the first one a
/// link brings.
fn kind_label(kind: Option<ValueKind>) -> &'static str {
    kind.map_or("Any", ValueKind::label)
}

/// The rows of a Math or Switch block: what it computes, its inputs and its result.
pub(super) fn number_rows(node: &Node, incoming: &[(Vec<String>, String)]) -> Vec<Row> {
    let Some((number, switch)) = node.as_number() else {
        return Vec::new();
    };
    let row = |path: Vec<String>, text: String, tip: String| Row {
        path,
        text,
        depth: 0,
        icon: NodeIcon::Setup,
        state: None,
        expander: None,
        input: false,
        output: false,
        linked_from: None,
        collection: false,
        partial: false,
        widget: None,
        tip,
    };
    let mut rows = Vec::new();
    if !switch {
        rows.push(Row {
            widget: Some(RowWidget::Operation),
            ..row(
                vec![OPERATION.to_owned()],
                "Operation".into(),
                format!(
                    "What the block computes: {}.",
                    number.operation.formula().to_lowercase()
                ),
            )
        });
    }
    rows.push(Row {
        widget: Some(RowWidget::NumberKind),
        ..row(
            vec![KIND.to_owned()],
            "Kind".into(),
            "What its numbers are, such as a length or a force: value links join only numbers \
             of one kind. Any takes the kind of the first number linked to it."
                .into(),
        )
    });
    for (index, (key, label, kind, _)) in number.inputs(switch).into_iter().enumerate() {
        let path = vec![links::PARAMETERS.to_owned(), key.clone()];
        let linked_from = (incoming.iter())
            .find(|(slot, _)| *slot == path)
            .map(|(_, source)| source.clone());
        let what = if key == INDEX {
            "Which choice the block gives, counted from 1".to_owned()
        } else if kind == Some(ValueKind::Factor) && number.operation.scales() && !switch {
            format!("{label}, a factor without unit")
        } else {
            format!("{label}, a {}", kind_label(kind).to_lowercase())
        };
        rows.push(Row {
            input: true,
            linked_from,
            widget: Some(RowWidget::Input(index)),
            ..row(
                path,
                label,
                format!(
                    "{what}. Type it, or drag the pin of a number here: a parameter, another \
                     Math or Switch block or an input of a parameter study."
                ),
            )
        });
    }
    if switch {
        rows.push(Row {
            widget: Some(RowWidget::Choices),
            ..row(
                vec![CHOICES.to_owned()],
                format!("{} choices", number.values.len().saturating_sub(1)),
                "Adds a choice, or removes the last one with its link.".into(),
            )
        });
    }
    let text = match number.result(switch) {
        Some(value) => format!("Result: {}", shown_number(value, number.kind, number.units)),
        None if switch => "Result: no choice".into(),
        None => "Result: none".into(),
    };
    rows.push(Row {
        output: true,
        icon: NodeIcon::Results,
        ..row(
            vec![links::VALUE.to_owned(), RESULT.to_owned()],
            text,
            "The number the block gives: drag its pin to a parameter of the same kind, to \
             another Math or Switch block, or to the Outputs of a parameter study."
                .into(),
        )
    });
    rows
}

impl ProjectApp {
    /// Adds a Math block, which adds two numbers until another operation is chosen.
    pub(super) fn add_math(&mut self, position: egui::Pos2) -> u64 {
        self.add_number(position, false)
    }

    /// Adds a Switch block with two choices, the first one picked.
    pub(super) fn add_switch(&mut self, position: egui::Pos2) -> u64 {
        self.add_number(position, true)
    }

    fn add_number(&mut self, position: egui::Pos2, switch: bool) -> u64 {
        let id = self.next_id();
        let number = NumberBlock {
            units: self.settings.new_model.units,
            kind: None,
            operation: MathOp::default(),
            values: if switch {
                vec![1.0, 0.0, 0.0]
            } else {
                vec![0.0, 0.0]
            },
        };
        let (name, block) = if switch {
            (self.unique_name("Switch"), Block::Switch(number))
        } else {
            (self.unique_name("Math"), Block::Math(number))
        };
        self.nodes.push(Node::new(id, name.clone(), block));
        self.graph.insert_node(position, id);
        self.output.push(format!(
            "{name} created: drag numbers to its inputs and its Result to the number it gives."
        ));
        id
    }

    /// The operation of a Math block, with as many inputs as it takes.
    pub(super) fn set_operation(&mut self, id: u64, operation: MathOp) {
        let Some((number, false)) = self.node(id).and_then(Node::as_number_mut) else {
            return;
        };
        // B of a product or quotient is a factor, else a number of the kind of the block.
        let rekinded = number.operation.scales() != operation.scales();
        number.operation = operation;
        number.values.resize(operation.inputs().len(), 0.0);
        // A link to an input the operation no longer has goes, and one to B once its kind
        // changes.
        let keys: Vec<String> = (number.inputs(false).into_iter())
            .map(|(key, ..)| key)
            .filter(|key| !(rekinded && key == "b"))
            .collect();
        self.links.retain(|l| {
            l.target != id
                || !matches!(links::slot(l), [c, k] if c == links::PARAMETERS && !keys.contains(k))
        });
        self.sync_due = true;
    }

    /// The number of choices of a Switch block; links to choices it no longer has go.
    pub(super) fn set_choices(&mut self, id: u64, count: usize) {
        let Some((number, true)) = self.node(id).and_then(Node::as_number_mut) else {
            return;
        };
        let count = count.clamp(1, MAX_CHOICES);
        number.values.resize(count + 1, 0.0);
        self.links.retain(|l| {
            l.target != id
                || !matches!(links::slot(l), [c, k]
                    if c == links::PARAMETERS && k.parse::<usize>().is_ok_and(|i| i > count))
        });
        self.sync_due = true;
    }

    /// A Math or Switch block without a kind at an end of a new value link takes the kind of
    /// the other end.
    pub(super) fn adopt_kind(
        &mut self,
        source: u64,
        item: &[String],
        target: u64,
        slot: &[String],
    ) {
        let given = value_kind(&self.nodes, source, item);
        let taken = value_kind(&self.nodes, target, slot);
        for (id, path, kind) in [(target, slot, given), (source, item, taken)] {
            let Some(kind) = kind else {
                continue;
            };
            let open =
                (self.nodes.iter().find(|n| n.id == id)).is_some_and(|n| open_number(n, path));
            if open && let Some((number, _)) = self.node(id).and_then(Node::as_number_mut) {
                number.kind = Some(kind);
            }
        }
    }
}

/// Whether the row of `path` of a block is a number of a Math or Switch block that still
/// takes any kind: its result, or an input of the kind of the block, while it has none.
pub(super) fn open_number(node: &Node, path: &[String]) -> bool {
    node.as_number().is_some_and(|(n, _)| n.kind.is_none()) && node.number_kind(path) == Some(None)
}

impl Viewer {
    /// The rows of a Math or Switch block that edit it in place.
    pub(super) fn edit_number_row(
        &mut self,
        ui: &mut egui::Ui,
        id: u64,
        row: &Row,
        widget: RowWidget,
        field: egui::Rect,
        tip: &str,
    ) {
        let Some(number) = self.block(id).and_then(|b| b.number.clone()) else {
            return;
        };
        let (block, switch) = number;
        match widget {
            RowWidget::Operation => {
                let mut operation = block.operation;
                egui::ComboBox::from_id_salt(("math operation", id))
                    .selected_text(operation.label())
                    .width(field.width())
                    .show_ui(ui, |ui| {
                        for choice in MathOp::ALL {
                            ui.selectable_value(&mut operation, choice, choice.label())
                                .on_hover_text(choice.formula());
                        }
                    })
                    .response
                    .on_hover_text(tip);
                if operation != block.operation {
                    self.actions.push(Action::SetOperation(id, operation));
                }
            }
            RowWidget::NumberKind => {
                let mut kind = block.kind;
                egui::ComboBox::from_id_salt(("number kind", id))
                    .selected_text(kind_label(kind))
                    .width(field.width())
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut kind, None, kind_label(None));
                        for choice in ValueKind::all() {
                            ui.selectable_value(&mut kind, Some(choice), choice.label());
                        }
                    })
                    .response
                    .on_hover_text(tip);
                if kind != block.kind {
                    self.actions.push(Action::SetNumberKind(id, kind));
                }
            }
            RowWidget::Input(index) => {
                let Some((key, _, kind, mut value)) = block.inputs(switch).into_iter().nth(index)
                else {
                    return;
                };
                if row.linked_from.is_some() {
                    ui.disable();
                }
                let before = value;
                let kind = kind.unwrap_or(ValueKind::Factor);
                ui.add_sized(
                    field.size(),
                    crate::parameter_set::field(&mut value, kind, block.units),
                )
                .on_hover_text(tip);
                if value != before {
                    self.actions.push(Action::SetNumberInput(id, key, value));
                }
            }
            RowWidget::Choices => {
                let count = block.values.len().saturating_sub(1);
                ui.horizontal(|ui| {
                    let add = ui.add_enabled(count < MAX_CHOICES, egui::Button::new("Add").small());
                    if add.on_hover_text("Adds a choice").clicked() {
                        self.actions.push(Action::SetChoices(id, count + 1));
                    }
                    let remove = ui.add_enabled(count > 1, egui::Button::new("Remove").small());
                    if remove
                        .on_hover_text("Removes the last choice, with its link")
                        .clicked()
                    {
                        self.actions.push(Action::SetChoices(id, count - 1));
                    }
                });
            }
            _ => {}
        }
    }
}
