# Draft: continuing an analysis from the state after a step

Outline for the user guide chapter on the project schematic (to be written in Typst later).

## State links

- Every block has a "State" row at the top with a square green pin. Dropping the results of
  a step of another analysis (orange pin of a step row under Results) on it makes the
  analysis continue that one: it takes the geometry, the mesh, the setup and the steps up to
  and including that step, and its own steps follow.
- What it takes is read-only in its window, like other linked items (blue in the tree,
  "comes from ..."). New steps are added as usual; they are its own.
- A state link replaces the other links of the analysis; while it continues another
  analysis, no further links can go to it.
- Unlink (right-click the State pin, or Unlink on a taken item in the window) makes the
  analysis independent: it keeps its copies.

## Continue from here

- Drag the results pin of a step to empty space and pick "Continue from here": a new
  analysis "<name> continued" is created at that place, already continuing from that step.
- Screenshot: continue-from-here-menu.png, continued-analysis.png.

## Solving

- CalculiX solves only the own steps: the solve reads the state CalculiX wrote after the
  step in the other analysis' run (monitor: "Restart after step 2 of 3: ... from Bracket").
  The results contain all steps, as if solved in one go.
- When the other analysis has no results yet, Solve and Update Project solve it first.
- The sets the own steps need are written into the other analysis' input file as well,
  because CalculiX takes no new sets after reading a state. When they are missing (the
  other analysis was solved before the steps were added), the analysis is solved in full,
  which gives the same results, only slower.

## Split and merge

- Block menu > Split After > step: the steps after it go to a new analysis that continues
  this one; what took results of those steps takes them from the new analysis.
- Block menu > Merge into <name> (only for an analysis that continues from the last step
  of the other one): its own steps go back to the other analysis, and it is removed.
- Screenshot: split-after-menu.png.
