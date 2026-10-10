#import "../../template.typ": *

= Running CalculiX <analysis>

== Running an analysis

A model has one analysis, #ui("Analysis-1"). It uses the CalculiX settings under
#menu("Tools", "Settings ...", "CalculiX") (@settings).

#fields(
  [Run Analysis (#key("F5"))], [Writes `Analysis-1.inp` to the work directory and starts
    CalculiX. Old result files of the analysis in that directory are deleted first.],
  [Check Model], [Lets CalculiX only read and check the model (`*NO ANALYSIS`), without solving
    it.],
  [Kill Analysis], [Stops a running analysis.],
  [Monitor], [Shows the monitor window.],
  [Open Results], [Opens the result file of the last run.],
)

These commands are in the #ui("Analysis") menu and in the context menu of #ui("Analysis-1").
Before CalculiX starts, PrePoLix refuses to run a model without mesh, without step or with all
steps deactivated, and it stops if the input file cannot be written (for example because a region
is empty).

=== Monitor

The monitor opens when a run starts. It shows the status and running time, the current step,
increment and iteration, and the output of CalculiX. #ui("Results") opens the results when the run
is finished.

#screenshot("monitor.png", [Monitor after a completed run])

The status at the end is #ui("completed"), #ui("finished with errors, results available"),
#ui("failed") or #ui("killed"). If CalculiX failed, PrePoLix looks for known error messages
in the output and adds a line "Possible cause: ..." with a remedy, for example for distorted
elements, missing material data, rigid body motion or missing convergence.

== Binding the result to the project <bind-result>

Normally a project file (`.plx`) holds only the mesh and the model definition; the results stay
in the work directory and are overwritten by the next run. To keep a result with the project,
right-click #ui("Analysis-1") after a finished run and choose #ui("Bind Result"). The input file,
the result file (`.frd`) and the history output (`.dat`) of the run are then saved in the project
file with the next #menu("File", "Save"). The output pane shows how much the files add to the
project file; they are stored compressed, at about a third of their size (a 236 MB result file
adds about 100 MB). Binding a large result takes a few seconds and runs in the background.

When a project with a bound result is opened, PrePoLix writes the files to
`Bound_Results/<project name>` in the work directory and opens the result on the #ui("Results")
tab. #ui("Analysis-1") then shows the status of the bound run, its tooltip says that a result is
bound, and #ui("Results") in its context menu opens the bound result again.

#screenshot("bind-result.png", [Context menu of an analysis with a bound result])

#fields(
  [Bind Result], [Binds the result of the last run to the project, replacing a result bound
    before. Available after a run in this session that wrote a result file.],
  [Remove Bound Result], [Removes the bound result. Save the project to drop it from the file.],
)

To send only the mesh and the model definition, use #menu("File", "Save Without Results ...").
It saves a copy without the bound result under a new name (proposed as
`<project name>-without-results.plx`); the open project stays on its own file and keeps its
result. The entry is available while a result is bound.

#note[Binding is a snapshot: a later run does not change the bound result until you bind again.]

== Model check <model-check>

PrePoLix checks the model continuously while you work. Items with a problem get a warning sign in
the tree; items with an error and their containers are shown in red. Click the warning sign to see
what is wrong and how to fix it.

#screenshot("model-check.png", [Model check: parts without a section])

Errors include elements without material, materials without the data the step needs (elasticity,
density, conductivity, specific heat), invalid material constants, distorted elements, a missing
initial temperature in a transient heat transfer step, an increment larger than the step, rigid
body motion of a part in a static step, truss structures that are mechanisms and missing
references (a deleted material, part or set that an item still uses). Warnings include parts held
only by contact, conflicting boundary conditions, loads on fixed nodes, rotations without effect
and steps without load.

The checks do not stop a run; their errors are listed at the top of the monitor. Missing
references do stop it, because the input file cannot be written.

== Exporting the input file <export-inp>

#menu("File", "Export CalculiX Input File ...") writes the same input file as a run, for example
to run CalculiX by hand or on another computer. Deactivated items appear as comments.

== Submodels <submodel>

A submodel computes a detail of a larger model more precisely. Its cut boundaries follow the
displacements of the global model.

+ Compute the global model and keep its result file (`.frd`).
+ In the submodel, open #menu("Model", "Model Properties ...") and set #ui("Model Type") to
  #ui("Submodel"). Choose the global result file under #ui("Global results .frd").
+ In a static step, create a boundary condition of type #ui("Submodel") on the cut faces. Choose
  the #ui("Global step") and the displacement components.
+ Run the analysis. PrePoLix copies the global results next to the input file and writes
  `*SUBMODEL` and `*BOUNDARY, SUBMODEL`.

#limitation[Only displacements can be transferred. The global result file must not be the result
file of the submodel's own analysis.]

== Exporting a deformed mesh <export-deformed>

#menu("File", "Export Deformed Mesh (.inp) ...") writes the mesh of the displayed result with its
nodes moved by the displacements times the current deformation scale factor. A typical use is an
imperfection for a buckling analysis: show the buckling mode, set #ui("Deformation") to #ui("User
defined") with the imperfection amplitude as factor, export, and open the file as the mesh of a new
model. The file contains only the mesh.
