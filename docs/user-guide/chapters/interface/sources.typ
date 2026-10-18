#import "../../template.typ": *

== Source blocks <source-blocks>

Besides analyses, the project schematic holds *source blocks*: blocks that only give data to
analyses through links. They have no steps and are not solved, and #ui("Update Project")
passes them by; the mesh export block at the end of this section takes data instead of giving
it. Add one from #menu("Project") or by double-clicking or right-clicking the background of the schematic;
its header has its own tint, so that it stands apart from the analyses. Its context menu has
#ui("Edit ..."), #ui("Rename ..."), #ui("Duplicate") and #ui("Delete ..."); a double-click
opens its editor in the project window. Deleting a source block removes its links; the
analyses keep copies of what they took.

=== Materials <materials-block>

A materials block holds materials in the project file, so that several analyses use the same
definition: change it once in the block and every analysis that links it follows. Unlike the
material library (@materials), which belongs to you and every project you open, the block
belongs to the project and travels with it.

#screenshot("materials-block.png", [A materials block: Static takes S235, Modal takes all of
  them])

Its rows are #ui("Materials"), which gives all of its materials, and below it one row per
material; the arrow in front of #ui("Materials") hides and shows them. Drag a row to the
#ui("Setup") row of an analysis, or to its #ui("Materials") row or a material once
#ui("Setup") is open, as with links between analyses (@data-links). A link of all materials
also passes on materials added to the block later; while the block shows its materials, it is
drawn as a wire per material. In the analysis window the linked
materials carry the link sign and are read-only; #ui("Open Materials") in their dialog opens
the block's editor.

#screenshot("materials-block-editor.png", [The editor of a materials block])

#fields(
  [Shown in], [The unit system the values are shown and typed in. Like the values of an
    analysis they are stored in SI units, so each analysis shows them in its own.],
  [New ...], [Defines a new material in the dialog of the FE Model tree (@materials).],
  [Edit ...], [Edits the selected material; a double-click on it does the same.],
  [Delete], [Removes the selected material.],
  [Add from Library ...], [Opens the material library (@material-library) with the block's
    materials in place of the model's: copy materials from the library into the block, or from
    the block into the library.],
  [Import from Analysis], [Copies the materials of an analysis into the block, so that a
    project can gather them in one place afterwards. A material of the same name is replaced.],
)

#ui("OK") keeps the changes, and the analyses that link the block take them at once;
#ui("Cancel") drops them.

=== Box and Rectangle meshes <mesh-block>

A mesh block makes a structured mesh from a few numbers, without geometry and without Gmsh:
#menu("Project", "Add Mesh", "Box") meshes a box with hexahedra for 3D analyses,
#menu("Project", "Add Mesh", "Rectangle") a rectangle in the X-Y plane with quadrilaterals for
2D analyses. Link its #ui("Mesh") row to the #ui("Mesh") row of analyses of the same model
space; several analyses can share one mesh block.

#screenshot("mesh-block.png", [A box linked to an analysis, its parameters in the block])

Its parameters are rows of the block under #ui("Mesh"), like the inputs of a Blender node:
#ui("X"), #ui("Y"), #ui("Z") of the corner it starts at and the edge lengths #ui("Lx"),
#ui("Ly"), #ui("Lz") in the block's units, the numbers of elements #ui("nx"), #ui("ny"),
#ui("nz") as whole numbers and the #ui("Element") type. Click a field to type a value, or drag
it; a rectangle has no Z rows. Once a parameter changes, the #ui("Mesh") row shows the mesh out
of date. The violet pins on the left of the numbers take a value from a Parameters block
(@parameters-block). To make the mesh finer or coarser as a whole, put a Refine Mesh block
after it (@refine-mesh).

Its sides are node sets and surfaces at once, named after the axis and the side they lie on:
#ui("X-") at the smallest X, #ui("X+") at the largest, then #ui("Y-"), #ui("Y+") and, for a
box, #ui("Z-") and #ui("Z+"). Boundary conditions and loads pick them under #ui("Node set") or
#ui("Surface") without selecting anything. The elements form the part #ui("BOX") or
#ui("RECTANGLE"), which a section takes.

The sides, edges and corners are also faces, edges and points of geometry. A selection in
the 3D view with #ui("Faces, edges and points") picks a whole side of the box, and it stays on
that side when the mesh is made anew with other divisions, other lengths or another
refinement, as on imported geometry. A selection of single nodes or element faces holds only
for the mesh it was made on.

#screenshot("box-face-remeshed.png", [A boundary condition picked on the end face of a box with
  12 x 2 x 2 elements, after the box was made anew with 24 x 4 x 2 elements])

A double-click on the block's name opens all its parameters in a dialog, with what the rows
leave out: the model space of a rectangle and the units the lengths are shown in.

#screenshot("mesh-block-editor.png", [The parameters of a box])

#fields(
  [Model space], [Rectangle only: plane stress, plane strain or axisymmetric. It is fixed while
    analyses link the mesh, which have the same model space. An axisymmetric rectangle lies at
    X >= 0, as X is the radius.],
  [Shown in], [The unit system the lengths are shown and typed in; they are stored in SI
    units.],
  [Origin], [The corner at the smallest coordinates.],
  [Edge lengths], [The size along each axis.],
  [Elements along], [The number of elements along each axis.],
  [Element type], [Box: C3D8, C3D8R, C3D8I, C3D20 or C3D20R. Rectangle: #ui("Quadratic")
    gives 8-node instead of 4-node elements, #ui("Reduced integration") the R types; the family
    (CPS, CPE or CAX) follows from the model space. The #ui("Element") row of a rectangle
    lists its four types.],
)

The line below the fields counts the elements and names the sides. #ui("OK") keeps the
parameters, and the #ui("Mesh") row of the block shows that its mesh is out of date.
#ui("Update Project") makes it anew before it solves (@update-project), and so does
#ui("Generate Mesh") in the block's context menu; the analyses that link it take the new mesh,
and their solutions are out of date.

#screenshot("mesh-block-result.png", [The box held at X- and pressed on Z+, solved])

=== Refine Mesh <refine-mesh>

A Refine Mesh block makes the mesh linked to it again, finer or coarser by a factor, and gives
the new mesh on. Add it with #menu("Project", "Add Mesh", "Refine Mesh"), drag the
#ui("Mesh") of a mesh block or of an analysis with imported geometry to the pin on the left of
its #ui("Mesh") row, and drag the pin on the right of that row to the #ui("Mesh") row of the
analyses that are to use the refined mesh. The mesh it refines stays as it is.

#screenshot("refine-mesh.png", [A box refined by 1.5 for the analysis Fine: the Factor takes the
  dashed wire of an optimizer, which refines the mesh in its loop])

#fields(
  [Mesh], [Left pin: the mesh to refine. Right pin: the refined mesh, with its state.],
  [Factor], [How much finer the mesh is: 2 twice as fine, 0.5 half as fine, 1 as it is. Type
    or drag it in place, or drop the value of a Parameters block or of an optimizer on its pin
    (@optimization-loops).],
)

What the factor does depends on where the mesh comes from:

- From a mesh block (Box, Rectangle): the numbers of elements along every edge are multiplied
  by the factor and rounded, so a box of 10 x 2 x 2 elements refined by 1.5 has 15 x 3 x 3. The
  block makes this mesh itself: once the factor or the mesh block changes, its #ui("Mesh") row
  is out of date and #ui("Update Project") or #ui("Generate Mesh") in its context menu makes it
  again.
- From an analysis with imported geometry: it gives the geometry with its mesh setup on, every
  element size divided by the factor (the max. and min. element size of the meshing
  parameters, those of Meshing Parameters items and the local mesh sizes). The analysis that
  takes it meshes the geometry with these sizes when it is solved; the sizes as set in the mesh
  setup stay. Its mesh can therefore not go to a Mesh Export block: export the mesh of the
  analysis that meshed it.

Refine Mesh blocks can follow each other; their factors multiply. A mesh read from an input
file has no parameters or geometry to mesh it again with, and the link is refused.

=== File <file-block>

A file block reads a file from outside the project and gives what it holds to analyses: the
results of a CalculiX run (`.frd`), the mesh of a CalculiX input file (`.inp`) or geometry
(STEP, IGES, BREP, STL). Add it with #menu("Project", "Add File ...") or from the picker; a
dialog asks for the file at once. Its rows depend on the file:

- results: #ui("Results") with a row per step of the file, whose pins link like the steps of
  an analysis, for example to the #ui("Global results") row of a submodel (@submodel);
- an input file: #ui("Mesh"), to link to the #ui("Mesh") row of analyses;
- geometry: #ui("Geometry"), to link to the #ui("Geometry") row of analyses.

#screenshot("file-block.png", [The results of a CalculiX run made elsewhere drive a submodel])

The #ui("File") row holds the path: type it, or double-click the block or use
#ui("Choose File ...") in its context menu. A relative path starts at the folder of the project
file. The block reads the file in the background and reads it again whenever it changes on
disk; #ui("Read Again") in the context menu reads it at once. The symbol of the first row shows
the state: a green tick once the file is read, a dash while no file is chosen or it is missing,
and an error when it could not be read, with the reason in the tooltip and the output pane.
A linked analysis is out of date when the file changes, and #ui("Update Project") solves it
again with the new data. The file is never written to; deleting the block leaves it as it is.

#limitation[Results of a file serve as global results of submodels only. Eigenmodes of a
frequency step cannot be linked, as the `.frd` file does not hold them.]

=== Parameters <parameters-block>

A Parameters block holds named numbers that other blocks take through *value links*, so that
one number drives several places: the length of two meshes, or a size that is changed in one
spot. Add it with #menu("Project", "Add Parameters"); it starts with one length.

#screenshot("value-links.png", [The length of a Parameters block gives the edge length Lx of a
  box: the row of the box is read-only, in violet, and follows the parameter])

Each parameter is a row with its value, typed or dragged in place, and a violet pin on the
right. Drag the pin to the pin on the left of a number of a mesh block (#ui("X") to #ui("nz"))
or to the #ui("Factor") of a Refine Mesh block. The number then takes the value of the
parameter: it is shown in violet and read-only, and when the parameter changes the mesh is out
of date, so #ui("Update Project") meshes and solves again. A value link joins like with like: a length goes to a length, a count
to a number of elements, a factor to #ui("Factor"); otherwise the output pane says why it was
refused. Right-click the pin to remove the link; the number keeps its last value.

A double-click on the block opens its editor:

#fields(
  [Lengths in], [The unit system lengths are shown and typed in; they are stored in SI units.],
  [Name], [The name of the parameter, unique in the block. Renaming it keeps its links.],
  [Kind], [#ui("Length"), #ui("Count") (a whole number of at least one) or #ui("Factor") (a
    number without unit).],
  [Value], [The number.],
  [Remove], [Removes the parameter and its links.],
  [Add Parameter], [Adds a length.],
)

=== Measure <measure-block>

A Measure block computes one key figure from the results of a step: the largest von Mises
stress, the displacement of a node, a total force or an eigenfrequency. It is the number an
optimizer reads, and a quick way to see the figure that matters without opening the results.
Add it with #menu("Project", "Add Measure"); it starts with the largest von Mises stress.

#screenshot("measure-blocks.png", [Two Measure blocks: the lowest temperature of the thermal
  step Heat and the largest von Mises stress of the step Load that uses those temperatures])

Drag the pin of a step under #ui("Results") of an analysis to the #ui("Results") row of the
block, with an orange wire like every link of results. A Measure block measures one step;
another wire to it takes the place of the first. The block shows what it measures and, in its
last row, the value in its units with the state of the value:

- a green tick when it was computed from the results as they are and those are up to date;
- the orange arrows when the results are out of date, after a change of the analysis or of a
  block it links to; #ui("Update Project") solves the analysis again and the value follows;
- a dash while there are no results, or when the results lack what it measures. The output
  pane then says why, for example "step 1 has no field STRESS in the results".

The value is computed anew in the background whenever the results or its definition change,
and the output pane prints it ("Max stress: Maximum of STRESS MISES = 8.6 MPa"). The project
file keeps the value, so a reopened project shows it at once. The pin on the right of the
value row gives it to an optimizer (@optimization-loops).

#screenshot("measure-editor.png", [The editor of a Measure block])

A double-click on the block opens its editor:

#fields(
  [Measure], [What it computes: #ui("Field extreme"), #ui("Value at a node"),
    #ui("History output") or #ui("Eigenfrequency").],
  [Shown in], [The unit system of the value; it is stored in SI units.],
  [Field, Component], [The field and its component as the Results workspace names them:
    STRESS MISES, DISP ALL, NDTEMP T, TOTAL_FORCE RF3 for a history output. With results
    linked, both are lists of what those results have (a name they lack is shown in the
    warning color); without, they are typed. The value is that of the last increment of the
    step.],
  [Take], [For a field extreme: #ui("Maximum"), #ui("Minimum") or #ui("Largest magnitude"),
    which keeps the sign.],
  [Region], [For a field extreme: all nodes, or a region of the analysis whose results are
    linked: a node set of its mesh or model, or the region of a boundary condition or load,
    such as #ui("Displacement_Rotation-1 (boundary condition)"). The region is found on the mesh the results
    were solved on, so a region picked on geometry or on the sides of a mesh block follows
    every new mesh of an optimization loop.],
  [Node], [For a value at a node: its number.],
  [Set], [For a history output: the set of the history output, as written to the `.dat` file;
    a list of the sets of the linked results.],
  [Mode], [For an eigenfrequency: the mode, counted from 1, of a frequency step.],
)

=== Mesh Export <mesh-export>

A mesh export block is the opposite of a source: it takes a mesh and writes it to a file, as
a CalculiX input file (`.inp`) with the nodes, the elements, and the node sets and surfaces
of the mesh. Add it with #menu("Project", "Add Mesh Export") or from the picker under
#ui("Export"), and drag the #ui("Mesh") row of a mesh block or an analysis to its #ui("Mesh")
row. Several export blocks may take the same mesh.

#screenshot("mesh-export.png", [A box whose mesh is written to box.inp])

The block has two rows. #ui("Mesh") is the input; its symbol shows whether the file is up to
date. #ui("File") is the file to write: type its path, or double-click the block's name to
pick it in a dialog. A relative path starts at the folder of the project file, so a project
moved together with its export folder keeps working; while the project is unsaved it starts
at the folder the program was started in.

#ui("Update Project") writes the file once everything before the block is done: the mesh of
a mesh block is made first, an analysis is solved first, and the file is written from the mesh
as it is then. It writes only when something changed, that is when the mesh or the path is
new or the file is gone, so an update of a project that did not change leaves the file alone.
If an analysis before the block fails, the block does not write. #ui("Export Now") in the
block's context menu writes the file at once, whatever the state.

The file holds the mesh only, not the loads or steps of an analysis that takes the same mesh,
so it reads back as a mesh when an analysis imports it. A path the program cannot write
shows as an error in the #ui("Mesh") row's tooltip and in the output window.
