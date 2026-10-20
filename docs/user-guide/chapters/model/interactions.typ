#import "../../template.typ": *

== Constraints <constraints>

Parts meshed separately share no nodes. Constraints connect them or attach springs to the model.
Create them with #menu("Interaction", "Create Constraint ...") or with #ui("Create ...") on
#ui("Constraints").

#screenshot("constraint.png", [Constraint dialog])

#screenshot("constraint-spring-2d.png", [Point spring in a plane stress model: only #ui("K1") and #ui("K2")]) <spring-2d>

#fields(
  [Point Spring], [Springs from every node of the region to ground. #ui("K1"), #ui("K2"),
    #ui("K3") are the stiffnesses in the global directions; 0 means no spring in that direction.
    In a 2D model (plane stress, plane strain, axisymmetric) there is no #ui("K3"): the dialog
    offers the first two directions only (@spring-2d). Written as `SPRING1` elements.],
  [Surface Spring], [Springs on a face region. The stiffness is given #ui("Total") or
    #ui("Per area") and distributed over the nodes by area.],
  [Compression Only], [A support that only takes pressure, like a part resting on a rigid floor.
    By default it pushes onto the surface from outside; #ui("Flip") under #ui("Direction")
    makes it push the surface outward from inside the material. #ui("Clearance") is the
    initial gap. #ui("Spring stiffness") and #ui("Tensile force") are optional and belong to
    the whole surface, spread over its nodes by area; with
    #ui("Apply to all nodes individually") every node gets the whole value. Without a value
    every gap gets CalculiX's default, practically rigid. With #ui("Nonlinear") the contact
    is solved nonlinearly, otherwise it is linearised in a linear step. Written as `GAPUNI`
    elements to fixed ground nodes, one per node.],
  [Surface To Surface Spring], [Springs between the nodes of a slave surface and the closest
    points of a master surface, with total or per-area stiffness.],
)

Surface-to-surface springs have a #ui("Master") and a #ui("Slave") region, each with its own
#ui("...") button. Choose the finer mesh as slave. #ui("Swap Master/Slave") in the context
menu exchanges the two.

The 3D view of the FE model shows active constraints in yellow, together with
the symbols of any step: a coil spring for each direction with stiffness, at each node of a
point spring (at their centre for ten nodes or more), at the centre of a surface spring and at
points spread over the slave surface of a surface-to-surface spring; cones with a ground plate on
the side a compression only support pushes from (@compression-only); and lines to the reference point of a rigid body
(@rigid-body). The selected or edited constraint is red. Ties have no symbol.

#screenshot("constraint-symbols.png", [Point spring, surface spring, compression only support
  and rigid body on a cantilever])

#ui("Hide") in the context menu of a constraint removes its symbol from the 3D view, #ui("Show")
brings it back; the tree marks a hidden constraint with an eye. #ui("Hide All") and
#ui("Show All") in the context menu of #ui("Constraints") do this for all constraints at once.
A hidden constraint is still drawn while it is selected, and the setting is saved
with the project. Hiding changes only the view, not the analysis; #ui("Deactivate") leaves a
constraint out of the input file.

#screenshot("compression-only-dialog.png", [Compression only support with flipped direction])
  <compression-only>

#screenshot("compression-only.png", [Front view of a cantilever: a compression only support
  pushing onto the end face from outside (right) and a flipped one pushing the top face
  outward from inside the material])

#screenshot("constraint-hide.png", [Hidden point spring and the context menu of Constraints])

#limitation[User-defined equations are not available yet.]

== Contacts <contacts>

=== Surface interactions

#menu("Interaction", "Create Surface Interaction ...") defines the contact behaviour that contact
pairs refer to. Move the models you need from #ui("Available") to #ui("Selected").

#screenshot("surface-interaction.png", [Surface interaction with surface behaviour])

#fields(
  [Surface Behavior], [Pressure-overclosure relation, the contact pressure over the overclosure
    (penetration) of the surfaces. #ui("Hard") is the usual rigid contact. The others are soft
    contacts: #ui("Linear") (stiffness K, about 5 to 50 times Young's modulus per length, and
    tension at large clearance σ#sub[∞]), #ui("Exponential") (pressure p#sub[0] at zero gap,
    dropping to 1 % at gap c#sub[0]; helps convergence from open contact), #ui("Tabular") (your
    own curve) or #ui("Tied") (the surfaces stay connected with stiffness K, see below).],
  [Friction], [Coulomb friction: the tangential force is limited to #ui("Friction coefficient")
    times the contact pressure. The optional #ui("Stick slope") is the stiffness while sticking;
    if empty, PrePoLix writes CalculiX's default, half the Young's modulus of the first
    material. Friction makes the analysis more nonlinear, so start with
    small coefficients and enough increments.],
  [Gap Conductance], [Heat transfer across the contact, constant or as a table over pressure
    and temperature, for thermal and coupled steps. Without it the contact is thermally
    insulating.],
)

A #ui("Tied") surface behavior glues the surfaces together, like a Tie, but as a penalty contact.
CalculiX needs #ui("Friction") with it in the same surface interaction: the #ui("Stick slope")
is the stiffness of the tie along the surfaces (half the Young's modulus of the first material if
empty), the friction coefficient is irrelevant. Without friction the run
aborts. So choosing #ui("Tied") adds #ui("Friction") to the selected models at once, and the
dialog does not accept a tied interaction without it. Tied contact works only with the method
#ui("Surface to surface"). The model check reports contact pairs that use it otherwise.

#screenshot("surface-interaction-tied.png", [Choosing Tied adds Friction])

=== Contact pairs

#menu("Interaction", "Create Contact Pair ...") creates a contact between a master and a slave
surface.

#screenshot("contact-pair.png", [Create Contact Pair; the grey text below #ui("Method") explains
  the chosen method])

#fields(
  [Surface Interaction], [The contact behaviour (see above).],
  [Method], [How CalculiX enforces the contact, see the table below. All contact pairs of a
    model must use the same method.],
  [Small sliding], [Slave nodes are paired with master faces only at the start of each
    increment and the pairing is kept during the iterations. This is faster and more stable when
    the surfaces slide little compared with the element size. Only for #ui("Node to surface").],
  [Adjust], [Moves slave nodes onto the master surface before the analysis, so that small gaps
    or overlaps from meshing do not start the contact with a jump. With #ui("Adjust distance")
    all slave nodes within that distance are moved; without it only nodes that penetrate the
    master surface. Only the start geometry changes, not the real geometry.],
  [Master / Slave], [The two surfaces, picked with their #ui("...") buttons. Choose the finer
    mesh as slave.],
)

In 3D models the master and slave regions can also be edges of shells and membranes, where a
shell meets a solid face edge on: a click near a free edge of a shell picks the edge, and so do
#ui("Edge angle") and, under #ui("More"), #ui("Edge"). CalculiX takes such an edge as a face of
the shell (S3 and up). For a contact on a shell edge use #ui("Surface to surface") with a
#ui("Tied") surface behavior (with its #ui("Friction"), see above).

==== Which method?

#fields(
  [Node to surface], [Each slave node is kept outside the master surface. Works with nodes or
    faces as slave and in all step types, including dynamics. Master nodes may poke through the
    slave surface and the pressure is noisy, so the finer mesh must be the slave. Best with linear
    elements and for explicit dynamics.],
  [Surface to surface], [Face-to-face penalty contact (default). The contact condition is
    integrated over the slave faces, so the slave must consist of element faces. Smoother
    pressures and better convergence than node to surface, especially with quadratic elements.
    Slower per iteration. A good choice for static contact.],
  [Mortar], [Face-to-face mortar contact. The most accurate contact pressure and little
    dependence on which side is master, which matters when you read contact pressures. Static
    steps only, element-face surfaces required, and the slowest method. Try it if surface to
    surface gives oscillating pressures; it is not meant for dynamics.],
  [Massless], [Massless node-to-face contact for explicit dynamics. Use it only in explicit
    dynamic steps.],
)

#note[The method is a CalculiX feature. See the sections on contact in the CalculiX manual
  (node-to-face, face-to-face penalty, face-to-face mortar and massless contact) for the exact
  formulations and limits.]

Hover over a method in the list to see a short description. The Search Contact Pairs window shows
the same texts.

=== Ties <ties>

#menu("Interaction", "Create Tie ...") or #ui("Create Tie ...") in the context menu of
#ui("Contact Pairs") glues a slave surface to a master surface (`*TIE`). Ties are listed under
#ui("Contact Pairs"), with the contact pairs and node ties; projects that kept them under
#ui("Constraints") open with them moved there.

#fields(
  [Position tolerance], [The largest distance at which slave nodes are tied.],
  [Adjust], [#ui("Move slave nodes onto master (Adjust)") projects the slave nodes onto the
    master surface.],
  [Master / Slave], [The two surfaces, picked with their #ui("...") buttons, also edges of
    shells as for contact pairs. Choose the finer mesh as slave, and an edge of a shell as slave
    of the face it stands on.],
  [Cyclic symmetry], [Ties the two cut faces of a sector of a rotationally repeating part, see
    @cyclic-symmetry.],
)

=== Node ties

#ui("Create Node Tie ...") in the context menu of #ui("Contact Pairs") makes all nodes of a region
move together. It connects the ends of beams and trusses of different parts. With
#ui("Rotations: rigid") the connection also transmits moments; without it, beams are connected
by a hinge.

=== Searching contact pairs

#menu("Interaction", "Search Contact Pairs ...") finds surfaces of different parts that touch and
creates ties or contact pairs for them.

#screenshot("contact-search.png", [Search Contact Pairs])

#fields(
  [Distance], [Surfaces closer than this count as touching (default 0.01 mm). For a tie it also
    becomes the position tolerance, for a contact the adjust distance.],
  [Angle], [Largest angle between the surface normals (default 35°).],
  [Group by], [#ui("None"): one pair per touching surface pair. #ui("Parts") (default): one pair
    per pair of parts. #ui("Graph"): merges surfaces so that each node is slave only once.],
  [Geometry Filter], [Which geometry is searched: in 3D the faces of #ui("Solid") parts, the top
    and bottom of #ui("Shell") and membrane parts, and their free edges (#ui("Shell edge")); in
    2D the element edges. #ui("Line end") finds beam and truss ends at the same point;
    #ui("Ignore hidden parts") leaves out hidden parts.],
  [Type], [#ui("Tie") or #ui("Contact"), for contacts with #ui("Surface Interaction") and
    #ui("Method").],
  [Adjust mesh], [Writes the pairs with adjust.],
)

#ui("Search") fills the table; its #ui("Geometry") column tells what touches, e.g.
#ui("Solid-Shell edge") where the edge of a shell or membrane stands on a solid. Each row can be edited and switched off with its check box; select
several rows to edit them together. The context menu of a row has #ui("Swap Master/Slave") and
#ui("Merge by Master/Slave"). PrePoLix chooses the coarser mesh as master, then the stiffer
material, then the larger surface; an edge of a shell is always the slave of a face. A shell
touches with the side facing the other part: CalculiX ties its top (S2) or bottom (S1). #ui("OK") creates the checked pairs, all under
#ui("Contact Pairs"); line ends become node ties. Surfaces that are whole CAD faces or edges are stored by geometry and
survive remeshing.

#screenshot("contact-search-shell-edge.png", [A tie found by the search between the bottom of a
  block (master, red) and the top edge of a shell standing under it (slave, violet)])

CalculiX ties the edge faces of membranes to nothing (the solver stops with a singular stiffness
matrix), so PrePoLix writes the slave side of a tie on membrane edges as a node surface of the
edge's nodes; a membrane has no rotations that would need more.
