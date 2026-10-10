#import "../../template.typ": *

== Campbell diagram <campbell>

A Campbell diagram shows the eigenfrequencies of a rotor over its speed of rotation. CalculiX
computes the whirling modes for one speed per run, so PrePoLix runs the analysis once per speed
and collects the results. PrePoMax has no such tool.

#menu("Analysis", "Campbell diagram ...") is enabled when the model has an active
#ui("Centrifugal") load and a #ui("Complex Frequency") step (@complex-frequency). The window
runs the sweep and shows the diagram in the same window, not under #ui("Results"): the sweep
does not run the analysis of the tree and adds nothing to its results. The diagram appears as
soon as two speeds are done. Closing the window keeps the last sweep; the menu entry opens it
again with its diagram until another model is opened.

#screenshot("campbell.png", [Campbell diagram of a spinning cantilever])

#fields(
  [Highest speed \[rpm\]], [The speed at the right end of the diagram, shown in rad/s next to
    it. Default: the speed of the centrifugal load.],
  [Speed steps], [Number of speeds between standstill and the highest speed (default 10, at
    least 2).
    CalculiX runs once per speed; standstill itself is not computed, as CalculiX merges the
    coinciding modes there.],
  [Engine orders], [The excitation lines $f = "order" times "speed"$ drawn into the diagram, as
    a comma-separated list such as `1, 2, 3`. They can be changed after the sweep.],
  [Frequency axis up to \[Hz\]], [Upper end of the frequency axis; 0 (default) fits the axis to
    the modes. A limit zooms in on the low frequencies, where the order lines of a stiff rotor
    run close to the speed axis. It can be changed at any time, also during the sweep.],
)

#ui("Start sweep") scales the centrifugal loads of the model to every speed, runs CalculiX in
the folder `Campbell` of the work directory (@settings) and reads the complex frequencies of the
last Complex Frequency step and the whirl direction of every mode from the `.dat` file. When the
`.dat` file of a CalculiX version has no readable complex frequencies, they come from the `.frd`
file, without whirl directions. The
progress and the messages of the runs appear below the diagram; #ui("Cancel") stops the sweep. When
fewer than two speeds give complex frequencies, the window says so in place of the diagram; the
messages below name the CalculiX error of each failed speed.

The diagram has one curve per mode and one dashed grey line per engine order.
The order lines start at 0 rpm even though the sweep leaves
out standstill, and carry their order as a label. A mode that whirls the same
way at every speed carries its direction in the legend, F (forward) or B (backward) as CalculiX
reports it. Where a mode crosses an order line, the rotor is
excited at one of its eigenfrequencies: these crossings are listed under #ui("Critical speeds")
with the order, the mode, its whirl, the speed in rpm and the frequency. #ui("Copy table")
copies the frequencies of all modes at all speeds as tab-separated text. #ui("Export CSV ...")
saves the same table as a CSV file, with a Save As dialog: one row per speed with the speed in
rad/s and rpm, the frequency in Hz and the whirl (F, B or empty) of every mode, and the frequency
of every order line. Numbers use a decimal point and the cells are separated by commas.

#note[CalculiX solves the Coriolis modes in the rotating frame of reference. The frequencies of
the diagram are therefore those an observer rotating with the shaft would see, and a pair of
bending modes splits into a rising and a falling branch with the speed.]
