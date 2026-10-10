#import "../../template.typ": *

= Results <results>

== Opening results

Results are opened with #menu("Analysis", "Open Results") after a run, with #ui("Results") in the
monitor, or with #menu("File", "Open ...") for any `.frd` file (ASCII or binary). A `.dat` file
with the same name is read as well. Several result files can be open at the same time; the
#ui("Result") box on the toolbar switches between them. #menu("Results", "Close Current
Results") and #ui("Close All Results") close them.

When a result file is opened, PrePoLix shows the #ui("Results") tab with the last increment.

#screenshot("results.png", [von Mises stress of a static step])

== Results tree

/ Model: the mesh of the results with its parts and sets, and the features (reference points,
  coordinate systems, planes) used for result evaluation.
/ Field Outputs: the fields of the result file with their components. Click a component to show
  it. Names follow PrePoMax (`U1`, `S11`, ...). PrePoLix adds the displacement magnitude `ALL`,
  `MISES`, `TRESCA`, the principal stresses and the signed maximum absolute principal stress.
/ History Outputs: values from the `.dat` file and history outputs you create (@history-outputs).
/ Hot Spot Stresses, Paths, Plane Results: evaluations described in the following sections.

== Results toolbar <results-toolbar>

#screenshot("results-toolbar.png", [Results toolbar])

#fields(
  [Result], [The displayed result file.],
  [Deformation], [#ui("Undeformed"), #ui("True scale"), #ui("Automatic") with factors from
    0.25 to 5, or #ui("User defined"). Results open in #ui("Automatic"); static and dynamic
    steps of a model with contact open in #ui("True scale"), so touching parts do not appear to
    penetrate each other, while mode shapes keep #ui("Automatic"). Contact is recognised by a
    contact pair in the input file next to the results (same name, as CalculiX leaves it) or
    contact results in the file. Until you pick a deformation yourself, it follows the shown
    step.],
  [Factor], [The deformation scale factor; editable for #ui("User defined").],
  [Show Undeformed], [Draws the undeformed outline behind the deformed shape.],
  [Color levels], [Number of colour bands (2 to 24). The #ui("...") button next to it opens the
    colour scale (@color-scale).],
  [Transformations], [Symmetries and patterns (@transformations).],
  [Step, Increment], [The displayed increment, with buttons for the first, previous, next and
    last increment.],
  [Animation], [Opens the animation window (@animation).],
  [Sound of the mode shapes], [Opens the sound window for modes of a frequency step
    (@sound).],
)

== Contour plot, legend and labels

The legend shows the field, the unit and the range in nine colour bands from blue (minimum) to red
(maximum) by default. The info block in the upper right shows file, date, step, increment or mode
with its frequency or buckling factor, and the deformation scale factor. Labels mark the node with
the maximum (and optionally the minimum) value. Legend, info block and labels can be dragged.
Which of them appear is set under #menu("Tools", "Settings ...", "Post-processing").

== Color scale <color-scale>

Each result file has its own colour scale. Double-click the legend, choose #ui("Color Scale...")
in its context menu, click #ui("...") next to #ui("Color levels") on the results toolbar, or use
#menu("Results", "Color Scale ..."). Changes show at once; #ui("Cancel") restores the scale the
dialog was opened with and #ui("Defaults") returns to PrePoMax's rainbow with automatic limits.

#screenshot("color-scale-dialog.png", [Color Scale dialog with manual limits: values above 40 MPa
and below 5 MPa are grey])

#fields(
  [Spectrum], [Cool-warm, Rainbow (default), Rainbow desaturated, Turbo, Jet, Warm, Cool,
    Cividis, Viridis, Plasma, Black body, Inferno, Kindlmann or Grayscale, as in PrePoMax.],
  [Brightness], [Lightens the colours towards white, from 0 to 1 (default 0.2).],
  [Reverse colors], [Puts the maximum colour at the minimum and vice versa.],
  [Number of colors], [The colour bands, the same as #ui("Color levels") on the toolbar.],
  [Min/max limit type], [#ui("Automatic") spans the smallest to the largest shown value (over
    all frames of an animation if it is set so). #ui("Manual") uses #ui("Minimum") and
    #ui("Maximum"); the values are kept exactly as typed. The minimum must be smaller than the
    maximum.],
  [Above maximum, Below minimum], [Colour of the values beyond the manual limits (dark and light
    grey by default). The legend shows them as an extra box labelled with the largest or
    smallest value unless #ui("Show in legend") is cleared.],
)

The last line of the legend title says #ui("Automatic") or #ui("Manual"). When the result is
bound to the project (@bind-result), its colour scale is saved in the project file and restored
with it; a result file that is read again, for example after another run, keeps its scale.

#screenshot("color-scale-cool-warm.png", [The same stress in the Cool-warm spectrum with
automatic limits])

#screenshot("results-mode.png", [First mode shape of a frequency step])

Increment 0 of a buckle step is the reference state; the modes follow as increments 1, 2, ...
