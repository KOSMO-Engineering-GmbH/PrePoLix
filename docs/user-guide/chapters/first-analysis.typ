#import "../template.typ": *

= A First Analysis

This chapter walks through a complete static analysis of a plate with a hole that is fixed at one
end and pulled at the other. Each step links to the chapter with the details.

+ *Create the analysis.* In the project window, #menu("Project", "New Analysis ...") asks for
  the model space and the unit system; keep #ui("3D") and #ui("mm, ton, s, °C")
  (@new-analysis). A block appears in the project schematic; double-click it to open the
  analysis in its own window (@project-window).
+ *Import the geometry.* #menu("Geometry", "Import ...") reads the STEP file of the plate. It
  appears as part `SOLID-1` on the #ui("Geometry") tab (@geometry-import).
+ *Mesh it.* #menu("Mesh", "Default Mesh Parameters ...") sets the element size, #ui("Mesh All
  Parts") creates quadratic tetrahedra. PrePoLix switches to the #ui("FE Model") tab
  (@meshing).
+ *Material and section.* #menu("Model", "Create Material ...") with density, Young's modulus
  210000 MPa and Poisson's ratio 0.3, or copy steel from the material library (@materials). Then
  #menu("Model", "Create Section ...") assigns it to the part (@sections).
+ *Step.* #menu("Model", "Create Step ...") with type #ui("Static") (@steps).
+ *Support.* #menu("Model", "Create Boundary Condition ..."), type #ui("Fixed"); click the end
  face of the plate in the 3D view (@boundary-conditions, @selection).
+ *Load.* #menu("Model", "Create Load ..."), type #ui("Surface Traction") with `F1` = 20000 N on
  the opposite end face (@loads).
+ *Solve.* #ui("Solve") in the toolbar or #key("F5"). The monitor shows the progress
  (@analysis). If the tree shows warning signs, click them first (@model-check).
+ *Look at the results.* Click #ui("Results") in the monitor. Select #ui("STRESS") >
  #ui("MISES") in the tree to see the von Mises stress around the hole (@results).
+ *Save the project.* #menu("File", "Save Project") writes a `.plx` file with all analyses of
  the project: geometry, mesh, model and results. The project schematic now shows the analysis
  as up to date; change a load, and its solution turns out of date until you solve again.

#screenshot("main-window.png", [The plate with support (green) and load (blue arrow) on the FE Model tab])
