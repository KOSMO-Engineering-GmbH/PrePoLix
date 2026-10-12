# Draft: results of one analysis as input of another

Outline for the user guide chapter on the project schematic (to be written in Typst later).

The step rows under Results give their results to other analyses. Where they go decides what
they are used for:

| Source step | Dropped on | Used as |
|---|---|---|
| Heat transfer, coupled or uncoupled temperature-displacement | a step of another analysis | the temperatures of that step: a defined field "Linked Temperatures" (`*TEMPERATURE, FILE=..., BSTEP=n`) |
| the same | Setup > Initial Conditions of another analysis | the temperatures the analysis starts at (`*INITIAL CONDITIONS, TYPE=TEMPERATURE` on every node) |
| Frequency with Storage | a Modal Dynamics or Steady State Dynamics step | the eigenmodes these steps work on: the `.eig` file of the other run |
| any other | a step of another analysis | the displacements on the cut boundary of a submodel (as before) |

- Temperatures and eigenmodes go node by node, so both analyses need the same mesh: link the
  mesh, or continue from a state. Otherwise the output pane says why nothing is taken.
- A frequency step without Storage cannot give its eigenmodes; the link is refused with a
  hint.
- A step that works on eigenmodes of another analysis needs no frequency step before it. Its
  boundary conditions are written before the first step, as CalculiX allows no new ones in
  a modal or steady state dynamics step.
- Defined temperatures need initial temperatures: a link to Initial Conditions provides them
  (the model check counts it).
- When the source has no results yet, Solve and Update Project solve it first; when it is
  solved again, the analyses using its results are solved after it.
- Unlink takes back what the link set: the defined field, the eigenmodes or the initial
  temperatures.
- Screenshot: temperature-links.png (one link to Initial Conditions, one to the step Load).
