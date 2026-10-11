//! Names of the bodies in a STEP file, read from its text.
//!
//! OpenCASCADE gives Gmsh the name of a part only where the part is a single solid: a
//! surface part such as a membrane, a shell of faces, and the solids of a part with several
//! bodies come without one. This reads the product structure of the file itself and lists
//! its bodies in the order OpenCASCADE transfers them, so the parts Gmsh read can be named
//! after them.

use std::collections::{BTreeMap, BTreeSet};

/// A body of a STEP file: a solid or the faces of a surface part.
#[derive(Debug, Clone, PartialEq)]
pub struct StepBody {
    pub name: String,
    /// Number of faces of a surface body, `None` for a solid.
    pub faces: Option<usize>,
}

/// An argument of a STEP entity.
#[derive(Debug, Clone)]
enum Arg {
    Ref(u64),
    Str(String),
    List(Vec<Arg>),
    Other,
}

impl Arg {
    fn reference(&self) -> Option<u64> {
        match self {
            Arg::Ref(id) => Some(*id),
            _ => None,
        }
    }

    fn string(&self) -> &str {
        match self {
            Arg::Str(s) => s,
            _ => "",
        }
    }

    fn list(&self) -> &[Arg] {
        match self {
            Arg::List(items) => items,
            _ => &[],
        }
    }
}

/// The bodies of a STEP file, in the order OpenCASCADE transfers them: the products from
/// the roots down the assemblies, each instance of a product once, solids and surface
/// bodies of a product in the order its representation lists them.
pub fn step_bodies(text: &str) -> Vec<StepBody> {
    let entities = parse(text);
    let get = |id: u64, kind: &str| entities.get(&id).filter(|(k, _)| k == kind);
    // Product definition -> product name.
    let mut product_names = BTreeMap::new();
    for (&id, (kind, args)) in &entities {
        if kind != "PRODUCT_DEFINITION" {
            continue;
        }
        let name = (args.get(2).and_then(Arg::reference))
            .and_then(|f| {
                get(f, "PRODUCT_DEFINITION_FORMATION")
                    .or_else(|| get(f, "PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE"))
            })
            .and_then(|(_, f)| f.get(2).and_then(Arg::reference))
            .and_then(|p| get(p, "PRODUCT"))
            .map(|(_, p)| {
                let name = p.get(1).map_or("", Arg::string);
                if name.trim().is_empty() {
                    p.first().map_or("", Arg::string)
                } else {
                    name
                }
            })
            .unwrap_or_default();
        product_names.insert(id, name.to_owned());
    }
    // Representations linked to one another, as a shape representation to the B-rep of a
    // part. Placements in assemblies are complex entities, which `parse` leaves out.
    let mut links: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (kind, args) in entities.values() {
        if kind != "SHAPE_REPRESENTATION_RELATIONSHIP" {
            continue;
        }
        if let (Some(a), Some(b)) = (
            args.get(2).and_then(Arg::reference),
            args.get(3).and_then(Arg::reference),
        ) {
            links.entry(a).or_default().push(b);
            links.entry(b).or_default().push(a);
        }
    }
    // Product definition -> its representations.
    let mut representations: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    for (kind, args) in entities.values() {
        if kind != "SHAPE_DEFINITION_REPRESENTATION" {
            continue;
        }
        let definition = (args.first().and_then(Arg::reference))
            .and_then(|s| get(s, "PRODUCT_DEFINITION_SHAPE"))
            .and_then(|(_, s)| s.get(2).and_then(Arg::reference));
        let (Some(definition), Some(rep)) = (definition, args.get(1).and_then(Arg::reference))
        else {
            continue;
        };
        let mut seen = BTreeSet::from([rep]);
        let mut todo = vec![rep];
        while let Some(r) = todo.pop() {
            for &next in links.get(&r).into_iter().flatten() {
                if seen.insert(next) {
                    todo.push(next);
                }
            }
        }
        representations.entry(definition).or_default().extend(seen);
    }
    // Assemblies: parent -> children, in the order of the file.
    let mut children: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    let mut is_child = BTreeSet::new();
    for (kind, args) in entities.values() {
        if kind != "NEXT_ASSEMBLY_USAGE_OCCURRENCE" {
            continue;
        }
        if let (Some(parent), Some(child)) = (
            args.get(3).and_then(Arg::reference),
            args.get(4).and_then(Arg::reference),
        ) {
            children.entry(parent).or_default().push(child);
            is_child.insert(child);
        }
    }
    let mut bodies = Vec::new();
    let roots = product_names.keys().filter(|d| !is_child.contains(d));
    let mut stack: Vec<(u64, usize)> = roots.rev().map(|&d| (d, 0)).collect();
    while let Some((definition, depth)) = stack.pop() {
        // A cyclic file would loop for ever.
        if depth > 64 {
            continue;
        }
        let product = product_names.get(&definition).map_or("", String::as_str);
        let own = product_bodies(&entities, representations.get(&definition), product);
        bodies.extend(own);
        for &child in children.get(&definition).into_iter().flatten().rev() {
            stack.push((child, depth + 1));
        }
    }
    bodies
}

/// The bodies of a product: its solids and the shells of its surface models, named after
/// the product, after themselves if the product has several and they have names.
fn product_bodies(
    entities: &BTreeMap<u64, (String, Vec<Arg>)>,
    representations: Option<&Vec<u64>>,
    product: &str,
) -> Vec<StepBody> {
    let mut found: Vec<(String, Option<usize>)> = Vec::new();
    let mut representation_name = String::new();
    for &rep in representations.into_iter().flatten() {
        let Some((_, args)) = entities.get(&rep) else {
            continue;
        };
        if representation_name.trim().is_empty() {
            representation_name = args.first().map_or("", Arg::string).to_owned();
        }
        for item in args.get(1).map_or(&[][..], Arg::list) {
            let Some((kind, item)) = item.reference().and_then(|i| entities.get(&i)) else {
                continue;
            };
            let name = item.first().map_or("", Arg::string).to_owned();
            match kind.as_str() {
                "MANIFOLD_SOLID_BREP" | "BREP_WITH_VOIDS" | "FACETED_BREP" => {
                    found.push((name, None));
                }
                "SHELL_BASED_SURFACE_MODEL" => {
                    let faces = (item.get(1).map_or(&[][..], Arg::list).iter())
                        .filter_map(|s| s.reference().and_then(|s| entities.get(&s)))
                        .map(|(_, shell)| shell.get(1).map_or(0, |f| f.list().len()))
                        .sum();
                    if faces > 0 {
                        found.push((name, Some(faces)));
                    }
                }
                _ => {}
            }
        }
    }
    let fallback = if product.trim().is_empty() {
        representation_name.as_str()
    } else {
        product
    };
    let several = found.len() > 1;
    (found.into_iter())
        .map(|(own, faces)| StepBody {
            name: if several && !own.trim().is_empty() {
                own
            } else {
                fallback.to_owned()
            },
            faces,
        })
        .collect()
}

/// The simple entities of the data section by their number, as type and arguments.
/// Complex entities, written as a list of partial entities, are left out.
fn parse(text: &str) -> BTreeMap<u64, (String, Vec<Arg>)> {
    let mut entities = BTreeMap::new();
    let data = text.find("DATA;").map_or(text, |start| &text[start + 5..]);
    for statement in statements(data) {
        let Some((id, rest)) = statement.trim().split_once('=') else {
            continue;
        };
        let Some(id) = id
            .trim()
            .strip_prefix('#')
            .and_then(|i| i.trim().parse().ok())
        else {
            continue;
        };
        let rest = rest.trim();
        let Some(open) = rest.find('(') else {
            continue;
        };
        let kind = rest[..open].trim();
        if kind.is_empty() {
            continue;
        }
        let mut chars = rest[open..].chars().peekable();
        if let Arg::List(args) = parse_arg(&mut chars) {
            entities.insert(id, (kind.to_ascii_uppercase(), args));
        }
    }
    entities
}

/// The statements of the data section, split at the semicolons outside strings.
fn statements(data: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    for (i, c) in data.char_indices() {
        match c {
            '\'' => in_string = !in_string,
            ';' if !in_string => {
                statements.push(&data[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    statements
}

fn parse_arg(chars: &mut std::iter::Peekable<std::str::Chars>) -> Arg {
    while chars.next_if(|c| c.is_whitespace()).is_some() {}
    match chars.peek() {
        Some('(') => {
            chars.next();
            let mut items = Vec::new();
            loop {
                while chars.next_if(|c| c.is_whitespace() || *c == ',').is_some() {}
                match chars.peek() {
                    None => break,
                    Some(')') => {
                        chars.next();
                        break;
                    }
                    _ => items.push(parse_arg(chars)),
                }
            }
            Arg::List(items)
        }
        Some('\'') => {
            chars.next();
            let mut raw = String::new();
            while let Some(c) = chars.next() {
                if c == '\'' {
                    if chars.next_if_eq(&'\'').is_some() {
                        raw.push('\'');
                        continue;
                    }
                    break;
                }
                raw.push(c);
            }
            Arg::Str(decode(&raw))
        }
        Some('#') => {
            chars.next();
            let mut digits = String::new();
            while let Some(c) = chars.next_if(char::is_ascii_digit) {
                digits.push(c);
            }
            digits.parse().map_or(Arg::Other, Arg::Ref)
        }
        _ => {
            // Numbers, enumerations, $ and *, or a typed value like LENGTH_MEASURE(1.).
            let mut depth = 0;
            while let Some(&c) = chars.peek() {
                match c {
                    '(' => depth += 1,
                    ')' if depth == 0 => break,
                    ')' => depth -= 1,
                    ',' if depth == 0 => break,
                    _ => {}
                }
                chars.next();
            }
            Arg::Other
        }
    }
}

/// Decodes the escapes of STEP strings: `\X2\00E4\X0\` (UTF-16), `\X4\...\X0\` (UTF-32),
/// `\X\E4` (ISO 8859-1), `\S\` and `\\`.
fn decode(raw: &str) -> String {
    if !raw.contains('\\') {
        return raw.to_owned();
    }
    let mut out = String::new();
    let mut rest = raw;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let hex_run = |body: &str, width: usize| -> Option<(String, usize)> {
            let end = body.find("\\X0\\")?;
            let hex = &body[..end];
            if !hex.len().is_multiple_of(width) {
                return None;
            }
            let units = (0..hex.len() / width)
                .map(|k| u32::from_str_radix(&hex[k * width..(k + 1) * width], 16).ok())
                .collect::<Option<Vec<_>>>()?;
            let text = if width == 4 {
                let units: Vec<u16> = units.into_iter().map(|u| u as u16).collect();
                String::from_utf16_lossy(&units)
            } else {
                units.into_iter().filter_map(char::from_u32).collect()
            };
            Some((text, end + 4))
        };
        if let Some(body) = rest.strip_prefix("\\X2\\") {
            if let Some((text, used)) = hex_run(body, 4) {
                out.push_str(&text);
                rest = &body[used..];
                continue;
            }
        } else if let Some(body) = rest.strip_prefix("\\X4\\") {
            if let Some((text, used)) = hex_run(body, 8) {
                out.push_str(&text);
                rest = &body[used..];
                continue;
            }
        } else if let Some(body) = rest.strip_prefix("\\X\\") {
            if let Some(c) = body.get(..2).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(char::from(c));
                rest = &body[2..];
                continue;
            }
        } else if let Some(body) = rest.strip_prefix("\\S\\") {
            if let Some(c) = body.chars().next() {
                out.push(char::from_u32(c as u32 + 128).unwrap_or(c));
                rest = &body[c.len_utf8()..];
                continue;
            }
        } else if let Some(body) = rest.strip_prefix("\\\\") {
            out.push('\\');
            rest = body;
            continue;
        }
        out.push('\\');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_escaped_names() {
        assert_eq!(decode("Oberfl\\X2\\00E4\\X0\\che"), "Oberfläche");
        assert_eq!(decode("Gr\\X\\F6\\X\\DFe"), "Größe");
        assert_eq!(decode("a\\\\b"), "a\\b");
        assert_eq!(decode("plain"), "plain");
    }

    #[test]
    fn lists_solids_and_shells_in_assembly_order() {
        let text = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n\
            #1 = PRODUCT('asm','asm','',(#90));\n\
            #2 = PRODUCT_DEFINITION_FORMATION('','',#1);\n\
            #3 = PRODUCT_DEFINITION('design','',#2,#91);\n\
            #10 = PRODUCT('p1','Membran','',(#90));\n\
            #11 = PRODUCT_DEFINITION_FORMATION('','',#10);\n\
            #12 = PRODUCT_DEFINITION('design','',#11,#91);\n\
            #13 = PRODUCT_DEFINITION_SHAPE('','',#12);\n\
            #14 = SHAPE_DEFINITION_REPRESENTATION(#13,#15);\n\
            #15 = MANIFOLD_SURFACE_SHAPE_REPRESENTATION('',(#16),#92);\n\
            #16 = SHELL_BASED_SURFACE_MODEL('',(#17));\n\
            #17 = OPEN_SHELL('',(#30,#31));\n\
            #20 = PRODUCT('p2','unten','',(#90));\n\
            #21 = PRODUCT_DEFINITION_FORMATION('','',#20);\n\
            #22 = PRODUCT_DEFINITION('design','',#21,#91);\n\
            #23 = PRODUCT_DEFINITION_SHAPE('','',#22);\n\
            #24 = SHAPE_DEFINITION_REPRESENTATION(#23,#25);\n\
            #25 = SHAPE_REPRESENTATION('',(#93),#92);\n\
            #26 = ADVANCED_BREP_SHAPE_REPRESENTATION('',(#27,#28),#92);\n\
            #27 = MANIFOLD_SOLID_BREP('Steg',#40);\n\
            #28 = MANIFOLD_SOLID_BREP('',#41);\n\
            #29 = SHAPE_REPRESENTATION_RELATIONSHIP('','',#25,#26);\n\
            #50 = NEXT_ASSEMBLY_USAGE_OCCURRENCE('2','','',#3,#22,$);\n\
            #51 = NEXT_ASSEMBLY_USAGE_OCCURRENCE('1','','',#3,#12,$);\n\
            #52 = ( REPRESENTATION_RELATIONSHIP('','',#25,#15) \
                REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#53) \
                SHAPE_REPRESENTATION_RELATIONSHIP() );\n\
            ENDSEC;\nEND-ISO-10303-21;\n";
        let bodies = step_bodies(text);
        let solid = |name: &str| StepBody {
            name: name.into(),
            faces: None,
        };
        assert_eq!(
            bodies,
            [
                solid("Steg"),
                solid("unten"),
                StepBody {
                    name: "Membran".into(),
                    faces: Some(2)
                },
            ]
        );
    }
}
