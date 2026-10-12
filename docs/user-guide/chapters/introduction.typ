#import "../template.typ": *

= Introduction

PrePoLix is a pre- and post-processor for the finite element solver
#link("http://www.calculix.de/")[CalculiX]. It runs on Linux and Windows. Its look, workflow
and feature set follow #link("https://prepomax.fs.um.si/")[PrePoMax], so users of PrePoMax will
find most dialogs where they expect them.

With PrePoLix you

- import CAD geometry (STEP, IGES, BREP) and mesh it with Gmsh, or open an existing CalculiX mesh
  (`.inp`),
- define the finite element model: materials, sections, constraints, contacts, steps, boundary
  conditions and loads,
- keep several analyses in one project, as in Ansys Workbench, and solve only what changed,
- run CalculiX and watch its progress,
- view and evaluate the results (`.frd`, `.dat`): contour plots, deformed shapes, animations,
  derived fields, history plots, paths, planes and hot spot stresses.

This guide is a reference. Each chapter describes one part of the program: where to find a
function, what every field of its dialog means, and what PrePoLix writes to the CalculiX input
file. You do not need to read it from front to back.

== How PrePoLix thinks about a model

You do not need to define node sets, element sets or surfaces. Instead you pick faces, edges,
parts or nodes in the 3D view, and the model stores your _intent_: "a pressure of 2 MPa on these
faces", "these two parts are tied". Only when the input file is written does PrePoLix turn this
intent into CalculiX keywords: it creates the required node sets, element sets and surfaces with
generated names and splits a distributed load into what CalculiX needs. Named sets of your own
are still possible (@mesh-sets).

This has two consequences you will notice:

- When you remesh a part, loads and boundary conditions that were picked on the CAD geometry
  stay where they are, because they refer to faces and edges of the geometry, not to element
  numbers.
- The `.inp` file contains sets you never named. You can inspect it at any time in the keyword
  editor (@keyword-editor) or export it (#menu("File", "Export CalculiX Input File ...")).

The project, including geometry, mesh and model, is saved as a PrePoLix project file (`.plx`).

== Requirements

- *CalculiX* (`ccx`) is needed to run analyses. The Windows installer and the Linux AppImage
  include CalculiX 2.23 with the PaStiX, SPOOLES and ARPACK solvers, so analyses run right after
  installation. Another CalculiX, for example one built with Pardiso, can be set under
  #menu("Tools", "Settings ...", "CalculiX"); #ui("Test CalculiX") checks it (@settings).
- *Gmsh* is needed to import and mesh CAD geometry. The Windows installer and the Linux AppImage
  include it. Without
  Gmsh PrePoLix still starts; only geometry import and meshing are unavailable. Its library path
  can be set under #menu("Tools", "Settings ...", "Gmsh").
- On Linux, PrePoLix needs a Vulkan or OpenGL driver and `libxkbcommon`, which most desktop
  installations provide. A Gmsh library installed separately additionally needs `libGLU` and
  `libXft`; the AppImage brings them along.

== Installation

*Windows.* Run `prepolix-<version>-setup.exe` from the release page. It installs PrePoLix with
Gmsh and CalculiX to `C:\Program Files\prepolix`, creates a start menu entry, optionally a desktop icon, and
can associate `.plx` project files with PrePoLix. Uninstall it under _Apps_ in the Windows
settings. Because programs may not write to `C:\Program Files`, the CalculiX work directory then
defaults to `%TEMP%\prepolix`.

*Linux.* Download `prepolix-<version>-x86_64.AppImage` from the release page, make it executable
(`chmod +x prepolix-*-x86_64.AppImage`) and start it. It includes Gmsh and CalculiX and runs on x86_64
distributions with glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later). A tool
such as Gear Lever or AppImageLauncher adds a menu entry, associates `.plx` files and updates
the AppImage when a new release is published. The CalculiX
work directory defaults to `/tmp/prepolix`. Alternatively build PrePoLix from source with a
current stable Rust toolchain (`cargo run --release`), see the README of the repository.

=== Portable version for Windows <portable>

Where installing is not possible, for example on a company computer without administrator
rights, download `prepolix-<version>-windows-portable.zip` from the release page instead. Unpack
it to any folder you can write to, for example in your documents or on a USB stick, and start `prepolix.exe` in the folder `PrePoLix-<version>`. The
folder contains the same program, Gmsh, CalculiX and this guide as the installer, but PrePoLix
does not register itself with Windows: there is no start menu entry and no file association, and
nothing is written to the registry. Settings and the material library are kept in the folder
`Settings` next to `prepolix.exe`, the CalculiX work directory defaults to the folder `Temp`
there, and the settings window shows #ui("Portable mode") at its bottom. The file `portable.txt`
switches this on; without it the settings go to `%APPDATA%\prepolix\data` as with the
installer. To update, unpack the new version (the update window links to its ZIP file, see
@updates) and copy the `Settings` folder over; to remove PrePoLix, delete the folder.

#screenshot("portable.png", [The portable version under Windows: CalculiX and its work directory
in the unpacked folder, the note at the bottom])

== Updates <updates>

PrePoLix asks GitHub for the latest release when it starts. If a newer version exists, a green
symbol with a download arrow appears at the right of the menu bar, next to the version number.
Nothing else happens: ignore it if you do not want to update. A click on the symbol, or
#menu("Help", "Check for Updates ..."), opens the update window with the release notes
(_What's Changed_, one line per change with a link to it on GitHub):

#screenshot("update-available.png", [Update symbol next to the version and the update window of a
  PrePoLix built from source; with the Windows installer it also offers #ui("Update Now")])

- #ui("Update Now") (Windows installer only) downloads the installer of the new version, checks
  its size and checksum and starts it. Windows asks for administrator rights, PrePoLix closes,
  the installer only shows its progress and then starts PrePoLix again; the shortcuts and the
  `.plx` association stay as you installed them. Save your project before: the project that was
  open is reopened when it was saved as a `.plx` file.
- #ui("Download ZIP") (portable version only, @portable) downloads the new portable ZIP file in
  the web browser. Unpack it yourself and copy the `Settings` folder over.
- #ui("Open Release Page") opens the release on GitHub. A PrePoLix built from source offers only
  this.
- #ui("Close") closes the window; the symbol stays until PrePoLix is closed.

The Linux AppImage does not check at startup, since an AppImage manager such as Gear Lever or
AppImageLauncher updates it; #menu("Help", "Check for Updates ...") still shows what is new. The
check at startup runs in the background and stays silent when there is nothing new or no
internet connection. It can be turned off under #menu("Tools", "Settings ...", "Updates")
(@settings). A proxy is taken from the `HTTPS_PROXY` variable or, on Windows, from a proxy server
entered in the Internet Options (automatic configuration scripts are not read). The connection is
checked with the certificates of the system, so a company proxy that inspects HTTPS works too.

== Starting PrePoLix

Start PrePoLix from the start menu, by double-clicking a `.plx` file (Windows, if associated),
by double-clicking `prepolix.exe` in the portable folder (@portable), or from a terminal. Files
given on the command line are opened in order, so a model and its results can be opened
together:

```sh
prepolix bracket.plx bracket.frd
```

Several CAD files given together are combined into one geometry. Without command line arguments,
PrePoLix reopens the project that was open when it was last closed, provided it was saved as a
`.plx` file and still exists.

== This guide offline <offline-guide>

The Windows installer, the portable ZIP and the Linux AppImage contain this guide as a web page.
#menu("Help", "User Guide") opens it in the default web browser; no internet connection is
needed. On Linux PrePoLix hands the page to the browser through a small web server on your
computer (address `http://127.0.0.1:<port>/`), because browsers installed as Snap or Flatpak
cannot read the files of a mounted AppImage. A PrePoLix built from source without the guide opens
the online guide at #link("https://kwentin-ui.github.io/PrePoLix/") instead. The guide is also
available as PDF on the release page on GitHub.
