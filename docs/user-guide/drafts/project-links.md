# Outline: project schematic, links and units (stage 2)

Draft structure for the user guide. Text and screenshots still to be written in Typst.

## 1. New analysis (chapters/interface/project.typ)

- Project > New Analysis ..., the toolbar button and the schematic's context menu open Model Properties first.
- Model space (3D, plane stress, plane strain, axisymmetric), model type and unit system are chosen there.
- After that they are fixed. In the analysis window, Model > Properties shows them read-only.
- Screenshot: the Model Properties dialog for a new analysis.

## 2. Blocks in the schematic

- An icon in the header's top-left corner shows the model space (cube, plate, long prism, axis with profile). Hover text: "Model space: ...".
- Rows: Geometry, Mesh, Setup (expandable to its categories and single items), Solution, Results, Steps.
- State column: up to date, out of date, not yet computed.
- Screenshot: three blocks with all four model space icons.

## 3. Linking analyses

- Drag from an output pin to an input pin. This works for a whole category or for a single item (one material, one step ...).
- Linked items are read-only in the target. In its tree they show a link icon and "from X", and Unlink makes them editable copies.
- Changes in the source reach the target live.
- Refusals are written to the output, for example:
  - a different model space ("linked analyses need the same model space");
  - a link that would go back and form a cycle;
  - an item that does not fit the slot.
- A different unit system is allowed: values are stored in SI and each analysis shows them in its own units.
- Right-click a pin to remove its links.
- Screenshots: a link wire, a linked material in the target tree, and the refusal message.

## 4. Submodel chain

- Steps are connection points. Link the results of a step to a submodel analysis to drive its boundary.
- Update Project solves in order. An upstream analysis is solved only if it has no result yet.
- Only parts whose inputs changed are recomputed (fingerprints per mesh part, setup and solution).
- Screenshot: a global model linked to a submodel.

## 5. Units and storage (chapters/model/model-properties.typ)

- Everything is stored in SI (m, kg, s, N, Pa). Temperature is stored in degC.
- The unit system only sets how values are shown and typed. Typing "3 cm" converts the value.
- An exported .inp file is in SI. The CalculiX keyword editor says so.
- Results are shown in the analysis' unit system (legend, scale bar, queries).
- Format 2 projects (one model space for all analyses) are converted on open. Their results are dropped and must be solved again. Once saved, the project no longer opens in older versions.

## 6. Outdated passages to rewrite

- chapters/interface/project.typ lines 18-19 and 85: "model space and unit system belong to the project" and the "Model Space and Units ..." menu entry, which no longer exists.
- chapters/model/model-properties.typ lines 15-16 and 33: File > New Model, changing the space or units, and refused space changes.
- chapters/interface/files.typ line 11 and main-window.typ line 32: New Model (Ctrl+N) has been removed from the analysis window.
- chapters/interface/main-window.typ line 27: the status bar of the analysis window still shows the units. The project window no longer does.
- chapters/geometry-mesh/geometry.typ lines 12 and 43-44: import needs an open analysis. A 2D mesh has to match the analysis' 2D space.
- chapters/first-analysis.typ line 10: start with Project > New Analysis ...

## Not in this stage

- Global mesh overlay over several analyses.
- Detect Cut Faces for submodels.
