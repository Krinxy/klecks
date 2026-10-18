# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-18
# ---
"""Takes the floaters out of a trained splat.

A Gaussian splat grows blobs in mid-air where the cameras disagreed: nearly invisible haze, huge faint
discs, isolated specks far from anything. They are what makes a room look smoky from the side. This removes
the splats that no wall, floor or piece of furniture owns:

  faint       opacity below FAINT
  haze        large and not clearly opaque
  stray       outside the room, or in a space where almost no other splat is
  runaway     a single splat much larger than the room's typical ones
  grain       tiny and not clearly opaque: the speckle on smooth walls

Plain numpy on the CPU (no ML). The file keeps its format, only rows are dropped.

    python clean_splat.py <splat.ply> [<out.ply>]
"""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

FAINT = 0.04              # opacity (after the sigmoid) below which a splat is invisible anyway
HAZE_OPACITY = 0.35       # larger than HAZE_SIZE x the typical splat and fainter than this: haze
HAZE_SIZE = 6.0
GRAIN_SIZE = 0.25         # smaller than this x the typical splat and not clearly opaque: grain, the speckle seen on flat walls
GRAIN_OPACITY = 0.30
RUNAWAY_SIZE = 40.0       # larger than this x the typical splat, whatever its opacity
ROOM_PERCENTILES = (0.5, 99.5)
ROOM_MARGIN = 0.15        # of the room's extent: splats further out are stray
VOXELS = 160              # grid cells along the longest side for the density check
MIN_NEIGHBOURS = 4        # a splat in a cell with fewer splats than this (and its neighbours) is an isolated speck


def read(path: Path) -> tuple[list[str], np.ndarray, bytes]:
    with open(path, "rb") as handle:
        header = b""
        while not header.endswith(b"end_header\n"):
            header += handle.readline()
        names, count = [], 0
        for line in header.decode("ascii").splitlines():
            if line.startswith("element vertex"):
                count = int(line.split()[-1])
            if line.startswith("property float "):
                names.append(line.split()[-1])
        data = np.frombuffer(handle.read(count * 4 * len(names)), dtype="<f4").reshape(count, len(names))
    return names, data, header


def keep_mask(names: list[str], data: np.ndarray) -> tuple[np.ndarray, dict[str, int]]:
    column = {name: index for index, name in enumerate(names)}
    xyz = data[:, [column["x"], column["y"], column["z"]]]
    opacity = 1.0 / (1.0 + np.exp(-data[:, column["opacity"]]))
    size = np.exp(data[:, [column["scale_0"], column["scale_1"], column["scale_2"]]]).max(axis=1)
    typical = float(np.median(size))

    reasons: dict[str, int] = {}
    keep = np.ones(len(data), dtype=bool)

    def drop(name: str, mask: np.ndarray) -> None:
        nonlocal keep
        gone = keep & mask
        reasons[name] = int(gone.sum())
        keep &= ~mask

    drop("faint", opacity < FAINT)
    drop("haze", (size > HAZE_SIZE * typical) & (opacity < HAZE_OPACITY))
    drop("runaway", size > RUNAWAY_SIZE * typical)
    drop("grain", (size < GRAIN_SIZE * typical) & (opacity < GRAIN_OPACITY))

    low, high = np.percentile(xyz[keep], ROOM_PERCENTILES, axis=0)
    margin = (high - low) * ROOM_MARGIN
    outside = np.any((xyz < low - margin) | (xyz > high + margin), axis=1)
    drop("outside the room", outside)

    # density: how many splats share a cell and the cells around it
    inside = xyz[keep]
    lo, hi = inside.min(axis=0), inside.max(axis=0)
    cell = max(float((hi - lo).max()) / VOXELS, 1e-9)
    index = np.floor((xyz - lo) / cell).astype(np.int64)
    index = np.clip(index, 0, VOXELS)
    flat = (index[:, 0] * (VOXELS + 1) + index[:, 1]) * (VOXELS + 1) + index[:, 2]
    counts = np.bincount(flat[keep], minlength=(VOXELS + 1) ** 3).reshape(VOXELS + 1, VOXELS + 1, VOXELS + 1)
    padded = np.pad(counts, 1)
    around = sum(padded[1 + dx:1 + dx + counts.shape[0], 1 + dy:1 + dy + counts.shape[1], 1 + dz:1 + dz + counts.shape[2]]
                 for dx in (-1, 0, 1) for dy in (-1, 0, 1) for dz in (-1, 0, 1))
    neighbours = around[index[:, 0], index[:, 1], index[:, 2]]
    drop("isolated", neighbours < MIN_NEIGHBOURS)
    return keep, reasons


def clean(source: Path, target: Path) -> dict:
    names, data, header = read(source)
    keep, reasons = keep_mask(names, data)
    kept = np.ascontiguousarray(data[keep])
    new_header = header.decode("ascii").replace(f"element vertex {len(data)}", f"element vertex {len(kept)}").encode("ascii")
    with open(target, "wb") as handle:
        handle.write(new_header)
        handle.write(kept.astype("<f4").tobytes())
    return {"before": int(len(data)), "after": int(len(kept)), "removed": reasons}


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(__doc__)
        return 2
    source = Path(sys.argv[1])
    target = Path(sys.argv[2]) if len(sys.argv) == 3 else source
    if target == source:
        # never replace the only copy before the new one is complete
        temporary = source.with_suffix(".clean.ply")
        report = clean(source, temporary)
        temporary.replace(source)
    else:
        report = clean(source, target)
    print(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
