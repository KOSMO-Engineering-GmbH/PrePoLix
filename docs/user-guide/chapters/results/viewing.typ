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

Results list the parts of the model they were computed from, as in the #ui("FE Model") tab, also
when several parts share a section. A `.frd` file itself only groups the elements by material;
PrePoLix reads the parts from the input file CalculiX ran, the `.inp` file of the same name next
to the results. Results of an analysis run from PrePoLix and results bound to a project always
have it. A `.frd` file opened without its input file shows one part per material.

#screenshot("results-parts.png", [Three parts sharing one section keep their names in the results])

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
  [Color levels], [Number of colour bands (2 to 24).],
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

#screenshot("results-mode.png", [First mode shape of a frequency step])

=== Parts without colour contours <color-contours>

A stiff or irrelevant part can dominate the colour scale. Right-click a part under
#ui("Model") in the results tree, or in the 3D view, and choose #ui("Color Contours Off"). The
part turns plain grey and no longer counts for the legend range, the minimum and maximum labels
or the animation limits; the colours of the other parts then use the full scale.
#ui("Color Contours On") brings the contours back. With several parts selected, the command
applies to all of them. Results bound to a project (@bind-result) remember which parts have
contours off. Nodes that a grey part shares with a part that still has contours keep
their colour.

#screenshot("color-contours-off.png", [The left part without colour contours])

Increment 0 of a buckle step is the reference state; the modes follow as increments 1, 2, ...
