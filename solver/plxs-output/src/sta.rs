//! The `.sta` (increments) and `.cvg` (convergence) tables.

use std::io::{BufWriter, Write};
use std::path::Path;

use plxs_core::{Error, Result};

/// One converged increment.
#[derive(Clone, Debug, PartialEq)]
pub struct StaRow {
    pub step: u32,
    pub increment: u32,
    pub attempt: u32,
    pub iterations: u32,
    pub total_time: f64,
    pub step_time: f64,
    pub increment_size: f64,
}

/// One Newton iteration.
#[derive(Clone, Debug, PartialEq)]
pub struct CvgRow {
    pub step: u32,
    pub increment: u32,
    pub attempt: u32,
    pub iteration: u32,
    pub contact_elements: u32,
    pub residual: f64,
    pub correction: f64,
    pub step_time: f64,
    pub increment_size: f64,
}

fn io(path: &Path, e: std::io::Error) -> Error {
    Error::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

fn e14(value: f64) -> String {
    let text = format!("{:.6E}", value.abs());
    let (mantissa, exponent) = text.split_once('E').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    let sign = if value < 0.0 { "-" } else { " " };
    let exp_sign = if exponent < 0 { "-" } else { "+" };
    format!("{sign}{mantissa}E{exp_sign}{:02}", exponent.abs())
}

pub fn write_sta(path: &Path, rows: &[StaRow]) -> Result<()> {
    let file = std::fs::File::create(path).map_err(|e| io(path, e))?;
    let mut out = BufWriter::new(file);
    (|| -> std::io::Result<()> {
        writeln!(out, "SUMMARY OF JOB INFORMATION")?;
        writeln!(
            out,
            "  STEP      INC     ATT  ITER    TOT TIME     STEP TIME       INC TIME"
        )?;
        for r in rows {
            writeln!(
                out,
                "{:>6}{:>9}{:>8}{:>6}{}{}{}",
                r.step,
                r.increment,
                r.attempt,
                r.iterations,
                e14(r.total_time),
                e14(r.step_time),
                e14(r.increment_size)
            )?;
        }
        out.flush()
    })()
    .map_err(|e| io(path, e))
}

pub fn write_cvg(path: &Path, rows: &[CvgRow]) -> Result<()> {
    let file = std::fs::File::create(path).map_err(|e| io(path, e))?;
    let mut out = BufWriter::new(file);
    (|| -> std::io::Result<()> {
        writeln!(out, "SUMMARY OF CONVERGENCE INFORMATION")?;
        writeln!(
            out,
            "  STEP     INC   ATT   ITER   CONT.EL.   RESID.FORCE   CORR.DISP   STEP TIME   INC TIME"
        )?;
        for r in rows {
            writeln!(
                out,
                "{:>6}{:>8}{:>6}{:>7}{:>11}{}{}{}{}",
                r.step,
                r.increment,
                r.attempt,
                r.iteration,
                r.contact_elements,
                e14(r.residual),
                e14(r.correction),
                e14(r.step_time),
                e14(r.increment_size)
            )?;
        }
        out.flush()
    })()
    .map_err(|e| io(path, e))
}
