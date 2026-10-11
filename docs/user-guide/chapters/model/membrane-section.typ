#import "../../template.typ": *

=== Membrane section <membrane-section>

A #ui("Membrane") section is the shell section's sibling for structures without bending
stiffness: foils, fabrics, balloons, pretensioned roofs or a thin skin on a solid. A membrane
carries forces in its plane only, like a tensioned cloth. Its nodes have no rotations, and a
flat membrane can take a load across its plane only once it is pretensioned or deforms, which
needs a geometrically nonlinear step (#ui("Nlgeom")). For a plate under a lateral load the
shell section (@shell-section) stays the right choice.

Create it with #menu("Model", "Create Section ...") and choose #ui("Membrane"). The elements of
the region must be membranes: double-click the part on the #ui("FE Model") tab and choose `M3D3`
for triangles or `M3D4`, `M3D4R` for quadrilaterals as its element type (@element-type).

#screenshot("membrane-element-type.png", [Choosing a membrane element type for a part])

#screenshot("membrane-section.png", [Membrane section])

#fields(
  [Thickness], [The thickness of the membrane (default 1); it must be greater than 0.],
  [Offset], [Position of the mesh within the thickness as a fraction of the thickness, as for
    shells. Written as `OFFSET=`.],
  [Material, Region], [As for the other sections.],
)

Written as `*MEMBRANE SECTION, ELSET=..., MATERIAL=...` with the thickness on the next line.
The field outputs are written with `OUTPUT=2D`, as for shells.

The model check reports a membrane section on shell or solid elements, and a shell or solid
section on membranes, as an invalid section. Membranes have to be linear: CalculiX 2.23
computes the quadratic membranes `M3D6` and `M3D8` wrongly (in a simple tension test `M3D8` is
1.7 times too soft and gets softer with a thicker section), so mesh the part with
#ui("Second order") off. A membrane section on quadratic elements is reported as an invalid
section.

A membrane strip of 100 × 10 × 2 stretched by 0.1 in a first step and loaded with a pressure
in a second one sags at mid-span as a taut string does, $p L^2 / (8 N)$ with the tension $N$
per width; PrePoLix's tests check this with CalculiX.
