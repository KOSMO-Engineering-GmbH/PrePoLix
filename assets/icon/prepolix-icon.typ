// PrePoLix program icon: a meshed 3D part from a CalculiX input file, coloured with its
// CalculiX result like a PrePoMax contour plot.
//
// The part comes from make_part.py: part/gear.inp holds the mesh (C3D8/C3D8I, only the
// corner nodes are used), part/gear.val one "node value" line per node with the result
// scaled to [0, 1]. Any hexahedral .inp works the same way.
//
// Inputs (typst --input key=value):
//   variant color | mono | light     color: on the raised tile, cut by its edge;
//                                    mono: dark grey faces with light lines, light: light
//                                    grey faces with dark lines; both whole, on a
//                                    transparent background
//   detail  full | small             small: no mesh lines, for 16-32 px
//
// The page is 256 pt square. scripts/make_icon.py exports all files the program uses.

#let variant = sys.inputs.at("variant", default: "color")
#let detail = sys.inputs.at("detail", default: "full")
#let small = detail == "small"
#let light-mono = variant == "light"
#let mono = variant == "mono" or light-mono

#let S = 256.0
#set page(width: S * 1pt, height: S * 1pt, margin: 0pt, fill: none)

// ---------------------------------------------------------------- colours

#let navy = rgb("#0e2448")
#let mono-ink = rgb("#1b2a3a")
#let light-face = rgb("#dfe3e8")
#let light-line = rgb("#3a4350")
// Contour colours like a CalculiX/PrePoMax result legend, blue (low) to red (high).
#let legend = (
  rgb("#2a3fe0"), rgb("#1e7ff0"), rgb("#18b8e8"), rgb("#1fd6b0"), rgb("#4ad94a"),
  rgb("#b4e02c"), rgb("#f5dc1e"), rgb("#ffa21c"), rgb("#f8601a"), rgb("#e0201e"),
)

// View angles, how the result maps onto the legend, and how large the part sits on the
// page: on the tile it is enlarged and moved to the lower right so the tile edge cuts it.
#let view = (azimuth: -62deg, tilt: 42deg, gamma: 1.0)
#let fill-frac = if mono { 0.94 } else { 1.25 }
#let shift = if mono { (0.0, 0.0) } else { (34.0, 30.0) }  // pt, x right, y down

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

#let (nodes, elements) = read-inp(read("part/gear.inp"))
#let values = read-values(read("part/gear.val"), nodes.len())

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
#let zoom = fill-frac * S / calc.max(x1 - x0, y1 - y0)
#let screen(n) = {
  let p = projected.at(n)
  (((p.at(0) - (x0 + x1) / 2) * zoom + S / 2 + shift.at(0)) * 1pt,
   (S / 2 - (p.at(1) - (y0 + y1) / 2) * zoom + shift.at(1)) * 1pt)
}

#let R = 56.0  // tile corner radius
#let bevel = 7.0

// Visible faces on the tile face, far ones first (painter's algorithm), with their normal for shading.
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
    let on-page = f.map(screen).any(((x, y)) => x > bevel * 1pt and x < (S - bevel) * 1pt and y > bevel * 1pt and y < (S - bevel) * 1pt)
    if len > 0 and nz / len > 0.01 and (mono or on-page) {
      let depth = p.map(q => q.at(2)).sum() / 4
      out.push((f, depth, (nx / len, ny / len, nz / len)))
    }
  }
  out.sorted(key: e => e.at(1))
}

// ---------------------------------------------------------------- drawing

// Raised tile: a bevel lit from the upper left around a slightly sunken face, a glow
// behind the part and its shadow on the face.
#let background() = {
  if mono { return }
  place(rect(width: 100%, height: 100%, radius: R * 1pt,
    fill: gradient.linear(rgb("#3a68b0"), rgb("#14305c"), rgb("#050f22"), angle: 45deg)))
  place(dx: bevel * 1pt, dy: bevel * 1pt, rect(width: (S - 2 * bevel) * 1pt, height: (S - 2 * bevel) * 1pt,
    radius: (R - bevel) * 1pt, fill: gradient.linear(rgb("#0c2147"), rgb("#163a70"), angle: 45deg)))
  // Glow in the colour of the low end of the legend.
  place(center + horizon, circle(radius: 112pt,
    fill: gradient.radial(rgb(60, 140, 255, 110), rgb(60, 140, 255, 0), radius: 50%)))
}

#let shadow() = {
  if mono { return }
  place(center + horizon, dy: 22pt, ellipse(width: 236pt, height: 150pt,
    fill: gradient.radial(rgb(0, 0, 0, 150), rgb(0, 0, 0, 60), rgb(0, 0, 0, 0), radius: 50%)))
}

// Cut a polygon of (x, y, value) corners down to the part where lo <= value <= hi
// (Sutherland-Hodgman with the value as the clipping coordinate).
#let clip-value(poly, keep, cut) = {
  let out = ()
  for i in range(poly.len()) {
    let a = poly.at(i)
    let b = poly.at(calc.rem(i + 1, poly.len()))
    let ka = keep(a.at(2))
    if ka { out.push(a) }
    if ka != keep(b.at(2)) {
      let t = (cut - a.at(2)) / (b.at(2) - a.at(2))
      out.push((a.at(0) + t * (b.at(0) - a.at(0)), a.at(1) + t * (b.at(1) - a.at(1)), cut))
    }
  }
  out
}

// Split a face into contour bands: the band edges run through the face like in a
// PrePoMax contour plot instead of one colour per face.
#let bands(corners) = {
  let n = legend.len()
  let vs = corners.map(c => c.at(2))
  let k0 = calc.clamp(int(calc.min(..vs) * n), 0, n - 1)
  let k1 = calc.clamp(int(calc.max(..vs) * n), 0, n - 1)
  if k0 == k1 { return ((k0, corners),) }
  let out = ()
  for k in range(k0, k1 + 1) {
    let poly = corners
    if k > k0 { poly = clip-value(poly, v => v >= k / n, k / n) }
    if k < k1 and poly.len() > 0 { poly = clip-value(poly, v => v <= (k + 1) / n, (k + 1) / n) }
    if poly.len() >= 3 { out.push((k, poly)) }
  }
  out
}

#let pt2(c) = (c.at(0) * 1pt, c.at(1) * 1pt)

#let part() = {
  for (f, depth, n) in visible {
    // Light from the upper left front.
    let light = calc.clamp(0.3 + 0.7 * (0.75 * n.at(2) - 0.35 * n.at(0) + 0.5 * n.at(1)), 0.0, 1.0)
    let corners = f.map(k => {
      let (x, y) = screen(k)
      (x / 1pt, y / 1pt, calc.pow(values.at(k), view.gamma))
    })
    if mono {
      let col = if light-mono { light-face.darken((1 - light) * if small { 70% } else { 40% }) }
        else { mono-ink.lighten((1 - light) * 55%) }
      let edge = if small { 0.35pt + col }
        else if light-mono { (paint: light-line, thickness: 0.55pt, join: "round") }
        else { (paint: white.transparentize(30%), thickness: 0.5pt, join: "round") }
      place(polygon(fill: col, stroke: edge, ..corners.map(pt2)))
    } else {
      let edge = if small { none } else { (paint: rgb(0, 0, 0, 80), thickness: 0.4pt, join: "round") }
      let pieces = bands(corners)
      if pieces.len() == 1 {
        let col = legend.at(pieces.at(0).at(0)).darken((1 - light) * 50%)
        place(polygon(fill: col, stroke: if edge == none { 0.35pt + col } else { edge }, ..corners.map(pt2)))
      } else {
        for (k, poly) in pieces {
          let col = legend.at(k).darken((1 - light) * 50%)
          // A hairline in the fill colour closes the seams between neighbouring pieces.
          place(polygon(fill: col, stroke: 0.35pt + col, ..poly.map(pt2)))
        }
        if edge != none { place(polygon(stroke: edge, ..corners.map(pt2))) }
      }
    }
  }
}

#box(width: 100%, height: 100%, {
  background()
  let tile = (S - 2 * bevel) * 1pt
  // Everything inside the bevel is clipped to the tile face, so a large part runs into
  // the bevel instead of floating above the tile.
  place(dx: bevel * 1pt, dy: bevel * 1pt, box(width: tile, height: tile, clip: not mono,
    radius: (R - bevel) * 1pt, place(dx: -bevel * 1pt, dy: -bevel * 1pt, {
      shadow()
      part()
    })))
})
