#import "../../template.typ": *

== Settings <settings>

#menu("Tools", "Settings ...") opens the settings window. The pages are listed on the left.
#ui("Defaults") resets all pages, #ui("Apply") applies without closing. Settings are stored per
user (`~/.local/share/prepolix` on Linux, `%APPDATA%\prepolix\data` on Windows), by the
portable Windows version in the folder `Settings` next to `prepolix.exe` (@portable).

#screenshot("settings.png", [Settings, CalculiX page])

=== Graphics

#fields(
  [Color theme], [#ui("Light") is PrePoMax's light look, #ui("Dark") shows dark panels, a dark
    3D background and light text over the model. #ui("System") (default) follows the light or
    dark mode of the operating system and switches along with it. Icons, tree symbols, plots and
    annotations change with the theme; exported SVG plots and the images of the user guide stay
    light.],
  [Global axes at the origin], [Draws a coordinate triad at the global origin (default on).],
  [Coordinate triad in the corner], [The clickable triad in the lower right (default on).],
  [Scale bar], [Shows a scale bar (default on).],
)

#screenshot("dark-theme.png", [The dark theme with the results of a cantilever])

=== Post-processing

#fields(
  [Show label at maximum / minimum], [Labels at the node with the largest / smallest value
    (default: maximum on, minimum off).],
  [Show status block], [The info block with file, step, increment and scale factor.],
  [Show undeformed outline], [Draws the undeformed model as a wireframe for newly opened
    results.],
  [Color levels], [Number of colour bands of the legend for newly opened results (2 to 24,
    default 9).],
  [FFmpeg executable], [FFmpeg for exporting animations as MP4 or MKV (@animation-export);
    empty for `ffmpeg` on the `PATH`.],
)

=== CalculiX

#fields(
  [Executable], [Path to `ccx`. Empty means the CalculiX shipped with PrePoLix (shown greyed
    in the field), or `ccx` on the system path if PrePoLix was built without it. A bare name
    such as `ccx` is looked up on the system path.],
  [Threads], [Number of threads CalculiX uses (`OMP_NUM_THREADS`), default 1.],
  [Work directory], [Where input and result files of a run are written. Empty means a `Temp`
    folder next to the PrePoLix program, or `<system temp>/prepolix` if that folder is not
    writable.],
  [Equation solvers], [The direct solvers found in the CalculiX executable, for example
    PARDISO. Steps with the solver #ui("Default") use Pardiso when it is available, otherwise
    PaStiX, otherwise CalculiX's own choice.],
  [Test CalculiX], [Starts CalculiX, computes a cantilever under a tip load and under pressure,
    compares the results with beam theory, checks which solvers are available and whether
    CalculiX knows every keyword PrePoLix writes. Each test shows OK or an error.],
)

#screenshot("calculix-test.png", [Result of #ui("Test CalculiX")])

*Keywords.* Older CalculiX versions lack keywords of newer features, for example
`*COMPLEX FREQUENCY` (complex frequency step) or `*CYCLIC SYMMETRY MODEL`. CalculiX does not stop
at an unknown keyword: it skips the card with a warning and computes without it. The test
therefore reads each keyword PrePoLix can write on its own and lists the unknown ones by the
feature that needs them, e.g. `Complex frequency step (*COMPLEX FREQUENCY)`. Every analysis
checks the keywords of its own input file in the same way, including those of the keyword editor
(@model-check).

=== Gmsh

#fields(
  [Library], [Path to the Gmsh library (`libgmsh.so` or `gmsh-4.15.dll`). Empty means: next to
    the program, then the system path. A changed library is loaded after a restart.],
  [Test Gmsh], [Loads Gmsh and meshes a cube.],
)
