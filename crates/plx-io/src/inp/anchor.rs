//! User keywords anchored to what they belong to instead of to indices in the keyword tree.
//!
//! [`UserKeyword::position`] counts the keywords of the tree, so a keyword in a step points
//! at another step once steps move between analyses. Continuations, splits and merges take
//! the keywords apart into those of the model definition and those of each step, named by
//! the step, and put them together again for the new order of steps.

use plx_model::{BoundaryKind, FeModel, UserKeyword};

/// The text of a user keyword and whether it is written.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    pub active: bool,
}

impl Text {
    fn of(keyword: &UserKeyword) -> Self {
        Self {
            text: keyword.text.clone(),
            active: keyword.active,
        }
    }

    fn at(&self, position: Vec<usize>) -> UserKeyword {
        UserKeyword {
            position,
            text: self.text.clone(),
            active: self.active,
        }
    }
}

/// Where a keyword of the model definition is.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelPlace {
    /// Between the section titles, before the one named, or at the end of the file.
    Before(Option<String>),
    /// Under the section title named, at this path below it.
    Under(String, Vec<usize>),
}

/// Where a keyword of a step is.
#[derive(Clone, Debug, PartialEq)]
pub enum StepPlace {
    /// Right before the step.
    Before,
    /// Inside the step, at this path below its title.
    Inside(Vec<usize>),
    /// Right after the step, when no step follows.
    After,
}

/// The user keywords of a model, anchored to section titles and step names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Anchored {
    pub model: Vec<(ModelPlace, Text)>,
    /// The name of the step, where in it and the keyword.
    pub steps: Vec<(String, StepPlace, Text)>,
}

const STEPS: &str = "Steps";

/// The section titles of the top of the keyword tree of [`super::model_keywords`], in
/// order; the last one is always the steps.
pub fn top_titles(model: &FeModel) -> Vec<&'static str> {
    let active = || model.steps.iter().filter(|s| s.active);
    let mut titles = vec!["Heading"];
    if active().any(|s| {
        (s.boundary_conditions.iter()).any(|b| {
            b.active
                && matches!(b.kind, BoundaryKind::Submodel { .. })
                && s.kind.supports_boundary(&b.kind)
        })
    }) {
        titles.push("Submodel");
    }
    titles.extend([
        "Nodes",
        "Elements",
        "Node sets",
        "Element sets",
        "Surfaces",
        "Physical constants",
        "Coordinate systems",
        "Materials",
        "Sections",
        "Pre-tension sections",
        "Constraints",
        "Surface interactions",
        "Contact pairs",
        "Amplitudes",
        "Initial conditions",
    ]);
    if active().any(|s| s.kind.design_region().is_some()) {
        titles.push("Design variables");
    }
    if model.properties.stored_modes.is_some()
        && active().next().is_some_and(|s| s.kind.uses_stored_modes())
    {
        titles.push("Boundary conditions");
    }
    titles.push(STEPS);
    titles
}

/// What a place of the tree holds while the keywords are inserted again.
enum Slot {
    Title(String),
    Step(String),
    User(usize),
}

/// Takes the user keywords of `model` apart. Keywords whose place does not exist are left
/// out, as the writer leaves them out.
pub fn anchor(model: &FeModel) -> Anchored {
    let keywords = &model.user_keywords;
    let mut top: Vec<Slot> = (top_titles(model).into_iter())
        .map(|t| Slot::Title(t.to_owned()))
        .collect();
    let mut steps: Vec<Slot> = (model.steps.iter())
        .map(|s| Slot::Step(s.name.clone()))
        .collect();
    // The keywords below a title or inside a step, in the order they were inserted.
    let mut under: Vec<(usize, ModelPlace)> = Vec::new();
    let mut inside: Vec<(usize, String, Vec<usize>)> = Vec::new();
    for (k, keyword) in keywords.iter().enumerate() {
        let position = &keyword.position;
        match position.as_slice() {
            [] => {}
            [index] => {
                if *index <= top.len() {
                    top.insert(*index, Slot::User(k));
                }
            }
            [index, rest @ ..] => match top.get(*index) {
                Some(Slot::Title(title)) if title == STEPS => match rest {
                    [step] => {
                        if *step <= steps.len() {
                            steps.insert(*step, Slot::User(k));
                        }
                    }
                    [step, path @ ..] => {
                        if let Some(Slot::Step(name)) = steps.get(*step) {
                            inside.push((k, name.clone(), path.to_vec()));
                        }
                    }
                    [] => {}
                },
                Some(Slot::Title(title)) => {
                    under.push((k, ModelPlace::Under(title.clone(), rest.to_vec())));
                }
                _ => {}
            },
        }
    }
    let text = |k: usize| Text::of(&keywords[k]);
    let mut anchored = Anchored::default();
    for (i, slot) in top.iter().enumerate() {
        if let Slot::User(k) = slot {
            let next = top[i..].iter().find_map(|s| match s {
                Slot::Title(t) => Some(t.clone()),
                _ => None,
            });
            anchored.model.push((ModelPlace::Before(next), text(*k)));
        }
    }
    for (i, slot) in steps.iter().enumerate() {
        let Slot::User(k) = slot else {
            continue;
        };
        let name = |s: &Slot| match s {
            Slot::Step(name) => Some(name.clone()),
            _ => None,
        };
        if let Some(next) = steps[i..].iter().find_map(name) {
            anchored.steps.push((next, StepPlace::Before, text(*k)));
        } else if let Some(previous) = steps[..i].iter().rev().find_map(name) {
            anchored.steps.push((previous, StepPlace::After, text(*k)));
        } else {
            // Without steps the end of the steps is the end of the file.
            anchored.model.push((ModelPlace::Before(None), text(*k)));
        }
    }
    anchored
        .model
        .extend(under.into_iter().map(|(k, place)| (place, text(k))));
    anchored.steps.extend(
        (inside.into_iter()).map(|(k, step, path)| (step, StepPlace::Inside(path), text(k))),
    );
    anchored
}

/// The user keywords of `model` with the places `anchored` names, for its titles and steps.
/// Keywords of a title or step that `model` has not are left out; keywords between titles
/// whose next title it has not go to the end of the model definition.
pub fn place(model: &FeModel, anchored: &Anchored) -> Vec<UserKeyword> {
    let titles = top_titles(model);
    let next_title = |place: &Option<String>| match place {
        Some(t) if titles.contains(&t.as_str()) => t.clone(),
        Some(_) => STEPS.to_owned(),
        None => String::new(),
    };
    let mut out = Vec::new();
    let mut i = 0;
    let before = |title: &str| -> Vec<&Text> {
        (anchored.model.iter())
            .filter_map(|(place, text)| match place {
                ModelPlace::Before(next) if next_title(next) == title => Some(text),
                _ => None,
            })
            .collect()
    };
    let of_step = |name: &str| -> Vec<(&StepPlace, &Text)> {
        (anchored.steps.iter())
            .filter(|(step, _, _)| step == name)
            .map(|(_, place, text)| (place, text))
            .collect()
    };
    for title in &titles {
        for text in before(title) {
            out.push(text.at(vec![i]));
            i += 1;
        }
        for (place, text) in &anchored.model {
            if let ModelPlace::Under(t, path) = place
                && t == title
            {
                out.push(text.at([vec![i], path.clone()].concat()));
            }
        }
        if *title == STEPS {
            let mut j = 0;
            for step in &model.steps {
                for (place, text) in of_step(&step.name) {
                    if *place == StepPlace::Before {
                        out.push(text.at(vec![i, j]));
                        j += 1;
                    }
                }
                for (place, text) in of_step(&step.name) {
                    if let StepPlace::Inside(path) = place {
                        out.push(text.at([vec![i, j], path.clone()].concat()));
                    }
                }
                j += 1;
                for (place, text) in of_step(&step.name) {
                    if *place == StepPlace::After {
                        out.push(text.at(vec![i, j]));
                        j += 1;
                    }
                }
            }
        }
        i += 1;
    }
    for text in before("") {
        out.push(text.at(vec![i]));
        i += 1;
    }
    out
}

impl Anchored {
    /// The keywords of the model definition and of the steps named.
    pub fn of_steps(&self, names: &[String]) -> Self {
        Self {
            model: self.model.clone(),
            steps: (self.steps.iter())
                .filter(|(step, _, _)| names.contains(step))
                .cloned()
                .collect(),
        }
    }

    /// The keywords of the steps named only.
    pub fn steps_only(&self, names: &[String]) -> Self {
        Self {
            model: Vec::new(),
            ..self.of_steps(names)
        }
    }

    /// These keywords followed by those of `other`.
    pub fn with(mut self, other: Self) -> Self {
        self.model.extend(other.model);
        self.steps.extend(other.steps);
        self
    }
}
