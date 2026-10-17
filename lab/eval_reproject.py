# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-17
# ---
"""Scores a scan by projecting its point cloud back into the real cameras.

For a handful of the frames the scan was built from, the cloud is drawn from that frame's camera
and compared with the real picture (reduced to the model's working size):

  coverage   share of pixels that received a point
  error      mean absolute colour difference (0-255) on covered pixels; lower = sharper, better aligned
  error@half the same at half size, where point speckle averages out and only real misalignment is left

A ghosting or smeared cloud has high error. This is a self-consistency check (the frames were used
to build the cloud), good for comparing variants, not an absolute truth. Use --save to look at it.

    python lab/eval_reproject.py D:/Projects/klecks-runs/single7695 [--points points.ply]
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import cv2
import numpy as np


def read_ply(path: Path) -> tuple[np.ndarray, np.ndarray]:
    with open(path, "rb") as handle:
        count = 0
        for line in iter(handle.readline, b"end_header\n"):
            if line.startswith(b"element vertex"):
                count = int(line.split()[-1])
        record = np.dtype([("p", "<f4", 3), ("c", "u1", 3)])
        data = np.frombuffer(handle.read(), dtype=record, count=count)
    return data["p"].astype(np.float32), data["c"]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("run", type=Path)
    parser.add_argument("--points", default="points.ply")
    parser.add_argument("--views", type=int, default=12)
    parser.add_argument("--save", type=Path, default=None, help="write real|render comparison images here")
    args = parser.parse_args()
    sheets: list[np.ndarray] = []

    meta = json.loads((args.run / "frames.json").read_text()) if (args.run / "frames.json").exists() else None
    cameras = np.load(args.run / "cameras.npz")
    extrinsic, intrinsic = cameras["extrinsic"], cameras["intrinsic"]
    frames = [Path(p) for p in meta["paths"]] if meta else sorted((args.run / "frames").glob("*.jpg"))
    height, width = (meta["height"], meta["width"]) if meta else (686, 392)

    points, colors = read_ply(args.run / args.points)
    picks = np.linspace(0, len(extrinsic) - 1, min(args.views, len(extrinsic)), dtype=int)
    rows = []
    for i in picks:
        R, t, K = extrinsic[i, :, :3], extrinsic[i, :, 3], intrinsic[i]
        cam = points @ R.T + t
        z = cam[:, 2]
        front = z > 0.01
        u = np.round(cam[front, 0] / z[front] * K[0, 0] + K[0, 2]).astype(int)
        v = np.round(cam[front, 1] / z[front] * K[1, 1] + K[1, 2]).astype(int)
        inside = (u >= 0) & (u < width) & (v >= 0) & (v < height)
        u, v, zz, cc = u[inside], v[inside], z[front][inside], colors[front][inside]
        order = np.argsort(-zz)                         # far first, so the nearest point is written last
        render = np.zeros((height, width, 3), np.uint8)
        covered = np.zeros((height, width), bool)
        render[v[order], u[order]] = cc[order]
        covered[v[order], u[order]] = True
        # fill single-pixel gaps between neighbouring points so coverage isn't punished for point spacing
        closed = cv2.morphologyEx(covered.astype(np.uint8), cv2.MORPH_CLOSE, np.ones((3, 3), np.uint8)).astype(bool)
        filled = cv2.dilate(render, np.ones((3, 3), np.uint8))
        render = np.where(covered[..., None], render, filled)

        real = cv2.cvtColor(cv2.imread(str(frames[i])), cv2.COLOR_BGR2RGB)
        real = cv2.resize(real, (width, height), interpolation=cv2.INTER_AREA)
        diff = np.abs(render.astype(np.int16) - real.astype(np.int16)).mean(axis=2)
        # the same comparison at half size: point speckle averages out, real misalignment stays
        small_r = cv2.resize(render, (width // 2, height // 2), interpolation=cv2.INTER_AREA)
        small_t = cv2.resize(real, (width // 2, height // 2), interpolation=cv2.INTER_AREA)
        diff_small = np.abs(small_r.astype(np.int16) - small_t.astype(np.int16)).mean()
        rows.append((closed.mean(), diff[closed].mean() if closed.any() else 255.0, diff_small))
        if args.save is not None:
            sheets.append(np.hstack([real, render]))
    coverage, error, error_small = (float(np.mean([r[k] for r in rows])) for k in range(3))
    print(f"{args.run.name}: coverage {coverage * 100:.0f}%  error {error:.1f}  error@half {error_small:.1f}"
          f"   ({len(points):,} points, {len(picks)} views)")
    if args.save is not None and sheets:
        args.save.parent.mkdir(parents=True, exist_ok=True)
        cv2.imwrite(str(args.save), cv2.cvtColor(np.hstack(sheets[:: max(1, len(sheets) // 3)][:3]), cv2.COLOR_RGB2BGR))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
