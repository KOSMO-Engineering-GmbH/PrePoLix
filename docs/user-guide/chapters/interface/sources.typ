#import "../../template.typ": *

== Source blocks <source-blocks>

Besides analyses, the project schematic holds *source blocks*: blocks that only give data to
analyses through links. They have no steps, are not solved, and #ui("Update Project") passes
them by. Add one from #menu("Project") or by right-clicking the background of the schematic;
its header has its own tint, so that it stands apart from the analyses. Its context menu has
#ui("Edit ..."), #ui("Rename ..."), #ui("Duplicate") and #ui("Delete ..."); a double-click
opens its editor in the project window. Deleting a source block removes its links; the
analyses keep copies of what they took.

=== Materials <materials-block>

A materials block holds materials in the project file, so that several analyses use the same
definition: change it once in the block and every analysis that links it follows. Unlike the
material library (@materials), which belongs to you and every project you open, the block
belongs to the project and travels with it.

#screenshot("materials-block.png", [A materials block: Static takes S235, Modal takes all of
  them])

Its rows are #ui("Materials"), which gives all of its materials, and one row per material.
Drag a row to the #ui("Setup") row of an analysis, or to its #ui("Materials") row or a
material once #ui("Setup") is open, as with links between analyses (@data-links). All of
them also pass on materials added to the block later. In the analysis window the linked
materials carry the link sign and are read-only; #ui("Open Materials") in their dialog opens
the block's editor.

#screenshot("materials-block-editor.png", [The editor of a materials block])

#fields(
  [Shown in], [The unit system the values are shown and typed in. Like the values of an
    analysis they are stored in SI units, so each analysis shows them in its own.],
  [New ...], [Defines a new material in the dialog of the FE Model tree (@materials).],
  [Edit ...], [Edits the selected material; a double-click on it does the same.],
  [Delete], [Removes the selected material.],
  [Add from Library ...], [Opens the material library (@material-library) with the block's
    materials in place of the model's: copy materials from the library into the block, or from
    the block into the library.],
  [Import from Analysis], [Copies the materials of an analysis into the block, so that a
    project can gather them in one place afterwards. A material of the same name is replaced.],
)

#ui("OK") keeps the changes, and the analyses that link the block take them at once;
#ui("Cancel") drops them.
