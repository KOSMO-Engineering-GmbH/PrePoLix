#import "../../template.typ": *

== Bolt pre-tension <pre-tension>

A #ui("Pre-tension (bolt preload)") load tightens a bolt: it pulls the two sides of a cut through
the shank together with a force, or shortens the bolt by a given length, as PrePoMax's
pre-tension load does. Create it with #ui("Create ...") on #ui("Loads") of a step or
#menu("Model", "Create Load ...").

#screenshot("pre-tension-load.png", [Pre-tension load on the cut through a bolt shank])

#fields(
  [Region], [The element faces on *one* side of a cut through the bolt shank, picked in the 3D
    view or as a surface. The cut must lie inside the bolt mesh: split the shank with a plane
    in the CAD geometry or pick the faces of one element layer.],
  [Preload by], [#ui("Force") (default) or #ui("Displacement").],
  [Force], [The force pulling the two sides together; a positive value tightens the bolt.],
  [Shortening], [The length the bolt is shortened by, when preloading by displacement.],
  [Direction], [The direction of the bolt axis. Unticked, PrePoLix uses the normal of the
    selected faces; #ui("Given") takes the components #ui("X"), #ui("Y"), #ui("Z").],
)

PrePoLix cuts the mesh apart at the selected faces when it writes the input file: every
element on their side, also one that only touches the cut with an edge or a corner, gets copies
of the nodes on the cut. `*Equation`s tie the copies to the other side the way CalculiX's
`*PRE-TENSION SECTION` does: the gap normal to the cut is the first degree of freedom of a new
node per load, the sides cannot slide against each other. The preload is a `*CLOAD` with the
force or a `*BOUNDARY` with the shortening on this node. The same load name in a later step
reuses the node, so a bolt tightened by a force in the first step can be held at its length in
the next step with a displacement of 0, like the usual "preload, then load" sequence of a bolted
joint.

#note[PrePoMax writes `*PRE-TENSION SECTION`, and CalculiX separates only the elements with a
face on the cut. In a tetrahedral mesh, elements that touch the cut with just an edge or a
corner then hold both sides together and take most of the preload, which is why PrePoMax users
add a thin layer of wedge elements at the cut (#menu("Model", "Tools", "Boundary Layer")).
PrePoLix needs no such layer: its own cut separates all of them.]

A pre-tension load can follow an #ui("Amplitude") (@amplitudes). The reaction force at the
pre-tension node is the bolt force; request it with a history output (@history-outputs) of `RF`
on the load.

#note[The preload acts across the cut only. Hold the bolt against rigid body motion with the
contact or tie to the clamped parts, not with a support on the cut faces.]
