// PrePoLix program icon, drawn procedurally: a meshed part coloured like a contour plot.
//
// Inputs (typst --input key=value):
//   design  wheel | gear | plate         which motif (draft stage)
//   variant color | mono                 mono: one colour on a transparent background
//   detail  full | small                 small: coarser mesh, no mesh lines, for 16-32 px
//
// The page is 256 pt square; export with --ppi 72 for 256 px.

#let design = sys.inputs.at("design", default: "wheel")
#let variant = sys.inputs.at("variant", default: "color")
#let detail = sys.inputs.at("detail", default: "full")
#let small = detail == "small"
#let mono = variant == "mono"

#let S = 256
#set page(width: S * 1pt, height: S * 1pt, margin: 0pt, fill: none)

// ---------------------------------------------------------------- colours

#let navy = rgb("#0e2448")
#let mono-ink = rgb("#1b2a3a")
// Contour colours like a CalculiX/PrePoMax result legend, blue (low) to red (high).
#let legend = (
  rgb("#2a3fe0"), rgb("#1e7ff0"), rgb("#18b8e8"), rgb("#1fd6b0"), rgb("#4ad94a"),
  rgb("#b4e02c"), rgb("#f5dc1e"), rgb("#ffa21c"), rgb("#f8601a"), rgb("#e0201e"),
)
#let band(v) = {
  let v = calc.clamp(v, 0.0, 0.9999)
  legend.at(int(v * legend.len()))
}

// ---------------------------------------------------------------- geometry helpers

#let C = S / 2
// Deterministic jitter so the structured mesh looks like a free mesh.
#let noise(i, j, k) = calc.fract(calc.sin(i * 12.9898 + j * 78.233 + k * 37.719) * 43758.5453) - 0.5

// Mesh a parametric patch f(s, t) -> (x, y) with s, t in [0, 1] into triangles.
// Returns a list of (points, centroid) where points are three (x, y) pairs.
#let patch(f, ns, nt, jitter: 0.0, seed: 0, closed-t: false) = {
  let node(i, j) = {
    let s = i / ns
    let t = j / nt
    let inner-s = i > 0 and i < ns
    let inner-t = closed-t or (j > 0 and j < nt)
    if inner-s and inner-t and jitter > 0 {
      let jj = if closed-t { calc.rem(j, nt) } else { j }
      s += noise(i, jj, seed) * jitter / ns
      t += noise(i, jj, seed + 7) * jitter / nt
    }
    f(s, t)
  }
  let tris = ()
  for i in range(ns) {
    for j in range(nt) {
      let a = node(i, j)
      let b = node(i + 1, j)
      let c = node(i + 1, j + 1)
      let d = node(i, j + 1)
      if calc.rem(i + j, 2) == 0 {
        tris.push((a, b, c))
        tris.push((a, c, d))
      } else {
        tris.push((a, b, d))
        tris.push((b, c, d))
      }
    }
  }
  tris
}

#let centroid(t) = ((t.at(0).at(0) + t.at(1).at(0) + t.at(2).at(0)) / 3,
                    (t.at(0).at(1) + t.at(1).at(1) + t.at(2).at(1)) / 3)

#let polar(r, a) = (C + r * calc.cos(a * 1deg), C - r * calc.sin(a * 1deg))

// Shrink a triangle towards its centroid (mono variant: mesh shows as gaps).
#let shrink(t, gap) = {
  let (cx, cy) = centroid(t)
  t.map(p => {
    let dx = p.at(0) - cx
    let dy = p.at(1) - cy
    let l = calc.sqrt(dx * dx + dy * dy)
    let k = if l > gap { (l - gap) / l } else { 0.0 }
    (cx + dx * k, cy + dy * k)
  })
}

// Draw meshed parts; field(x, y) -> value in [0, 1].
#let draw-parts(parts, field, outline-w: 0pt) = {
  let line-w = if small { 0pt } else { 0.9pt }
  // Dark outline around each part: draw every triangle with a thick stroke first.
  if not mono and outline-w > 0pt {
    for tris in parts {
      for t in tris {
        place(polygon(fill: navy, stroke: (paint: navy, thickness: outline-w, join: "round"),
          ..t.map(p => (p.at(0) * 1pt, p.at(1) * 1pt))))
      }
    }
  }
  for tris in parts {
    for t in tris {
      if mono {
        let g = if small { 0.0 } else { 0.9 }
        let q = shrink(t, g)
        place(polygon(fill: mono-ink, stroke: if small { 0.4pt + mono-ink } else { none },
          ..q.map(p => (p.at(0) * 1pt, p.at(1) * 1pt))))
      } else {
        let (cx, cy) = centroid(t)
        let col = band(field(cx, cy))
        place(polygon(fill: col,
          stroke: if line-w > 0pt { (paint: white.transparentize(35%), thickness: line-w, join: "round") }
                  else { 0.3pt + col },
          ..t.map(p => (p.at(0) * 1pt, p.at(1) * 1pt))))
      }
    }
  }
}

#let background() = {
  if not mono {
    place(rect(width: 100%, height: 100%, radius: 56pt, fill: gradient.linear(
      rgb("#13305e"), navy, angle: 60deg)))
  }
}

// ---------------------------------------------------------------- motif: impeller wheel

#let wheel() = {
  let hub-r = (24.0, 46.0)
  let rim-r = (90.0, 104.0)
  let blades = if small { 5 } else { 7 }
  let k = if small { 0.5 } else { 1.0 }
  let parts = ()
  // Hub annulus
  parts.push(patch((s, t) => polar(hub-r.at(0) + s * (hub-r.at(1) - hub-r.at(0)), t * 360),
    calc.max(1, int(2 * k)), int(28 * k), jitter: 0.35, seed: 1, closed-t: true))
  // Rim annulus
  parts.push(patch((s, t) => polar(rim-r.at(0) + s * (rim-r.at(1) - rim-r.at(0)), t * 360),
    calc.max(1, int(2 * k)), int(64 * k), jitter: 0.35, seed: 2, closed-t: true))
  // Curved blades from the hub to the rim, thicker at the root.
  for b in range(blades) {
    let a0 = b * 360 / blades + 15
    let f = (s, t) => {
      let r = hub-r.at(1) - 3 + s * (rim-r.at(0) - hub-r.at(1) + 6)
      let w = 26 * (1 - s) + 13 * s          // angular-ish width in pt
      let a = a0 + 38 * calc.pow(s, 1.2) + (t - 0.5) * w / r * 180 / calc.pi
      polar(r, a)
    }
    parts.push(patch(f, int(8 * k), calc.max(1, int(3 * k)), jitter: 0.3, seed: 10 + b))
  }
  // Field: high at the outer right, low inner left (centrifugal load plus a side load).
  let field = (x, y) => {
    let dx = x - C
    let dy = y - C
    let r = calc.sqrt(dx * dx + dy * dy) / rim-r.at(1)
    (r - 0.24) / 0.76 * 0.8 + 0.25 * dx / rim-r.at(1)
  }
  draw-parts(parts, field, outline-w: if small { 1.5pt } else { 3pt })
}

// ---------------------------------------------------------------- motif: gear

#let gear() = {
  let teeth = if small { 8 } else { 9 }
  let r-hole = 26.0
  let r-root = 72.0
  let r-tip = 104.0
  let k = if small { 0.5 } else { 1.0 }
  let tooth = 360 / teeth
  // Outer radius as a trapezoid tooth profile over the angle.
  let outer(a) = {
    let u = calc.rem-euclid(a / tooth, 1.0)
    let rise = 0.16
    if u < 0.25 { r-root }
    else if u < 0.25 + rise { r-root + (r-tip - r-root) * (u - 0.25) / rise }
    else if u < 0.75 - rise { r-tip }
    else if u < 0.75 { r-tip - (r-tip - r-root) * (u - 0.75 + rise) / rise }
    else { r-root }
  }
  let parts = ()
  // Body: hole to root circle; teeth: root to profile.
  parts.push(patch((s, t) => polar(r-hole + s * (r-root - r-hole), t * 360),
    int(5 * k), int(teeth * 4 * k), jitter: 0.4, seed: 3, closed-t: true))
  for i in range(teeth) {
    let a0 = i * tooth + 0.2 * tooth
    let a1 = i * tooth + 0.8 * tooth
    let f = (s, t) => {
      let a = a0 + t * (a1 - a0)
      // Narrow the tooth towards the tip.
      let am = (a0 + a1) / 2
      let a = am + (a - am) * (1 - 0.55 * s)
      polar(r-root + s * (outer(am) - r-root) , a)
    }
    parts.push(patch(f, calc.max(1, int(3 * k)), calc.max(2, int(4 * k)), jitter: 0.3, seed: 20 + i))
  }
  // Field: stress concentration at the loaded tooth (upper right), decaying with distance.
  let hot = polar(r-root + 4, 45)
  let field = (x, y) => {
    let d = calc.sqrt(calc.pow(x - hot.at(0), 2) + calc.pow(y - hot.at(1), 2))
    let r = calc.sqrt(calc.pow(x - C, 2) + calc.pow(y - C, 2))
    1.02 * calc.exp(-d / 62) + 0.12 * (1 - r / r-tip)
  }
  draw-parts(parts, field, outline-w: if small { 1.5pt } else { 3pt })
  // Shaft key way look: a small dark hole edge is already the background.
}

// ---------------------------------------------------------------- motif: plate with hole

#let plate() = {
  let half = 86.0
  let hole = 34.0
  let k = if small { 0.5 } else { 1.0 }
  // O-grid: from the hole circle to the square outline, per side.
  let parts = ()
  for side in range(4) {
    let f = (s, t) => {
      let a = side * 90 - 45 + t * 90
      let inner = polar(hole, a)
      // Point on the square boundary along the same angle.
      let c = calc.cos(a * 1deg)
      let sn = calc.sin(a * 1deg)
      let m = calc.max(calc.abs(c), calc.abs(sn))
      let outer = (C + half * c / m, C - half * sn / m)
      let s2 = calc.pow(s, 1.35)  // finer near the hole
      (inner.at(0) + s2 * (outer.at(0) - inner.at(0)), inner.at(1) + s2 * (outer.at(1) - inner.at(1)))
    }
    parts.push(patch(f, int(6 * k), int(7 * k), jitter: 0.3, seed: 30 + side))
  }
  // Kirsch solution for tension in x: hoop stress, peak (3x) at the top and bottom of the hole.
  let field = (x, y) => {
    let dx = x - C
    let dy = C - y
    let r = calc.max(calc.sqrt(dx * dx + dy * dy), hole)
    let th = calc.atan2(dx, dy)
    let q = hole * hole / (r * r)
    let sig = 0.5 * (1 + q) - 0.5 * (1 + 3 * q * q) * calc.cos(2 * th)
    (sig + 1) / 4
  }
  draw-parts(parts, field, outline-w: if small { 1.5pt } else { 3pt })
}

// ---------------------------------------------------------------- page

#box(width: 100%, height: 100%, {
  background()
  // Small icons: the motif fills more of the square so it stays legible.
  let motif = if design == "wheel" { wheel() } else if design == "gear" { gear() } else { plate() }
  if small { scale(110%, origin: center + horizon, motif) } else { motif }
})
