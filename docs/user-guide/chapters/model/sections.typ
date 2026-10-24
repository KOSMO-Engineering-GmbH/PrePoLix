#import "../../template.typ": *

== Sections <sections>

A section assigns a material to elements. Every element needs a section, otherwise the model check
reports "Elements without material". Create sections with #menu("Model", "Create Section ...").

#screenshot("section.png", [Section dialog])

#fields(
  [Type], [#ui("Solid"), #ui("Truss") or #ui("Beam").],
  [Material], [One of the model's materials.],
  [Region], [#ui("Parts") (picked in the 3D view) or an #ui("Element Set") (@mesh-sets).],
)

=== Solid section

For solid elements and the plane and axisymmetric elements of 2D models. In plane stress and plane
strain models the section has a #ui("Thickness") (default 1). Written as `*SOLID SECTION`.

=== Truss section

For line parts that carry only axial force. #ui("Cross-section area") must be positive. The lines
are written as `T3D2` truss elements.

#note[Trusses must form a stable truss structure. If they can move without resistance, the model
check reports "Truss structure is a mechanism".]

=== Beam section

#fields(
  [Profile], [#ui("Rectangle") (thicknesses a and b in the 1- and 2-direction), #ui("Circle")
    (radius), #ui("Pipe") (outer radius and wall thickness) or #ui("Box") (widths a and b and the
    four wall thicknesses).],
  [Normal (1-direction)], [#ui("Automatic") uses the global z axis, or x for beams parallel to z.
    #ui("Vector") takes the direction from x, y and z; it must not be parallel to a beam.],
  [Offset in 1-/2-direction], [Shifts the beam axis by a multiple of the section dimension.],
)

Beams are written as `*BEAM SECTION` with `B31` or `B32` elements. Pipe and box sections need
quadratic lines and are written as `B32R`, as CalculiX requires.

==== Beam profiles in the FE model and the results <beam-profiles>

The FE model draws beams as their profile swept along the line, as CalculiX expands them: the
1-direction is the #ui("Normal") turned perpendicular to the beam, the 2-direction the beam axis
crossed with it, and the offsets move the profile off the nodes (0.5 puts the nodes on the
positive face). Pipes and boxes show their hollow walls. Where two elements of a line meet
the profile runs on and its outline marks the element border; at the ends of a line, at kinks
of more than 45 degrees and at branches each element ends with a face. Trusses are drawn as round
bars of their cross-section area.

#screenshot("beam-profiles.png", [A frame with a rectangular beam (left), a pipe frame (right),
truss bracing and a roof of shells, drawn with their sections.])

In the Results workspace CalculiX usually hands the beams back as the solids it expanded them
to. A model with shells makes it report them as lines, though (`OUTPUT=2D`), and trusses are
always lines; PrePoLix then sweeps the profiles of the input file CalculiX ran along the
deformed lines, coloured by the shown result. The profile is moved with its nodes, not turned
by their rotations.

#screenshot("beam-profiles-results.png", [The same frame solved: shells, beams and trusses
drawn with their sections in the results.])

Turn the profiles off with #menu("View", "Beam Profiles") or in the #link(<settings>)[settings]
under #ui("Graphics") to see the lines only. Picking still selects the lines.

#limitation[Line elements can only be used in 3D models.]
