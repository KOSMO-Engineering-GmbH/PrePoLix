#import "../../template.typ": *

== Optimization loops <optimization-loops>

An *Optimizer* block runs a loop: it gives a value to a number of a mesh block, has the
project updated, reads a Measure block (@measure-block) and decides on the next value, until it
is done. Two optimizers are there:

- *Mesh convergence* refines a mesh by a factor until the measure changes less than a
  tolerance from one mesh to the next, so that the results no longer depend on the mesh.
- *Parameter study* solves a list of values, such as three lengths or refinements, and lists
  the measure of each.

Add one with #menu("Project", "Add Optimizer"). Then make the loop:

+ Drag the pin of its #ui("Value") row to the number it varies: #ui("Refine") of a mesh block
  for a mesh convergence, any number of a mesh block for a study. The wire is violet and
  dashed and runs below the blocks, so that the loop can be seen.
+ Drag the value pin of a Measure block of the results on that mesh to its #ui("Measure")
  row.

#screenshot("optimizer-loop.png", [A mesh convergence: the optimizer Convergence varies Refine
  of the box (dashed wire) and reads the largest von Mises stress of the step Load, which the
  analyses Thermal and Structural compute on that mesh])

#ui("Update Project") then runs the loop of every optimizer that is not up to date, each
iteration a whole update: the mesh is made anew, the analyses are solved, the measure is
computed. The output pane prints each iteration and how the loop ended. The block shows the
iterations so far with the last measure, and its value; #ui("Stop") of the project window
ends the loop after it stopped the running solves. The results of the iterations are not kept:
the analyses hold those of the last one. The block is up to date while its measure is what the
loop ended on; when the analyses change, for example a load, the next #ui("Update Project")
runs the loop again from the start. Its context menu has #ui("Run"), which runs the loop at
once, and #ui("Reset"), which forgets the iterations and gives the start value.

#screenshot("optimizer-editor.png", [The editor of an optimizer after a mesh convergence that
  did not converge: the largest stress at the fixed end grows with every refinement])

A double-click on the block opens its editor, with the settings, the table of the iterations
of the last run and a diagram of the measure over the elements (mesh convergence) or over the
value (study):

#fields(
  [Optimizer], [#ui("Mesh convergence") or #ui("Parameter study").],
  [Start refinement], [The refinement of the first iteration; 1 meshes the divisions of the
    mesh block as they are.],
  [Factor per step], [The refinement grows by this factor from one iteration to the next. The
    divisions are rounded, so in 3D the elements grow about by its cube.],
  [Tolerance], [The relative change of the measure below which the mesh has converged.],
  [Max iterations], [The loop ends after this many iterations, converged or not.],
  [Max elements], [No mesh with more elements is solved: the loop ends before it.],
  [Kind, Value 1 ...], [For a study: what the values are (#ui("Length"), #ui("Count") or
    #ui("Factor")) and the values, in the units shown in.],
  [Run, Reset], [As in the context menu.],
)

A converged mesh convergence leaves the refinement on its last mesh. A loop that does not
converge says so, and when the measure grew with every refinement it points out a
singularity: a stress at a sharp corner or under a point load grows without end as the mesh
gets finer. Measure a displacement there, or a region away from it.
