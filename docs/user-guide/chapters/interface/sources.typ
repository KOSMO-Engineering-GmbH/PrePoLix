#import "../../template.typ": *

== Source blocks <source-blocks>

Besides analyses, the project schematic holds *source blocks*: blocks that only give data to
analyses through links. They have no steps, are not solved, and #ui("Update Project") passes
them by. Add one from #menu("Project") or by right-clicking the background of the schematic;
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

#screenshot("mesh-block.png", [A box linked to an analysis; its mesh is out of date after its
  division changed])

Its sides are node sets and surfaces at once, named after the axis and the side they lie on:
#ui("X-") at the smallest X, #ui("X+") at the largest, then #ui("Y-"), #ui("Y+") and, for a
box, #ui("Z-") and #ui("Z+"). Boundary conditions and loads pick them under #ui("Node set") or
#ui("Surface") without selecting anything. The elements form the part #ui("BOX") or
#ui("RECTANGLE"), which a section takes.

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
    (CPS, CPE or CAX) follows from the model space.],
)

The line below the fields counts the elements and names the sides. #ui("OK") keeps the
parameters, and the #ui("Mesh") row of the block shows that its mesh is out of date.
#ui("Update Project") makes it anew before it solves (@update-project), and so does
#ui("Generate Mesh") in the block's context menu; the analyses that link it take the new mesh,
and their solutions are out of date.

#screenshot("mesh-block-result.png", [The box held at X- and pressed on Z+, solved])
