#import "../../template.typ": *

== Steps <steps>

A step is one analysis of CalculiX with its own boundary conditions, loads and outputs. Create it
with #menu("Model", "Create Step ...") or #ui("Create ...") on #ui("Steps"). A new step copies the
boundary conditions and loads of the last step, as far as its type accepts them. The type of a
step cannot be changed after it was created.

#note[Every step writes `OP=NEW` for its boundary conditions and loads. What applies in a step is
exactly what is listed under it in the tree.]

#screenshot("step.png", [Static step])

=== Order of the steps

CalculiX runs the steps in the order of the list, and a step starts from the state at the end of
the previous one. To change the order, right-click a step in the #ui("Steps") list and choose
#ui("Move Up") or #ui("Move Down"). The step keeps its boundary conditions, loads, defined
fields and outputs, and stays selected. #ui("Move Up") is disabled on the first step and
#ui("Move Down") on the last.

#screenshot("step-move-menu.png", [Context menu of a step])

Steps that build on the previous one, such as a step with #ui("Perturbation"), take the state of
whatever step now precedes them, so check the order after moving. The step numbers of existing
results refer to the order at the time of the run.

=== Solver

All step types have a #ui("Solver") field: #ui("Default (Pardiso or PaStiX if available)"),
#ui("Pardiso"), #ui("Spooles"), #ui("PaStiX"), #ui("Iterative scaling") or
#ui("Iterative Cholesky"). #ui("Default") uses Pardiso if the CalculiX executable contains it,
otherwise PaStiX if it contains that, otherwise the default of CalculiX. The CalculiX shipped
with PrePoLix contains PaStiX and SPOOLES; on models with many nodes PaStiX is several times
faster. Frequency and buckle steps need a direct solver.

=== Static

#fields(
  [Nonlinear geometry (Nlgeom)], [Large deformations (`NLGEOM`).],
  [Incrementation], [#ui("Default") leaves the incrementation to CalculiX. #ui("Automatic") uses
    the time and increment values below, #ui("Direct") uses fixed increments.],
  [Max. increments], [Largest number of increments (`INC`).],
  [Time period], [Length of the step (default 1).],
  [Initial / Min. / Max. increment], [Increment sizes for automatic incrementation.],
)

=== Frequency

Computes eigenfrequencies and mode shapes (`*FREQUENCY`). A frequency step takes no loads.

#fields(
  [Perturbation], [Uses the stress state of the previous step (prestress).],
  [Number of eigenfrequencies], [Default 10.],
  [Lower / Upper frequency bound], [Optional range of frequencies to compute.],
  [Store matrices and eigenmodes], [Writes the `.eig` file (`STORAGE=YES`).],
)

Materials need a density in a frequency step.

=== Buckle

Computes buckling factors and buckling modes (`*BUCKLE`). The loads of the step are the reference
load: the critical load is the buckling factor times these loads.

#fields(
  [Preload from previous step (Perturbation)], [Takes the state of the previous step as
    preload.],
  [Number of buckling factors], [Default 1.],
  [Accuracy], [Convergence tolerance of the eigenvalue solver (default 1e-4).],
)

=== Heat Transfer, Coupled and Uncoupled Temperature-Displacement

#ui("Heat Transfer") computes temperatures only (`*HEAT TRANSFER`). #ui("Coupled
Temperature-Displacement") computes temperatures and displacements together in one system of
equations (`*COUPLED TEMPERATURE-DISPLACEMENT`). #ui("Uncoupled Temperature-Displacement")
(`*UNCOUPLED TEMPERATURE-DISPLACEMENT`) solves the temperatures first and then the displacements
from them in each increment. It is faster and suits thermal stresses as long as the deformation
does not change the heat flow, for example through a contact that opens. The step takes the same
boundary conditions and loads as the coupled step: temperatures, heat fluxes, films and radiation
together with supports and mechanical loads.

#screenshot("uncoupled-step.png", [Creating an uncoupled temperature-displacement step])

#fields(
  [Steady state], [Steady-state solution (default on). Switch it off for a transient
    analysis; materials then need density and specific heat, and the model an initial
    temperature.],
  [Max. temperature change], [Largest temperature change per increment in a transient analysis
    (`DELTMX`).],
  [Nonlinear geometry], [Coupled steps only, as in PrePoMax.],
)

Incrementation and solver fields are the same as for a static step.

=== Field outputs <step-field-outputs>

Field outputs control what CalculiX writes to the result file (`.frd`) as `*NODE FILE`,
`*EL FILE` and `*CONTACT FILE`. A new step gets a node and an element field output whose
variables depend on the step type. Create more with #ui("Create ...") on #ui("Field Outputs") of a
step or with #menu("Model", "Create Field Output ..."). Edit one with a double-click. Hovering over
a variable shows what it is.

#fields(
  [Type], [#ui("Node Output"), #ui("Element Output") or #ui("Contact Output"); chosen when the
    output is created.],
  [Variables], [Node: `RF U PU V NT PNT RFL SEN` (`SEN` in sensitivity steps only). Element: `S PHS E ME PEEQ ENER HFL ERR HER ZZS SDV`;
    `ERR` and `HER` exclude `ZZS`. Contact: `CDIS CSTR CELS PCON`.],
  [Last iterations], [#ui("On") writes every iteration of the last increment, to find out why a
    solution diverges.],
  [Contact elements], [#ui("On") writes the contact elements to a `.cel` file.],
  [Output (2D/3D)], [Element outputs only. #ui("3D") writes 1D and 2D elements in their expanded
    3D form; #ui("Default") writes shell and plane models in 2D.],
  [Global], [Node and element outputs. #ui("No") writes the results in the local coordinate
    systems.],
)

#screenshot("field-output-dialog.png", [Creating an element field output])

Like boundary conditions, field outputs can be deactivated and deleted from their context menu.
A deactivated field output is left out of the input file.

#screenshot("field-output-tree.png", [A deactivated contact field output])

#note[Field outputs of a step are what CalculiX computes. The field outputs on the #ui("Results")
tab derive new fields from opened results instead (@derived-fields).]

=== History outputs

History outputs write values of a region to the `.dat` file in every increment (`*NODE PRINT`,
`*EL PRINT`, `*CONTACT PRINT`). Create them with #ui("Create ...") on #ui("History Outputs") of a
step. Hovering over a variable shows what it is.

#fields(
  [Type], [#ui("Node Output") (region of nodes), #ui("Element Output") (parts, element set or
    selection) or #ui("Contact Output") (a contact pair).],
  [Variables], [Node: `RF U V NT RFL`. Element: `S E ME PEEQ HFL ENER ELSE ELKE EVOL EBHE`.
    Contact: `CDIS CSTR CELS CNUM CF`.],
  [Totals], [#ui("Yes") adds the sum over the region, #ui("Only") writes only the sum.],
)

When results are opened, the `.dat` file is read too, and the history outputs appear on the
#ui("Results") tab (@history-outputs).
