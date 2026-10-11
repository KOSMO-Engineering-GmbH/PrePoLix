#!/usr/bin/env python3
"""Fetches the CalculiX solver (ccx) that the PrePoLix releases ship.

The executables are the conda-forge builds of CalculiX, the same that FreeCAD's packages use:
the official ccx 2.23 sources with conda-forge's bug fixes, built with SPOOLES (multithreaded)
and ARPACK, and checked against a set of reference models before each build is published. The
script downloads the pinned conda packages of ccx and the libraries it needs, checks their
SHA-256 and unpacks into a folder, by default target/ccx/<platform>:

    bin/ccx, lib/*.so.*                   Linux; ccx finds the libraries via $ORIGIN/../lib
    bin/ccx.exe, bin/*.dll                Windows; DLLs next to the executable
    licenses/                             license texts of the bundled parts

Releases put this folder as `ccx` next to the prepolix executable, where prepolix finds it
when no other CalculiX is set in the settings. Development builds find it in target/ccx.

    python3 scripts/fetch_ccx.py [--platform linux|windows] [--dest DIR]

Needs the Python standard library and, before Python 3.14, the zstd program.
"""

import argparse
import hashlib
import io
import pathlib
import shutil
import subprocess
import sys
import tarfile
import zipfile

sys.dont_write_bytecode = True
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from fetch_gmsh import download  # noqa: E402

CHANNEL = "https://conda.anaconda.org/conda-forge"
# Also named in installer/*/THIRD-PARTY.txt and the user guide.
VERSION = "2.23"
# The recipe (patches, build flags) the ccx packages were built from, named in the packages'
# info/about.json; with the sources from www.dhondt.de it is the complete source of ccx.
FEEDSTOCK = "https://github.com/conda-forge/calculix-feedstock/tree/9d06fb5716f31488e18f67d13956164a6857e05c"

# The packages, fixed with their checksums from the channel's repodata. BLAS and LAPACK are
# the reference (netlib) builds: ccx solves with SPOOLES, ARPACK needs little of them, and
# they are small. On Windows the netlib build 3.9.0-6 is the last one made with MinGW; newer
# ones need the Visual C++ runtime, which may not be passed on without a Visual Studio license.
PACKAGES = {
    "linux": (
        "linux-64",
        {
            "calculix-2.23-pl5321h92dc11e_8.conda": "b0295e07a4a697ed510cd17e3f1451be00ffd7c28312aec201e7e4773abd7836",
            "arpack-3.9.1-nompi_hf03ea27_102.conda": "6d71343420292132be0192ddd962b308f7b8a0a0630d1db83fb9d65e8167c6ce",
            "libblas-3.11.0-10_h978bd12_netlib.conda": "38668b1a0f2cea27ddd8cb1ee573ab1494afda6c52c3c9ffa23c96669de35c1f",
            "liblapack-3.11.0-10_he2a6e96_netlib.conda": "3f83565d7a1b60f45324e92f47f509a6e833e812ef842618ce38e74f642ff66c",
            "libgfortran5-16.2.0-h6b99dfc_7.conda": "04dbab12c925b92977bb835dd2f5eda413607afa79f95a554e5a04b07a380322",
            "libgcc-16.2.0-ha9f2e26_7.conda": "e031634c3a928f9594eba868bc46ef5230d6571a364a0c63215b06c4563bfcea",
            "libgomp-16.2.0-he0feb66_7.conda": "92a0bb350c7be6a4eedcb4e111c60aefd176018e6b1c009a6e025dbe9bbcd758",
        },
    ),
    "windows": (
        "win-64",
        {
            "calculix-2.23-pl5321h7abefab_8.conda": "a6c1e75c092893d73d0bb3902cba9db7e14ba3fe4d2f8cd74cdfbc1bc041d1d1",
            "arpack-3.9.1-nompi_h1f7371d_102.conda": "393a0f11fe53c9a4ce8e003f0f9816ce098179d4c06eacffd573afbb0f2e2f0c",
            "libblas-3.9.0-6_h28841b1_netlib.conda": "dd5abd4d37ae08b044e7793ddb93db9c881535017ff223a1ceb4fb927301b4c1",
            "liblapack-3.9.0-6_h28841b1_netlib.conda": "8c149fbd857a5d33758fc6117252695c5f60dc00a396b48f567cc3a8c6668da5",
            "libgfortran5-16.2.0-h7c9de5e_7.conda": "e9b86fe14cb9938abd08f0b1685b04457339f71d9f00783fe9258a1eb0229963",
            "libgcc-16.2.0-h110b43a_7.conda": "59feb6d16cba13a70f22dae70a44670eb2cc45e652e9a34a014814c2ca871132",
            "libgomp-16.2.0-h8ee18e1_7.conda": "4b8a747f047d069ccd2a6cf2f4fdbc6b0c0e4dcd840998d1fe607b8c234e707b",
            "libwinpthread-12.0.0.r4.gg4f2fc60ca-h57928b3_10.conda": "0fccf2d17026255b6e10ace1f191d0a2a18f2d65088fd02430be17c701f8ffe0",
        },
    ),
}

# What ccx loads: on Linux by soname (the packages hold libfoo.so.1.2.3), on Windows by name.
LINUX_LIBRARIES = (
    "libarpack.so.2",
    "liblapack.so.3",
    "libblas.so.3",
    "libgfortran.so.5",
    "libquadmath.so.0",
    "libgomp.so.1",
    "libgcc_s.so.1",
)
WINDOWS_LIBRARIES = (
    "libarpack.dll",
    "liblapack.dll",
    "libblas.dll",
    "libgfortran-5.dll",
    "libquadmath-0.dll",
    "libgomp-1.dll",
    "libgcc_s_seh-1.dll",
    "libwinpthread-1.dll",
)

# License texts from the packages, under the name they get in licenses/. ARPACK's and
# winpthreads' packages carry none; their texts are in installer/ccx. The GCC runtime libraries
# are under the GPL 3 with the runtime library exception; the GPL 3 is PrePoLix' LICENSE.
LICENSES = {
    "calculix": {"info/licenses/LICENSE": "CalculiX-GPL-2.0.txt"},
    "liblapack": {"info/licenses/LICENSE": "LAPACK-LICENSE.txt"},
    "libgcc": {
        "share/licenses/gcc-libs/RUNTIME.LIBRARY.EXCEPTION": "GCC-RUNTIME-LIBRARY-EXCEPTION.txt",
        "Library/share/licenses/gcc-libs/RUNTIME.LIBRARY.EXCEPTION": "GCC-RUNTIME-LIBRARY-EXCEPTION.txt",
    },
}
EXTRA_LICENSES = ("ARPACK-LICENSE.txt", "WINPTHREADS-LICENSE.txt")


def unzstd(data):
    try:
        from compression import zstd  # Python 3.14 and newer

        return zstd.decompress(data)
    except ImportError:
        program = shutil.which("zstd")
        if program is None:
            sys.exit("zstd fehlt (apt install zstd), oder Python 3.14 verwenden")
        return subprocess.run(
            [program, "-dc"], input=data, check=True, stdout=subprocess.PIPE
        ).stdout


def members(package):
    """The files of a .conda package (a zip with zstd-compressed tars), as name -> bytes,
    with symbolic links resolved."""
    files = {}
    links = {}
    with zipfile.ZipFile(io.BytesIO(package)) as archive:
        for name in archive.namelist():
            if not name.endswith(".tar.zst"):
                continue
            with tarfile.open(fileobj=io.BytesIO(unzstd(archive.read(name)))) as tar:
                for member in tar:
                    if member.isfile():
                        files[member.name] = tar.extractfile(member).read()
                    elif member.issym():
                        target = pathlib.PurePosixPath(member.name).parent / member.linkname
                        links[member.name] = str(target)
    for name, target in links.items():
        if target in files:
            files[name] = files[target]
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    default = "windows" if sys.platform.startswith("win") else "linux"
    parser.add_argument("--platform", choices=sorted(PACKAGES), default=default)
    root = pathlib.Path(__file__).resolve().parent.parent
    parser.add_argument("--dest", type=pathlib.Path)
    args = parser.parse_args()
    dest = args.dest or root / "target" / "ccx" / args.platform

    subdir, packages = PACKAGES[args.platform]
    files = {}
    for filename, sha256 in packages.items():
        url = f"{CHANNEL}/{subdir}/{filename}"
        print(f"Lade {url}")
        data = download(url)
        if hashlib.sha256(data).hexdigest() != sha256:
            sys.exit(f"Prüfsumme von {filename} stimmt nicht, Download verworfen")
        name = filename.rsplit("-", 2)[0]
        files[name] = members(data)

    if dest.exists():
        shutil.rmtree(dest)
    (dest / "bin").mkdir(parents=True)
    (dest / "licenses").mkdir()
    everything = {path: data for package in files.values() for path, data in package.items()}

    def put(source, target, executable=False):
        target.write_bytes(everything[source])
        if executable:
            target.chmod(0o755)
        print(f"  {target}")

    if args.platform == "linux":
        put("bin/ccx", dest / "bin" / "ccx", executable=True)
        (dest / "lib").mkdir()
        for soname in LINUX_LIBRARIES:
            # Not every package links the soname to the file, libgomp.so.1 for one.
            source = min(path for path in everything if path.startswith(f"lib/{soname}"))
            put(source, dest / "lib" / soname, executable=True)
    else:
        put("Library/bin/ccx.exe", dest / "bin" / "ccx.exe")
        for dll in WINDOWS_LIBRARIES:
            source = next(
                path for path in everything if path.endswith(f"/bin/{dll}") and "Library/" in path
            )
            put(source, dest / "bin" / dll)

    for package, texts in LICENSES.items():
        for source, target in texts.items():
            if source in files[package]:
                (dest / "licenses" / target).write_bytes(files[package][source])
    for text in EXTRA_LICENSES:
        shutil.copy(root / "installer" / "ccx" / text, dest / "licenses" / text)
    shutil.copy(root / "LICENSE", dest / "licenses" / "GPL-3.0.txt")
    (dest / "SOURCE.txt").write_text(
        f"CalculiX ccx {VERSION}, conda-forge build for {subdir}\n"
        f"Sources: http://www.dhondt.de/ccx_{VERSION}.src.tar.bz2\n"
        f"Patches and build recipe: {FEEDSTOCK}\n"
        "Packages:\n" + "".join(f"  {CHANNEL}/{subdir}/{name}\n" for name in packages)
    )
    print(f"CalculiX {VERSION} liegt in {dest}")


if __name__ == "__main__":
    main()
