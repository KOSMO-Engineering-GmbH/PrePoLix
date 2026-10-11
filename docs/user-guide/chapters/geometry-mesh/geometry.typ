#import "../../template.typ": *

= Geometry and Mesh <geometry>

== Importing geometry <geometry-import>

#menu("Geometry", "Import ...") or the toolbar button #ui("Import Geometry") reads STEP
(`.step`, `.stp`), IGES (`.iges`, `.igs`) and BREP (`.brep`) files. Several files can be
selected at once. Gmsh with OpenCASCADE reads the files, so Gmsh must be available
(@settings).

- If no model is open, #ui("Model Properties") opens first, so you can choose the model space
  (3D or 2D) and the unit system before the geometry is read (@model-properties).
- If the model already has geometry or a mesh, the imported parts are *added*. Existing parts keep
  their names, mesh settings and selections.
- STEP and IGES files are converted to the length unit of the model. BREP files have no unit and
  are read as they are.
- Default element sizes are derived from the size of the geometry: maximum 5 % and minimum 0.1 %
  of the bounding box diagonal.

#screenshot("geometry.png", [Imported geometry on the Geometry tab])

=== Importing a mesh <mesh-import>

The same dialog also takes input files (`.inp`). Their mesh is added to the model as *mesh
parts*, for example a mesh written by #menu("File", "Export Deformed Mesh (.inp) ...")
(@export-deformed) to use a buckling mode as imperfection. Select the files in the file dialog
(the filter #ui("Input file") shows only those) and the parts appear on the #ui("FE Model") tab.

- A model is never replaced. If it already has parts, the imported nodes and elements are
  numbered after the highest numbers it has, so nothing that refers to the existing parts changes.
- A part, set or surface whose name is already taken gets a number appended: the second `PART-1`
  becomes `PART-1-2`.
- Only the mesh is read: nodes, elements, sets and surfaces. Materials, steps and loads of the
  file are skipped and listed in the output window. To open an input file as a model of its own,
  use #menu("File", "Open ...") instead.
- Into an empty model the mesh decides the model space (a mesh of plane stress elements makes a 2D
  model). A 2D mesh cannot be added to a 3D model or the other way round.

#screenshot("mesh-import.png", [A deformed mesh imported next to the original part])

=== Parts

Every solid becomes a part. Faces that do not belong to a solid become shell parts, edges that do
not belong to a face become line parts (for beams and trusses). Part names are taken from the
CAD file where it has names, otherwise they are `SOLID-1`, `SHELL-1`, `LINE-1` and so on.

The context menu of a geometry part has #ui("Create Mesh"), #ui("Properties ..."),
#ui("Hide")/#ui("Show") and #ui("Delete"). Deleting a geometry part keeps a mesh that was already
created from it.

=== Split faces

It is common to split faces in the CAD program, for example a ring on a shaft where a bearing
sits, so that a boundary condition can be applied to exactly that area. PrePoLix keeps such
faces: the mesh follows their borders, and on the #ui("FE Model") tab their outline is drawn
like any other edge although the surface does not bend there. Pick them with the geometry based
selection (@selection) like any other face.

#screenshot("split-face.png", [A ring split off the shaft in the CAD program, outlined and picked on the mesh])

=== Geometry for 2D models

In a 2D model the geometry must consist of faces only and lie in the plane z = 0. For an
axisymmetric model it must lie at x ≥ 0, because the y axis is the axis of rotation. Otherwise
the import is refused with a message. Faces whose normal points in −z direction are flipped
when they are meshed.
