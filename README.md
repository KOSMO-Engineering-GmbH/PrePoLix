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

PrePoLix needs two other programs: the solver CalculiX (not bundled on any platform) and, for
geometry import and meshing, Gmsh (bundled by the Windows installer only). See the sections below.

### Windows

Download `prepolix-<version>-setup.exe` from the
[latest release](https://github.com/KwentiN-ui/prepolix/releases/latest) and run it. It installs
PrePoLix with a Start menu entry, bundles Gmsh and can associate `.plx` project files.

### Linux

There is no binary package yet, but you can install straight from the Git repository with one
command. You do not need to clone it. You need a current stable Rust toolchain ([rustup](https://rustup.rs/)):

```sh
cargo install --git https://github.com/KwentiN-ui/PrePoLix plx-app
```

This builds the `prepolix` program and puts it in `~/.cargo/bin`. The first build takes a few
minutes. Building needs the ALSA development files (Debian/Ubuntu: `sudo apt install libasound2-dev pkg-config`,
Fedora: `sudo dnf install alsa-lib-devel`). At run time Linux needs `libxkbcommon-x11` (X11) or
`libxkbcommon` (Wayland) and a Vulkan or OpenGL driver, which most desktops already have.

PrePoLix also needs the Gmsh library, which `cargo install` does not provide, see [Gmsh](#gmsh) below.
To update later, run the same command again with `--force`. To build from a clone instead, see
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

### Gmsh

PrePoLix imports STEP, IGES and BREP geometry and meshes it with [Gmsh](https://gmsh.info/). Gmsh is
not part of PrePoLix and is loaded as a library at run time. The Windows installer bundles it. On
Linux you provide it yourself until a bundled package exists. **Without Gmsh** PrePoLix starts and
works normally, but geometry import and meshing are not available. Opening `.inp` meshes and `.frd`
results, model setup, running CalculiX and post-processing do not need it.

On Linux, the simplest way is to take the official library (Gmsh 4.15, the tested version) from its
PyPI package and put it next to the program. No clone and no Python packages are needed beyond `pip`:

```sh
pip download gmsh==4.15.2 --no-deps --only-binary=:all: --platform manylinux_2_24_x86_64 -d /tmp/gmsh
mkdir -p ~/.cargo/bin/gmsh
unzip -j /tmp/gmsh/gmsh-4.15.2-*.whl '*/lib/libgmsh.so.4.15' '*/doc/gmsh/LICENSE.txt' -d ~/.cargo/bin/gmsh
mv ~/.cargo/bin/gmsh/LICENSE.txt ~/.cargo/bin/gmsh/GMSH-LICENSE.txt
```

The library also needs `libGLU` and `libXft` (Debian/Ubuntu: `sudo apt install libglu1-mesa libxft2`).
PrePoLix looks for the library, in this order, at the path set in *Tools > Settings > Gmsh*, in the
environment variable `PREPOLIX_GMSH`, next to the program and in its `gmsh` subfolder, and finally in
the system library path. If you already have Gmsh installed (for example `libgmsh` from your
distribution), point the setting or `PREPOLIX_GMSH` to it; other Gmsh 4 versions may work but are
not tested. The *Test Gmsh* button in the settings loads the library and meshes a cube. From a clone,
`python3 scripts/fetch_gmsh.py` does the same, see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md#gmsh).

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
