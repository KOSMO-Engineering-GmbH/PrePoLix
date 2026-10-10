<p align="center">
  <img src="assets/icon/prepolix.svg" alt="PrePoLix icon" width="160">
</p>

<h1 align="center">PrePoLix</h1>

PrePoLix is a graphical pre- and post-processor for the finite element solver
[CalculiX](http://www.calculix.de/). It is a rebuild of [PrePoMax](https://prepomax.fs.um.si/) in
Rust, limited to CalculiX, and runs on **Linux and Windows**. The goal is the same workflow and look as
PrePoMax: import a geometry, mesh it, set up materials, loads and steps, run CalculiX and look at
the results, all in one window and without editing input files by hand.

![PrePoLix main window with a result](docs/user-guide/images/results.png)

> PrePoLix is a hobby project under active development. It is not affiliated with PrePoMax or
> CalculiX. Expect rough edges and please report what you find in the
> [issue tracker](https://github.com/KwentiN-ui/prepolix/issues).

## Features

**Geometry and mesh**
- Import STEP, IGES and BREP geometry, or open an existing CalculiX `.inp` mesh.
- Tetrahedral meshing (1st and 2nd order) with [Gmsh](https://gmsh.info/): per-part meshing
  parameters and local mesh sizes on faces and edges.
- Mesh tools: merge nodes, mirror, transform, renumber.
- 2D models (plane stress, plane strain, axisymmetric), shells, beams and trusses.

**Model setup**
- Materials (elastic, plastic, thermal) with a material library; solid, shell and beam sections.
- Boundary conditions, loads (pressure, surface and body loads, centrifugal, moments), amplitudes,
  initial conditions and defined fields.
- Contact pairs and ties, constraints, rigid bodies, pre-tension, submodeling.
- Node and element sets are created implicitly from your selection, as in PrePoMax.
- Unit systems with unit-aware input, model check, and a keyword editor for anything the GUI does not cover.

**Analysis**
- Static, frequency, buckling, dynamic, modal dynamics, steady-state dynamics, heat transfer and
  coupled temperature-displacement steps, complex frequency with Campbell diagram.
- Start CalculiX from the program and watch the solver output live.

**Post-processing**
- Contour plots, deformed shapes, section views, animations and mode shapes.
- Derived field and history outputs, paths, planes, hot-spot search and result queries.
- Screenshots to file or clipboard.

Projects are saved as `.plx` files.

## Installation

### Windows

Download `prepolix-<version>-setup.exe` from the
[latest release](https://github.com/KwentiN-ui/prepolix/releases/latest) and run it. It installs
PrePoLix with a Start menu entry, bundles Gmsh and can associate `.plx` project files.

### Linux

There is no binary package yet. Build from source, see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

### CalculiX

PrePoLix does not ship the solver. Install CalculiX (`ccx`) separately, for example from
[calculix.de](http://www.calculix.de/) or your package manager (`sudo apt install calculix-ccx`),
and select the executable in *Tools > Settings*. A self-test button there checks that it works.

## Documentation

The **User Guide** is a PDF attached to every [release](https://github.com/KwentiN-ui/prepolix/releases/latest)
(`prepolix-<version>-user-guide.pdf`). It starts with a first analysis step by step and then
describes every dialog and what PrePoLix writes to the CalculiX input file. The source is in
[docs/user-guide/](docs/user-guide/).

## Contributing and development

Building from source, project layout, tests and releases are described in
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## License

PrePoLix is licensed under the GPL-3.0-or-later, see [LICENSE](LICENSE).

Gmsh is licensed under the GPL-2.0-or-later with an exception for OpenCASCADE, Netgen and METIS,
and OpenCASCADE under the LGPL-2.1 with exception; both are compatible with the GPL-3.0. If you
distribute PrePoLix together with the Gmsh library, include `GMSH-LICENSE.txt` and point to the
sources: Gmsh at <https://gitlab.onelab.info/gmsh/gmsh> (tag `gmsh_4_15_2`), OpenCASCADE at
<https://github.com/Open-Cascade-SAS/OCCT>.

The bundled font Noto Sans (`crates/plx-app/assets/fonts`, reduced to Latin, Greek and common
symbols) is licensed under the SIL Open Font License 1.1, see
[OFL.txt](crates/plx-app/assets/fonts/OFL.txt).
