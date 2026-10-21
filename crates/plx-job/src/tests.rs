use super::*;

const STA: &str = "\
SUMMARY OF JOB INFORMATION
  STEP      INC     ATT  ITRS     TOT TIME     STEP TIME         INC TIME
     1       1     1     2   0.250000E+00  0.250000E+00  0.250000E+00
     1       2     1     3   0.625000E+00  0.625000E+00  0.375000E+00
";

#[test]
fn reads_the_last_increment_of_the_status_file() {
    let progress = parse_sta(STA).unwrap();
    assert_eq!(
        (progress.step, progress.increment, progress.iterations),
        (1, 2, 3)
    );
    assert_eq!(progress.step_time, 0.625);
    assert_eq!(parse_sta("SUMMARY OF JOB INFORMATION\n"), None);
    // An attempt that did not converge is marked with a "U".
    let failed = parse_sta(&format!(
        "{STA}     1  3   2U    4  0.6E+00  0.6E+00  0.1E+00\n"
    ));
    assert_eq!(failed.map(|p| (p.increment, p.attempt)), Some((3, 2)));
}

fn work_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("plx-job-{name}-{}", std::process::id()))
}

fn ccx_installed() -> bool {
    let found = Command::new("ccx").arg("-v").output().is_ok();
    if !found {
        eprintln!("ccx not found, test skipped");
    }
    found
}

#[test]
fn runs_calculix_and_finds_the_results() {
    if !ccx_installed() {
        return;
    }
    let input = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/kragbalken_c3d8.inp"),
    )
    .unwrap();
    let dir = work_dir("ok");
    let mut job = Job::start(&Solver::default(), &dir, "Analysis-1", &input).unwrap();
    assert_eq!(job.wait(), JobStatus::Completed);
    assert!(job.new_output().iter().any(|l| l.contains("Job finished")));
    assert!(job.new_output().is_empty());
    assert_eq!(job.progress().map(|p| p.step), Some(1));
    assert!(job.results().is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn input_errors_fail_the_job() {
    if !ccx_installed() {
        return;
    }
    let dir = work_dir("error");
    let mut job = Job::start(&Solver::default(), &dir, "Analysis-1", "*STEP\n*STATIC\n").unwrap();
    assert_eq!(job.wait(), JobStatus::Failed);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_solver_is_reported_at_start() {
    let solver = Solver {
        executable: PathBuf::from("plx-kein-ccx"),
        threads: 1,
    };
    let dir = work_dir("missing");
    assert!(Job::start(&solver, &dir, "Analysis-1", "").is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_solver_that_crashes_after_writing_results_fails() {
    use std::os::unix::fs::PermissionsExt;
    // Writes results as CalculiX would, then ends with an error code but no error message,
    // as a CalculiX that runs out of memory does.
    let dir = work_dir("crash");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("crashing-ccx.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nhead -c 1000 /dev/zero | tr '\\0' 'x' > \"$1.frd\"\nexit 3\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let solver = Solver {
        executable: script,
        threads: 1,
    };
    let mut job = Job::start(&solver, &dir, "job", "*HEADING\n").unwrap();
    assert_eq!(job.wait(), JobStatus::FailedWithResults);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The live progress follows the output of a real run: two nonlinear steps, and the second
/// step's header comes before its increments, not buffered to the end of the run.
#[test]
fn live_progress_follows_a_nonlinear_run() {
    if !ccx_installed() {
        return;
    }
    let input = "\
*NODE
1,0,0,0
2,1,0,0
3,1,1,0
4,0,1,0
5,0,0,1
6,1,0,1
7,1,1,1
8,0,1,1
*ELEMENT,TYPE=C3D8,ELSET=E
1,1,2,3,4,5,6,7,8
*NSET,NSET=BOT
1,2,3,4
*NSET,NSET=TOP
5,6,7,8
*MATERIAL,NAME=M
*ELASTIC
210000,0.3
*PLASTIC
200,0
400,0.1
*SOLID SECTION,ELSET=E,MATERIAL=M
*STEP,NLGEOM,INC=100
*STATIC
0.5,1
*BOUNDARY
BOT,1,3
TOP,3,3,0.3
*END STEP
*STEP,NLGEOM
*STATIC
0.25,1
*BOUNDARY
BOT,1,3
*CLOAD
TOP,1,10
*END STEP
";
    let dir = work_dir("live");
    let mut job = Job::start(&Solver::default(), &dir, "Analysis-1", input).unwrap();
    assert_eq!(job.wait(), JobStatus::Completed);
    let (mut steps, mut replay) = (Vec::new(), live::LiveProgress::default());
    for line in job.new_output() {
        replay.read(&line);
        if replay.increment.is_some() {
            steps.push(replay.step);
        }
    }
    steps.dedup();
    assert_eq!(
        steps,
        [Some(1), Some(2)],
        "increments of both steps under their header"
    );
    let live = job.live_progress();
    assert_eq!(live, &replay);
    assert!(
        live.iteration.is_some() && live.step_time.is_some(),
        "{live:?}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
