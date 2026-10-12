#import "../../template.typ": *

= The User Interface

== Projects and analyses <project-window>

A PrePoLix project (`.plx`) holds several analyses, like a project of Ansys Workbench. PrePoLix
starts with the *project window*. It shows each analysis as a block in the project schematic;
each analysis opens in a window of its own (@main-window), which looks like the main window of
PrePoMax. Several analysis windows can be open at the same time, next to the project window.

#screenshot("project-window.png", [Project window with three analyses: #ui("Cantilever
  continued") continues #ui("Cantilever") after its last step (green wire), #ui("Axial-load")
  takes its mesh and materials (blue wires)])

New analyses are empty and independent of the others. To start from an existing one, use
#ui("Duplicate"): the copy takes over geometry, mesh, model and results. To share parts of an
analysis, link them in the schematic (@links): a linked item changes in one place and is used in
both.

=== Creating an analysis <new-analysis>

#menu("Project", "New Analysis ..."), the first toolbar button or a right-click on the
background of the schematic open #ui("Model Properties") for the new analysis
(@model-properties). Choose its *model space* (3D, plane stress, plane strain or axisymmetric)
and its *unit system* there.

#screenshot("new-analysis-dialog.png", [Model Properties for a new analysis])

#note[The model space and the unit system of an analysis are fixed once it is created. In the
  analysis window, #menu("Model", "Model Properties ...") shows them, but they cannot be
  changed; create a new analysis instead. Analyses of one project may use different unit systems,
  because all values are stored in SI units and only shown in the units of their analysis
  (@unit-systems).]

=== The blocks of the schematic

An icon in the top left corner of each block shows its model space: a cube for 3D, a thin plate
for plane stress, a long prism for plane strain and a profile turned about its axis for
axisymmetric models. Hover over it for the name. Below the header, each block lists the cells of
its analysis from top to bottom, and below them the steps of the analysis as they are defined
at the moment:

#fields(
  [State], [Where the analysis starts: by itself, or from the state after a step of another
    analysis whose results are dropped here (@state-links). The row then says
    #ui("State of <analysis>").],
  [Geometry], [The imported CAD geometry. A dash means the analysis has no geometry because
    its mesh was imported from an input file.],
  [Mesh], [The mesh of the parts.],
  [Setup], [The model definition; it is there as soon as the analysis has a step. The arrow in
    front of it opens its categories (#ui("Features"), #ui("Materials"), #ui("Sections"),
    #ui("Constraints"), #ui("Contacts"), #ui("Amplitudes"), #ui("Initial Conditions"),
    #ui("Steps")), and those open to their single items, so that each can be linked on its
    own.],
  [Solution], [The last CalculiX run whose results the analysis keeps.],
  [Results], [The results of that run, followed by one row per step. The row of a step gives
    the results of that step to other analyses (@results-links, @state-links).],
)

The symbol on the right of a cell shows its state; its tooltip says what it means:

#fields(
  [Green tick], [Up to date.],
  [Question mark], [Not there yet: nothing imported, meshed, set up or solved.],
  [Circular arrow], [Out of date: the cell was made from inputs that have changed since.
    #ui("Solve") makes it again.],
  [Red cross], [CalculiX reported errors; results may still be there.],
)

PrePoLix keeps a fingerprint of the inputs each cell was made from, and only cells whose inputs
changed are out of date. New meshing settings of one part make only the mesh of that part out
of date, and with it the solution. A new load makes the solution out of date but not the mesh.
Changes that CalculiX does not see, such as result paths or planes, leave the solution up to
date. Within the solution, PrePoLix also remembers which steps are unchanged, so that CalculiX
can start after them (@restart).

The pins on both sides of a row take and give links. Blue wires carry data (geometry, mesh,
setup items), orange wires carry results and green wires carry the state after a step. A round
pin on the left takes one wire, a tall one (the categories of the setup) takes any number, and
the green square takes the state of another analysis (@data-links).

=== Working with the schematic

- *Double-click* a block to open its analysis in its own window. If the window is open
  already, it comes to the front. The header of an open analysis says #ui("(open)").
  Double-clicking a row opens the window in the matching workspace: #ui("Geometry") and
  #ui("Mesh") in the Geometry workspace, #ui("Setup") and its categories in the FE Model
  workspace, and #ui("Results") in the Results workspace.
- *Drag* a block by its header to move it, drag the background to pan, and scroll to zoom.
  Double-click the background to see all blocks.
- *Drag* from a pin on the right of a row to a pin on the left of a row of another block to
  link them (@links). Right-click a pin to remove its links.
- *Right-click* a block for its context menu, or the background for #ui("New Analysis"),
  #ui("Add Materials") and #ui("Add Mesh") (@source-blocks).

#screenshot("project-node-menu.png", [Context menu of an analysis])

#fields(
  [Open], [Opens the analysis window.],
  [Solve], [Opens the analysis and solves it, like #ui("Solve") in its window (@analysis).],
  [#ui("Split After") > step], [Moves the steps after the chosen step into a new analysis that
    continues this one from the state after it (@split-merge). Only for analyses with more than
    one step.],
  [#ui("Merge into <analysis>")], [Only for an analysis that continues another one from its last step:
    gives its steps back to that analysis and removes itself (@split-merge).],
  [Rename ...], [Gives the analysis a new name. Names are unique within a project.],
  [Duplicate], [Adds a copy of the analysis, named "Copy of ...".],
  [Delete ...], [Removes the analysis from the project after asking. An open window closes.
    Links to and from it are removed; analyses that took items from it keep their copies.],
)

=== Menus and toolbar of the project window

#fields(
  [File > New Project], [Closes the project and starts a new, empty one. Add analyses with #ui("New Analysis").],
  [File > Open ... (#key("Ctrl+O"))], [Opens a project. Its analyses open when you
    double-click them.],
  [File > Save (#key("Ctrl+S"))], [Saves the project with all analyses, also those whose
    windows are open (@files).],
  [File > Save As ... (#key("Ctrl+Shift+S"))], [Saves the project under a new name.],
  [File > Save Without Results ...], [Saves a copy without the results the analyses keep.],
  [Project > New Analysis ...], [Adds an analysis after asking for its model space and unit
    system (@new-analysis).],
  [Project > Add Materials], [Adds a block of materials that analyses link to
    (@materials-block).],
  [Project > Add Mesh], [Adds a block that meshes a box or a rectangle for analyses to link
    (@mesh-block).],
  [Project > Update Project], [Solves every analysis that is not up to date (@update-project).],
  [Tools > Settings ...], [The settings (@settings); they apply to all windows.],
  [Help], [#ui("User Guide") opens this guide, #ui("Check for Updates ...") looks
    for a new version.],
)

The toolbar holds #ui("New Analysis"), #ui("Open"), #ui("Save"), #ui("Update Project") and
the button that stops the update. Stopping the update lets the running CalculiX jobs finish; use
#ui("Stop") in the analysis window to kill one.

Closing the project window closes PrePoLix with all analysis windows. Closing an analysis
window (or #menu("File", "Close Analysis") in it) keeps the analysis in the project, with all
changes; save the project to write them to the file. An analysis window that is meshing or
solving cannot be closed.

=== Update Project <update-project>

#menu("Project", "Update Project") solves every analysis whose mesh or solution is out of
date or missing, like #ui("Update Project") in Ansys Workbench. Analyses without a mesh,
geometry or step are skipped and listed in the output pane. Mesh blocks whose mesh is out of date make it anew
first (@mesh-block).

- Analyses are solved in the order of their links: an analysis that takes results or a state
  from another one waits until that one is solved. An analysis upstream is solved only when it
  is not up to date itself; when it is solved again, the analyses that use its results are
  solved after it.
- Independent analyses are solved at the same time, up to #ui("Parallel analyses") of the
  CalculiX settings (default 2), each with the number of threads set there (@settings). Two
  analyses on one computer are worth it when CalculiX spends its time in the equation solver on
  few threads; set #ui("Threads") times #ui("Parallel analyses") to at most the number of
  processor cores.
- Windows that the update opens close again when their analysis is solved. If CalculiX fails,
  the window stays open with the monitor; only the analyses that depend on the failed one are
  skipped, independent ones go on. The output pane sums up with #ui("Project updated") or
  #ui("Update finished with errors").
