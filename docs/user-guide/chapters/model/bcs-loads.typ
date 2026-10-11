#import "../../template.typ": *

== Boundary conditions <boundary-conditions>

Boundary conditions belong to a step. Create them with #ui("Create ...") on #ui("BCs") of a step,
or with #menu("Model", "Create Boundary Condition ...") for the last step. The dialog lists only
the types the step accepts. In the 3D view, supports are drawn as cones (translations) and
plates (rotations).

#screenshot("boundary-condition.png", [Fixed support on a face])

#fields(
  [Fixed], [All degrees of freedom held at zero. In 2D only the two translations are fixed.],
  [Displacement/Rotation], [Prescribed values for the ticked components `U1`, `U2`, `U3`
    (length) and `UR1`, `UR2`, `UR3` (angle in radians). Unticked components are free.],
  [Temperature], [Prescribed temperature (heat transfer, coupled and uncoupled steps).],
  [Submodel], [Displacements taken from the global model (static steps of a submodel,
    @submodel). #ui("Global step") selects the step of the global results, the check boxes the
    components.],
  [Symmetry], [Holds the selected faces in their plane like a mirror image (@symmetry).],
)

The region is a selection of faces, edges or nodes, a node set or a surface. Displacement and
temperature boundary conditions can follow an #ui("Amplitude") (@amplitudes).

=== Motion preview <motion-preview>

When a displacement boundary condition follows an amplitude, a play button appears right of the
#ui("Amplitude") field. It previews the prescribed motion in the 3D view: the nodes of the region
move by the prescribed displacements times the amplitude, passing through the amplitude's time
range in three seconds and starting again after a short rest. The button turns into a stop
button; the preview also ends when the dialog closes or the amplitude is removed. Changes to the
values, the amplitude or the region show while it plays.

#screenshot("motion-preview.png", [Tip of a cantilever moved up and down by an amplitude])

Only the nodes of the region move; the rest of the mesh stays in place, so the elements next to
the region stretch. The motion is exaggerated so that the largest displacement is a quarter of
the model size; the dialog shows the current time of the amplitude and this scale. Rotations
(`UR1` to `UR3`) are not shown.

== Loads <loads>

Loads are created like boundary conditions, with #ui("Create ...") on #ui("Loads") of a step or
#menu("Model", "Create Load ..."). A frequency step takes no loads.

#screenshot("selection.png", [Surface traction on the end face of a plate])

#fields(
  [Concentrated Force], [`F1`, `F2`, `F3` at *every* node of the region (`*CLOAD`). The force is
    not divided between the nodes; use #ui("Surface Traction") for a total force on a face.],
  [Pressure], [Pressure on faces; positive values push into the material (`*DLOAD`).],
  [Surface Traction], [A *total* force on faces, or along edges with #ui("Apply to")
    #ui("Edges") (@traction-edges). When the input file is written it is distributed to the
    nodes according to the face areas or edge lengths and the element shape functions
    (`*CLOAD`).],
  [Moving Load], [Wheel forces that travel over faces, one static step per position
    (@moving-load).],
  [Concentrated Flux], [Heat flow into every node of the region (`*CFLUX`).],
  [Surface Flux], [Heat flux density into faces (`*DFLUX`).],
  [Body Flux], [Heat generated per volume in parts or elements (`*DFLUX`, `BF`).],
  [Film (Convection)], [Convection to the surroundings with #ui("Sink temperature") and
    #ui("Film coefficient") (`*FILM`).],
  [Radiation], [Radiation to the surroundings with #ui("Sink temperature") and
    #ui("Emissivity") (`*RADIATE`). Needs the physical constants of the model
    (@model-properties).],
)

Every load can follow an #ui("Amplitude"). Film and radiation have a second amplitude for the film
coefficient or the emissivity.

In 2D models face loads act on element edges. In axisymmetric models forces act on the full
circumference.

#limitation[Line loads (force per length) are not available in the dialogs; a
#ui("Surface Traction") along edges takes the total force (@traction-edges). Bolt pre-tension:
@pre-tension. Defined temperature fields: @defined-fields. They can be added with the keyword
editor (@keyword-editor).]

=== Surface traction along edges <traction-edges>

In 3D models a #ui("Surface Traction") can act along edges instead of faces, for example on
the free edge of a shell or a membrane, or along an edge of a solid. Choose #ui("Apply to")
#ui("Edges") in the load dialog; the #ui("Selection") window then picks edges:

#fields(
  [Edges], [A click picks the edge nearest to it, from corner to corner. On a mesh created from
    the geometry it picks the edge of the geometry, which is kept when the part is remeshed.],
  [Edge angle], [The chain of edges through the click, continued while it bends by less than
    the angle.],
)

#key("Shift") adds edges, #key("Ctrl") removes them, and a box selects the edges inside it. The
field below the region shows the number of element edges. When the input file is written the
total force is distributed to the nodes of the edges by their length, as the consistent nodal
forces of a constant line load: on quadratic elements an edge's end nodes take 1/6 and its
midside node 4/6 of the edge's share.

#screenshot("surface-traction-edge.png", [Surface traction of 100 N along the free edge of a
  shell plate])

== Hiding symbols <hide-symbols>

#ui("Hide") in the context menu of a boundary condition or load removes its symbol from the 3D
view, #ui("Show") brings it back; the tree marks a hidden item with an eye. #ui("Hide All") and
#ui("Show All") in the context menu of #ui("BCs") or #ui("Loads") of a step do this for all of
them. Constraints are hidden the same way (@constraints). As in PrePoMax, a hidden item is still
drawn while it is selected, and the setting is saved with the project. Hiding changes only the
view; #ui("Deactivate") leaves an item out of the input file.

#screenshot("hide-loads.png", [Hidden pressure load and the context menu of Loads])
