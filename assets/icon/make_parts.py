#!/usr/bin/env python3
"""Builds the 3D parts the program icon is drawn from and solves them with CalculiX.

For every part this writes, next to this script:
  parts/<name>.inp   hexahedral mesh (C3D8I) with a static load step, a normal CalculiX input
  parts/<name>.val   one line "node value" per node, the result the icon is coloured with

prepolix-icon.typ reads both. Only needed when the parts change, the outputs are checked in.

    python3 assets/icon/make_parts.py          # needs ccx on PATH
"""

import math
import pathlib
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
PARTS = HERE / "parts"


class Mesh:
    def __init__(self):
        self.nodes = {}  # key -> (id, (x, y, z))
        self.elements = []

    def node(self, key, xyz):
        if key not in self.nodes:
            self.nodes[key] = (len(self.nodes) + 1, xyz)
        return self.nodes[key][0]

    def by_id(self):
        return {nid: xyz for nid, xyz in self.nodes.values()}


def polar(r, a, z):
    return (r * math.cos(a), r * math.sin(a), z)


def extrude(mesh, cells, layers, z0, z1, tag):
    """cells: list of ((key, xy) * 4) counter-clockwise quads; extruded into hexahedra."""
    for quad in cells:
        ids = []
        for k in range(layers + 1):
            z = z0 + (z1 - z0) * k / layers
            ids.append([mesh.node((tag, key, round(z, 9)), (xy[0], xy[1], z)) for key, xy in quad])
        for k in range(layers):
            mesh.elements.append(ids[k] + ids[k + 1])


# ------------------------------------------------------------------ gear


def gear():
    teeth, r_hole, r_root, r_tip, width = 12, 11.0, 32.0, 42.0, 10.0
    per_tooth, n_body, n_tooth = 8, 5, 4
    nt = teeth * per_tooth

    def body(i, j):
        r = r_hole + (r_root - r_hole) * (i / n_body) ** 0.9
        return (("b", i, j % nt), polar(r, 2 * math.pi * j / nt, 0)[:2])

    def tooth(t, i, j):
        # j = 0..4 across the tooth, which occupies half the pitch and narrows to the tip.
        if i == 0:
            return body(n_body, t * per_tooth + 2 + j)
        pitch = 2 * math.pi / teeth
        mid = t * pitch + pitch / 2
        a = mid + (j - 2) / 4 * pitch / 2 * (1 - 0.55 * i / n_tooth)
        r = r_root + (r_tip - r_root) * i / n_tooth
        return (("t", t, i, j), polar(r, a, 0)[:2])

    cells = []
    for i in range(n_body):
        for j in range(nt):
            cells.append([body(i, j), body(i + 1, j), body(i + 1, j + 1), body(i, j + 1)])
    for t in range(teeth):
        for i in range(n_tooth):
            for j in range(4):
                cells.append([tooth(t, i, j), tooth(t, i + 1, j), tooth(t, i + 1, j + 1), tooth(t, i, j + 1)])
    mesh = Mesh()
    # Body and teeth share the root circle nodes; one tag keeps them conforming.
    extrude(mesh, cells, 3, 0.0, width, "g")
    nodes = mesh.by_id()
    fixed = [n for n, (x, y, z) in nodes.items() if math.hypot(x, y) < r_hole + 1e-6]
    # Torque: a tangential force on one flank of every tooth.
    pitch = 2 * math.pi / teeth
    flank = []
    for n, (x, y, z) in nodes.items():
        r = math.hypot(x, y)
        if r <= r_root + 1:
            continue
        a = math.atan2(y, x) % (2 * math.pi)
        u = (r - r_root) / (r_tip - r_root)
        for t in range(teeth):
            a_flank = t * pitch + pitch / 2 + pitch / 4 * (1 - 0.55 * u)
            if abs(a - a_flank % (2 * math.pi)) < 1e-6:
                flank.append((n, a))
    loads = []
    for n, a in flank:
        f = -4000.0 / len(flank)
        loads += [(n, 1, -f * math.sin(a)), (n, 2, f * math.cos(a))]
    return mesh, fixed, loads, None, "mises"


# ------------------------------------------------------------------ impeller


def impeller():
    blades, r_hole, r_out, disc, height = 7, 9.0, 44.0, 4.0, 11.0
    nr, per_blade, blade_cells = 12, 8, 3
    na = blades * per_blade
    sweep = math.radians(70)

    def grid(i, j):
        s = i / nr
        r = r_hole + (r_out - r_hole) * s
        a = 2 * math.pi * j / na + sweep * s**1.3
        return (("d", i, j % na), polar(r, a, 0)[:2])

    disc_cells, blade_cells_list = [], []
    for i in range(nr):
        for j in range(na):
            quad = [grid(i, j), grid(i + 1, j), grid(i + 1, j + 1), grid(i, j + 1)]
            disc_cells.append(quad)
            if i >= 3 and j % per_blade < blade_cells:
                blade_cells_list.append(quad)
    mesh = Mesh()
    extrude(mesh, disc_cells, 2, 0.0, disc, "g")
    # Blades start on the top face of the disc: same keys at z = disc, so they share nodes.
    extrude(mesh, blade_cells_list, 3, disc, disc + height, "g")
    nodes = mesh.by_id()
    fixed = [n for n, (x, y, z) in nodes.items() if math.hypot(x, y) < r_hole + 1e-6]
    return mesh, fixed, [], 2000.0, "displacement"


# ------------------------------------------------------------------ CalculiX


def write_inp(path, mesh, fixed, loads, omega):
    nodes = mesh.by_id()
    lines = ["** PrePoLix icon part, written by assets/icon/make_parts.py", "*NODE, NSET=NALL"]
    lines += [f"{n}, {x:.6f}, {y:.6f}, {z:.6f}" for n, (x, y, z) in sorted(nodes.items())]
    lines.append("*ELEMENT, TYPE=C3D8I, ELSET=EALL")
    lines += [f"{e}, " + ", ".join(map(str, ids)) for e, ids in enumerate(mesh.elements, 1)]
    lines.append("*NSET, NSET=FIXED")
    lines += [f"{n}," for n in fixed]
    lines += [
        "*MATERIAL, NAME=STEEL",
        "*ELASTIC",
        "210000, 0.3",
        "*DENSITY",
        "7.85e-9",
        "*SOLID SECTION, ELSET=EALL, MATERIAL=STEEL",
        "*STEP",
        "*STATIC",
        "*BOUNDARY",
        "FIXED, 1, 3",
    ]
    if loads:
        lines.append("*CLOAD")
        lines += [f"{n}, {d}, {f:.6f}" for n, d, f in loads]
    if omega:
        lines += ["*DLOAD", f"EALL, CENTRIF, {omega**2:.1f}, 0, 0, 0, 0, 0, 1"]
    lines += ["*NODE FILE", "U", "*EL FILE", "S", "*END STEP", ""]
    path.write_text("\n".join(lines))


def read_frd(path):
    """Nodal displacement magnitude and von Mises stress from a CalculiX .frd file."""
    disp, mises, block = {}, {}, None
    for line in path.read_text().splitlines():
        if line.startswith(" -4"):
            block = line.split()[1]
        elif line.startswith(" -1") and block in ("DISP", "STRESS"):
            n = int(line[3:13])
            v = [float(line[13 + 12 * k : 25 + 12 * k]) for k in range((len(line) - 13) // 12)]
            if block == "DISP":
                disp[n] = math.sqrt(sum(c * c for c in v[:3]))
            else:
                sx, sy, sz, sxy, syz, szx = v[:6]
                mises[n] = math.sqrt(
                    0.5 * ((sx - sy) ** 2 + (sy - sz) ** 2 + (sz - sx) ** 2)
                    + 3 * (sxy**2 + syz**2 + szx**2)
                )
        elif line.startswith(" -3"):
            block = None
    return {"displacement": disp, "mises": mises}


def main():
    PARTS.mkdir(exist_ok=True)
    for name, build in (("gear", gear), ("impeller", impeller)):
        mesh, fixed, loads, omega, field = build()
        inp = PARTS / f"{name}.inp"
        write_inp(inp, mesh, fixed, loads, omega)
        with tempfile.TemporaryDirectory() as tmp:
            job = pathlib.Path(tmp) / name
            job.with_suffix(".inp").write_text(inp.read_text())
            subprocess.run(["ccx", "-i", str(job)], cwd=tmp, check=True, capture_output=True)
            values = read_frd(job.with_suffix(".frd"))[field]
        top = max(values.values())
        (PARTS / f"{name}.val").write_text(
            "".join(f"{n} {v / top:.4f}\n" for n, v in sorted(values.items()))
        )
        print(f"{name}: {len(mesh.nodes)} nodes, {len(mesh.elements)} elements, max {field} {top:.4g}")


if __name__ == "__main__":
    main()
