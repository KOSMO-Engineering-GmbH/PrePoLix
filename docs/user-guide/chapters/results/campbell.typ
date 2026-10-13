#import "../../template.typ": *

== Campbell diagram <campbell>

A Campbell diagram shows the eigenfrequencies of a rotor over its speed of rotation. CalculiX
computes the whirling modes for one speed per run, so PrePoLix runs the analysis once per speed
and collects the results.

#menu("Analysis", "Campbell diagram ...") is enabled when the model has an active
#ui("Centrifugal") load and a #ui("Complex Frequency") step (@complex-frequency). The window
runs the sweep and shows the diagram in the same window, not under #ui("Results"): the sweep
does not run the analysis of the tree and adds nothing to its results. The diagram appears as
soon as two speeds are done. Closing the window keeps the last sweep; the menu entry opens it
again with its diagram until another model is opened.

#fields(
  [Speed range \[rpm\]], [The lowest and the highest speed of the diagram, shown in rad/s
    next to them. Default: from 0 to the speed of the centrifugal load. Standstill itself is not
    computed, as CalculiX merges the coinciding modes there; a range from 0 starts at a fiftieth
    of the highest speed.],
  [Maximum runs], [Number of CalculiX runs after which the sweep stops by itself (default 40);
    0 runs until #ui("Stop") is pressed.],
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
file, without whirl directions. As many speeds run at the same time as #ui("Parallel analyses") of
the solver settings allows. The files of a speed are deleted once its frequencies are read; those
of a failed speed stay for a look.

The sweep needs no step size. It starts with eight speeds spread evenly over the range and then
keeps adding speeds where the curves bend most: between the two speeds whose curves deviate most
from a straight line, weighted with the width of the gap, so that a resolved curve lets the
speeds spread over the rest of the range. The diagram grows with every speed. The sweep ends at
#ui("Maximum runs"), at #ui("Stop"), which cancels the running speeds and keeps the finished ones,
or when no gap is wider than a five-hundredth of the range. Gaps next to a speed CalculiX could
not solve are not refined further. The progress and the messages of the runs appear below the
diagram. When fewer than two speeds give complex frequencies, the window says so in place of the
diagram; the messages below name the CalculiX error of each failed speed.

The diagram has one curve per mode and one dashed grey line per engine order, which starts at
0 rpm even though the sweep leaves out standstill. The modes are followed from speed to speed by
their course, not by their rank among the frequencies: a mode joins the curve it continues best,
so curves that cross keep their colour, and a curve that falls to zero and rises again (in the
rotating frame of reference, the forward whirl at its critical speed) stays one curve. Roots
CalculiX finds at fewer than three neighbouring speeds, such as a spurious root near 0 Hz, are
drawn as grey crosses named #ui("Unmatched roots") instead of tearing the curves; growing modes,
whose eigenvalue has an imaginary part of weight, are drawn as red diamonds named
#ui("Unstable"). A mode that whirls the same way at every speed carries its direction in the
legend, F (forward) or B (backward) as CalculiX reports it.

The diagram can be moved and zoomed like the plots of egui: drag it to pan, scroll to zoom,
drag along an axis to zoom only that axis, and double-click to fit the view to the curves again.
A click on a legend entry hides or shows that mode or order line. Hovering a point shows its
curve, speed and frequency.

#screenshot("campbell.png", [Campbell diagram of a spinning cantilever after an adaptive sweep;
Mode 7 is hidden by a click on its legend entry])

Where a mode crosses an order line, the rotor is
excited at one of its eigenfrequencies: these crossings are listed under #ui("Critical speeds")
with the order, the mode, its whirl, the speed in rpm and the frequency. #ui("Copy table")
copies the frequencies of all modes at all speeds as tab-separated text. #ui("Export CSV ...")
saves the same table as a CSV file, with a Save As dialog: one row per speed with the speed in
rad/s and rpm, the frequency in Hz and the whirl (F, B or empty) of every curve, the other roots
CalculiX found at that speed (unmatched and unstable, separated by spaces), and the frequency of
every order line. Numbers use a decimal point and the cells are separated by commas.

#note[CalculiX solves the Coriolis modes in the rotating frame of reference. The frequencies of
the diagram are therefore those an observer rotating with the shaft would see, and a pair of
bending modes splits into a rising and a falling branch with the speed.]
