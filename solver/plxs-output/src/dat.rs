//! The `.dat` writer: history blocks with CalculiX's headings (`plx-io::dat` recognises the
//! headings in `crates/plx-io/src/dat.rs`).

use std::io::{BufWriter, Write};
use std::path::Path;

use plxs_core::results::{HistoryBlock, Results};
use plxs_core::{Error, Result};

use crate::{fortran_e13, fortran_time};

/// Writes the history blocks of all steps.
pub fn write_dat(path: &Path, results: &Results) -> Result<()> {
    let io = |e: std::io::Error| Error::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    };
    let file = std::fs::File::create(path).map_err(io)?;
    let mut out = BufWriter::new(file);
    write_dat_to(&mut out, results).map_err(io)?;
    out.flush().map_err(io)
}

pub fn write_dat_to(out: &mut impl Write, results: &Results) -> std::io::Result<()> {
    let mut last: Option<(u32, u32)> = None;
    for block in &results.history {
        if last.map(|(s, _)| s) != Some(block.step) {
            writeln!(out)?;
            writeln!(out, "                        S T E P {:>7}", block.step)?;
            writeln!(out)?;
        }
        if last != Some((block.step, block.increment)) {
            writeln!(out)?;
            writeln!(
                out,
                "                                INCREMENT {:>5}",
                block.increment
            )?;
            writeln!(out)?;
            last = Some((block.step, block.increment));
        }
        write_block(out, block)?;
    }
    Ok(())
}

fn write_block(out: &mut impl Write, block: &HistoryBlock) -> std::io::Result<()> {
    writeln!(out)?;
    writeln!(
        out,
        " {} for set {} and time  {}",
        block.quantity,
        block.set,
        fortran_time(block.time)
    )?;
    writeln!(out)?;
    let per_point = block.quantity.contains("integ.pnt.");
    let totals = block.quantity.starts_with("total ");
    for (id, values) in &block.rows {
        let mut values = values.as_slice();
        if totals {
            write!(out, "{:>6}", "")?;
        } else {
            write!(out, "{:>10}", id)?;
        }
        if per_point {
            write!(out, "{:>4}", values.first().copied().unwrap_or(0.0) as u32)?;
            values = &values[1..];
        }
        for v in values {
            write!(out, " {}", fortran_e13(*v))?;
        }
        writeln!(out)?;
    }
    Ok(())
}
