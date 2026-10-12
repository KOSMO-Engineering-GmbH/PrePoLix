# Draft: solving again from the last unchanged step, parallel analyses

Outline for the user guide chapter on solving (to be written in Typst later).

## Restart from the last unchanged step

- CalculiX writes the state after each step over time (static, dynamic, heat transfer,
  coupled and uncoupled temperature-displacement, visco) into a restart file.
- When only later steps of an analysis change, Solve starts after the last unchanged step:
  CalculiX reads the state from the earlier run and solves the rest. The monitor says
  "Restart after step 2 of 3: ..." or "Full run: <reason>".
- The results look as if all steps had been solved: the results and history output of the
  unchanged steps are joined with the new ones, and the input file kept with the results is
  the full one, which runs on its own.
- A full run happens when the model definition changed (material, section, mesh, a new set
  for a load), when the first step changed, when the steps before the change include a
  perturbation step (frequency, buckle), when another CalculiX is set, or when the restart
  files are gone. A restart CalculiX cannot finish is solved once more in full.
- Settings > CalculiX: "Keep the state after each step" (on by default) and "Restart files
  per project" (4096 MB): beyond it the oldest restart files are removed; that only costs
  time, never correctness.
- Screenshot: settings page (settings-restart-parallel.png).

## Parallel analyses

- Update Project solves up to "Parallel analyses" (Settings > CalculiX, default 2)
  independent analyses at the same time, each with the threads set above.
- An analysis that takes data (links) from another waits until that one is solved; when an
  analysis fails, only the analyses depending on it are skipped.
