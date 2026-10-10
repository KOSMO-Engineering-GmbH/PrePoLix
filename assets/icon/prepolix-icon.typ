// PrePoLix program icon: a meshed 3D part from a CalculiX input file, coloured with its
// CalculiX result like a PrePoMax contour plot.
//
// The parts come from make_parts.py: parts/<design>.inp holds the mesh (C3D8/C3D8I,
// only the corner nodes are used), parts/<design>.val one "node value" line per node
// with the result scaled to [0, 1].
//
// Inputs (typst --input key=value):
//   design  gear | impeller          which part
//   variant color | mono             mono: shades of one colour on a transparent background
//   detail  full | small             small: no mesh lines, larger part, for 16-32 px
//
// The page is 256 pt square; export with --ppi 72 for 256 px.

#let design = sys.inputs.at("design", default: "gear")
#let variant = sys.inputs.at("variant", default: "color")
#let detail = sys.inputs.at("detail", default: "full")
#let small = detail == "small"
#let mono = variant == "mono"

#let S = 256.0
#set page(width: S * 1pt, height: S * 1pt, margin: 0pt, fill: none)

// ---------------------------------------------------------------- colours

#let navy = rgb("#0e2448")
#let mono-ink = rgb("#1b2a3a")
// Contour colours like a CalculiX/PrePoMax result legend, blue (low) to red (high).
#let legend = (
  rgb("#2a3fe0"), rgb("#1e7ff0"), rgb("#18b8e8"), rgb("#1fd6b0"), rgb("#4ad94a"),
  rgb("#b4e02c"), rgb("#f5dc1e"), rgb("#ffa21c"), rgb("#f8601a"), rgb("#e0201e"),
)
#let band(v) = legend.at(int(calc.clamp(v, 0.0, 0.9999) * legend.len()))

// Per part: view angles, how the result maps onto the legend, and the frame.
#let views = (
  gear: (azimuth: -62deg, tilt: 58deg, gamma: 0.8, fill: 0.86),
  impeller: (azimuth: -20deg, tilt: 50deg, gamma: 1.0, fill: 0.88),
)
#let view = views.at(design)

// ---------------------------------------------------------------- read the part

#let read-inp(text) = {
  let nodes = ()
  let elements = ()
  let section = none
  for line in text.split("\n") {
    let line = line.trim()
    if line == "" or line.starts-with("**") { continue }
    if line.starts-with("*") {
      let key = upper(line)
      section = if key.starts-with("*NODE,") or key == "*NODE" { "node" }
        else if key.starts-with("*ELEMENT") and (key.contains("C3D8") or key.contains("C3D20")) { "element" }
        else { none }
      continue
    }
    let cells = line.split(",").map(c => c.trim()).filter(c => c != "")
    if section == "node" {
      let id = int(cells.at(0))
      while nodes.len() <= id { nodes.push(none) }
      nodes.at(id) = cells.slice(1, 4).map(float)
    } else if section == "element" {
      // Corner nodes only; a C3D20 element continues on the next line, which we skip.
      if cells.len() >= 9 { elements.push(cells.slice(1, 9).map(int)) }
    }
  }
  (nodes, elements)
}

#let read-values(text, count) = {
  let values = range(count).map(_ => 0.0)
  for line in text.split("\n") {
    let cells = line.split(" ").filter(c => c != "")
    if cells.len() == 2 { values.at(int(cells.at(0))) = float(cells.at(1)) }
  }
  values
}

#let (nodes, elements) = read-inp(read("parts/" + design + ".inp"))
#let values = read-values(read("parts/" + design + ".val"), nodes.len())

// ---------------------------------------------------------------- outer faces

// Hexahedron faces with outward normals (CalculiX node order).
#let hex-faces = ((0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7))

#let outer-faces = {
  let seen = (:)
  for el in elements {
    for f in hex-faces {
      let ids = f.map(k => el.at(k))
      let key = ids.sorted().map(str).join("-")
      if key in seen { seen.at(key) = none } else { seen.insert(key, ids) }
    }
  }
  seen.values().filter(f => f != none)
}

// ---------------------------------------------------------------- view

// Turn about z (azimuth), then tip the part towards the viewer (tilt about x).
#let ca = calc.cos(view.azimuth)
#let sa = calc.sin(view.azimuth)
#let ct = calc.cos(view.tilt)
#let st = calc.sin(view.tilt)
#let eye(p) = {
  let x = p.at(0) * ca - p.at(1) * sa
  let y = p.at(0) * sa + p.at(1) * ca
  let z = p.at(2)
  // screen x, screen y (up), depth (towards the viewer)
  (x, y * ct + z * st, -y * st + z * ct)
}
#let projected = nodes.map(p => if p == none { none } else { eye(p) })

#let used = outer-faces.flatten().dedup()
#let xs = used.map(n => projected.at(n).at(0))
#let ys = used.map(n => projected.at(n).at(1))
#let (x0, x1, y0, y1) = (calc.min(..xs), calc.max(..xs), calc.min(..ys), calc.max(..ys))
#let fill-frac = if small { calc.min(view.fill + 0.08, 0.97) } else { view.fill }
#let zoom = fill-frac * S / calc.max(x1 - x0, y1 - y0)
#let screen(n) = {
  let p = projected.at(n)
  (((p.at(0) - (x0 + x1) / 2) * zoom + S / 2) * 1pt, (S / 2 - (p.at(1) - (y0 + y1) / 2) * zoom) * 1pt)
}

// Visible faces, far ones first (painter's algorithm), with their normal for shading.
#let visible = {
  let out = ()
  for f in outer-faces {
    let p = f.map(n => projected.at(n))
    let d1 = p.at(2).zip(p.at(0)).map(((a, b)) => a - b)
    let d2 = p.at(3).zip(p.at(1)).map(((a, b)) => a - b)
    let nx = d1.at(1) * d2.at(2) - d1.at(2) * d2.at(1)
    let ny = d1.at(2) * d2.at(0) - d1.at(0) * d2.at(2)
    let nz = d1.at(0) * d2.at(1) - d1.at(1) * d2.at(0)
    let len = calc.sqrt(nx * nx + ny * ny + nz * nz)
    if len > 0 and nz / len > 0.01 {
      let depth = p.map(q => q.at(2)).sum() / 4
      out.push((f, depth, (nx / len, ny / len, nz / len)))
    }
  }
  out.sorted(key: e => e.at(1))
}

// ---------------------------------------------------------------- drawing

#let background() = {
  if not mono {
    place(rect(width: 100%, height: 100%, radius: 56pt,
      fill: gradient.linear(rgb("#163564"), navy, angle: 60deg)))
  }
}

#let part() = {
  for (f, depth, n) in visible {
    // Light from the upper left front.
    let light = calc.clamp(0.3 + 0.7 * (0.75 * n.at(2) - 0.35 * n.at(0) + 0.5 * n.at(1)), 0.0, 1.0)
    let v = calc.pow(f.map(k => values.at(k)).sum() / 4, view.gamma)
    let col = if mono { mono-ink.lighten((1 - light) * 55%) } else { band(v).darken((1 - light) * 50%) }
    let edge = if small { 0.35pt + col }
      else if mono { (paint: white.transparentize(30%), thickness: 0.5pt, join: "round") }
      else { (paint: rgb(0, 0, 0, 90), thickness: 0.45pt, join: "round") }
    place(polygon(fill: col, stroke: edge, ..f.map(screen)))
  }
}

#box(width: 100%, height: 100%, {
  background()
  part()
})
