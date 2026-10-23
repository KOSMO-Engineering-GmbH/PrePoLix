//! Tables saved as CSV or JSON files. JSON keeps the columns and rows apart, so that other
//! programs, Typst for example, read the values without parsing text.

use std::path::PathBuf;

/// File format of a saved table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Csv,
    Json,
}

impl Format {
    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }

    fn filter(self) -> &'static str {
        match self {
            Self::Csv => "CSV file",
            Self::Json => "JSON file",
        }
    }
}

/// Asks where to save a table, proposing `<name>.csv` or `<name>.json`; a name without an
/// extension gets the one of the format.
pub fn save_dialog(title: &str, name: &str, format: Format) -> Option<PathBuf> {
    let extension = format.extension();
    let path = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(format.filter(), &[extension])
        .set_file_name(format!("{}.{extension}", name.trim()))
        .save_file()?;
    Some(if path.extension().is_none() {
        path.with_extension(extension)
    } else {
        path
    })
}

/// Writes `text`, a table with a header line and its cells split by `separator`, as CSV or
/// JSON (see [`to_json`]).
pub fn write(
    path: &std::path::Path,
    text: &str,
    separator: char,
    format: Format,
) -> std::io::Result<()> {
    match format {
        Format::Csv => std::fs::write(path, text),
        Format::Json => std::fs::write(path, to_json(text, separator)),
    }
}

/// A table of text as JSON: `{"columns": [names], "rows": [[cells]]}`, cells that are numbers
/// as numbers, empty ones as `null`. Several tables, split by empty lines, become an array of
/// them.
pub fn to_json(text: &str, separator: char) -> String {
    let mut tables = Vec::new();
    let mut lines = Vec::new();
    for line in text.lines().chain([""]) {
        if line.trim().is_empty() {
            if !lines.is_empty() {
                tables.push(table(&std::mem::take(&mut lines), separator));
            }
        } else {
            lines.push(line);
        }
    }
    let value = match tables.len() {
        1 => tables.remove(0),
        _ => serde_json::Value::Array(tables),
    };
    serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
}

fn table(lines: &[&str], separator: char) -> serde_json::Value {
    use serde_json::Value;
    let cells = |line: &str| {
        line.split(separator)
            .map(str::trim)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let columns = lines.first().map(|l| cells(l)).unwrap_or_default();
    let rows = (lines.iter().skip(1))
        .map(|line| {
            let row = cells(line).into_iter().map(|cell| {
                if cell.is_empty() {
                    Value::Null
                } else if let Ok(integer) = cell.parse::<i64>() {
                    Value::from(integer)
                } else if let Some(number) =
                    (cell.parse::<f64>().ok()).and_then(serde_json::Number::from_f64)
                {
                    Value::Number(number)
                } else {
                    Value::String(cell)
                }
            });
            Value::Array(row.collect())
        })
        .collect();
    serde_json::json!({ "columns": columns, "rows": Value::Array(rows) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_becomes_columns_and_rows() {
        let json = to_json("Distance;S Mises [MPa]\n0;12.5\n1.5;\n2;n/a\n", ';');
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "columns": ["Distance", "S Mises [MPa]"],
                "rows": [[0, 12.5], [1.5, null], [2, "n/a"]],
            })
        );
    }

    #[test]
    fn tables_split_by_empty_lines_become_an_array() {
        let json = to_json("A,B\n1,2\n\nNode,S_hs\n7,3e2\n", ',');
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value,
            serde_json::json!([
                {"columns": ["A", "B"], "rows": [[1, 2]]},
                {"columns": ["Node", "S_hs"], "rows": [[7, 300.0]]},
            ])
        );
    }
}
