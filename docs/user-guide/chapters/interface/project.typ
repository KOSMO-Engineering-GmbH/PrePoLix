#import "../../template.typ": *

= The User Interface

== Projects and analyses <project-window>

A PrePoLix project (`.plx`) holds several analyses, like a project of Ansys Workbench. PrePoLix
starts with the *project window*. It shows each analysis as a block in the project schematic;
each analysis opens in a window of its own (@main-window), which looks like the main window of
PrePoMax. Several analysis windows can be open at the same time, next to the project window.

#screenshot("project-window.png", [Project window with three analyses: two are solved, the
  third has no step yet])

New analyses are empty and independent of the others. To start from an existing one, use
#ui("Duplicate"): the copy takes over geometry, mesh, model and results.

The model space and the unit system belong to the project: all analyses use the same ones.
#menu("Project", "Model Space and Units ...") changes them for all analyses; the same happens
when you change them in #ui("Model Properties") of an analysis (@model-properties).

=== The blocks of the schematic

Each block lists the cells of its analysis from top to bottom, and below them the steps of the
analysis as they are defined at the moment:

#fields(
  [Geometry], [The imported CAD geometry. A dash means the analysis has no geometry because
    its mesh was imported from an input file.],
  [Mesh], [The mesh of the parts.],
  [Setup], [The model definition; it is there as soon as the analysis has a step.],
  [Solution], [The last CalculiX run whose results the analysis keeps.],
  [Results], [The results of that run.],
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
date.

=== Working with the schematic

- *Double-click* a block to open its analysis in its own window. If the window is open
  already, it comes to the front. The header of an open analysis says #ui("(open)").
- *Drag* a block by its header to move it, drag the background to pan, and scroll to zoom.
  Double-click the background to see all blocks.
- *Right-click* a block for its context menu, or the background for #ui("New Analysis").

#screenshot("project-node-menu.png", [Context menu of an analysis])

#fields(
  [Open], [Opens the analysis window.],
  [Solve], [Opens the analysis and solves it, like #ui("Solve") in its window (@analysis).],
  [Rename ...], [Gives the analysis a new name. Names are unique within a project.],
  [Duplicate], [Adds a copy of the analysis, named "Copy of ...".],
  [Delete ...], [Removes the analysis from the project after asking. An open window closes.],
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
  [Project > New Analysis], [Adds an empty analysis.],
  [Project > Update Project], [Solves every analysis that is not up to date, one after the
    other. Analyses without a mesh, geometry or step are skipped and listed in the output pane.
    Windows that the update opens close again when their analysis is solved; if CalculiX
    fails, the update stops and the window stays open with the monitor.],
  [Project > Model Space and Units ...], [Model space and unit system of all analyses.],
  [Tools > Settings ...], [The settings (@settings); they apply to all windows.],
  [Help], [#ui("User Guide") opens this guide, #ui("Check for Updates ...") looks
    for a new version.],
)

The toolbar holds #ui("New Analysis"), #ui("Open"), #ui("Save"), #ui("Update Project") and
the button that stops the update. Stopping the update lets a running CalculiX job finish; use
#ui("Stop") in the analysis window to kill it.

Closing the project window closes PrePoLix with all analysis windows. Closing an analysis
window (or #menu("File", "Close Analysis") in it) keeps the analysis in the project, with all
changes; save the project to write them to the file. An analysis window that is meshing or
solving cannot be closed.
