# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-18
# ---
"""Top-down plan of a scanned room: where the walls and furniture are seen, and where the cameras walked.

Reads cameras.npz, frames.json and scene.ply of a scan folder and writes plan.json next to them.
Plain numpy, no ML. room_scan.py calls it at the end of a scan; run it by hand for older scans:

    python room_plan.py <scan dir>
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import cv2
import numpy as np

GRID = 96            # cells along the longer side of the plan
TRIM = (1.0, 99.0)   # percentiles that cut stray points away from the plan's bounds
MAX_POINTS = 1_500_000
RANGE = 0.35         # how far a camera is credited with seeing, as a share of the plan's longer side
THIN_FRAMES = 4      # a cell seen by fewer frames than this counts as thin
MIN_FLAT = 0.25      # a camera looking mostly up or down says nothing about the plan


def read_points(path: Path) -> np.ndarray:
    """XYZ of a binary little-endian PLY written by room_scan.write_ply."""
    with open(path, "rb") as handle:
        count = 0
        while True:
            line = handle.readline().decode("ascii").strip()
            if line.startswith("element vertex"):
                count = int(line.split()[-1])
            if line == "end_header":
                break
        record = np.dtype([("p", "<f4", 3), ("c", "u1", 3)])
        data = np.frombuffer(handle.read(count * record.itemsize), dtype=record)
    return data["p"].astype(np.float64)


def build(scan: Path) -> dict:
    extrinsic = np.load(scan / "cameras.npz")["extrinsic"]
    frames = json.loads((scan / "frames.json").read_text())
    video_of = [int(Path(p).name.split("_")[0]) for p in frames["paths"]]

    rotation, translation = extrinsic[:, :, :3], extrinsic[:, :, 3]
    centers = -np.einsum("sji,sj->si", rotation, translation)
    forward = rotation[:, 2, :]                         # camera z axis in world coordinates
    up = -rotation[:, 1, :].mean(axis=0)                # camera y points down
    up /= np.linalg.norm(up)

    # Plane basis perpendicular to up; the first axis follows the direction the cameras spread most.
    helper = np.array([1.0, 0, 0]) if abs(up[0]) < 0.9 else np.array([0, 1.0, 0])
    a = np.cross(up, helper); a /= np.linalg.norm(a)
    b = np.cross(up, a)
    flat = centers @ np.stack([a, b], axis=1)
    if len(flat) > 2:
        _, _, vt = np.linalg.svd(flat - flat.mean(axis=0))
        turn = np.array([[vt[0][0], vt[0][1]], [-vt[0][1], vt[0][0]]])
    else:
        turn = np.eye(2)
    basis = np.stack([a, b], axis=1) @ turn.T

    points = read_points(scan / "scene.ply")
    if len(points) > MAX_POINTS:
        points = points[np.random.default_rng(0).choice(len(points), MAX_POINTS, replace=False)]
    ground = points @ basis
    spot = centers @ basis
    low = np.percentile(ground, TRIM[0], axis=0)
    high = np.percentile(ground, TRIM[1], axis=0)
    low = np.minimum(low, spot.min(axis=0)); high = np.maximum(high, spot.max(axis=0))
    size = np.maximum(high - low, 1e-6)

    scale = GRID / size.max()
    width, height = max(1, int(np.ceil(size[0] * scale))), max(1, int(np.ceil(size[1] * scale)))
    cell = np.floor((ground - low) * scale).astype(int)
    inside = (cell[:, 0] >= 0) & (cell[:, 0] < width) & (cell[:, 1] >= 0) & (cell[:, 1] < height)
    counts = np.zeros((height, width))
    np.add.at(counts, (cell[inside, 1], cell[inside, 0]), 1)
    density = np.log1p(counts)
    density = (255 * density / max(density.max(), 1e-9)).astype(int)

    heading = forward @ basis
    flat = np.linalg.norm(heading, axis=1)
    heading /= np.maximum(flat[:, None], 1e-9)
    cameras = [
        [round(float((spot[i, 0] - low[0]) * scale), 2), round(float((spot[i, 1] - low[1]) * scale), 2),
         video_of[i], round(float(heading[i, 0]), 3), round(float(heading[i, 1]), 3)]
        for i in range(len(spot))
    ]
    intrinsic = np.load(scan / "cameras.npz")["intrinsic"]
    score = coverage(np.array([[c[0], c[1]] for c in cameras]), heading, flat, intrinsic, video_of, density, scale, size)
    return {"width": width, "height": height, "cells": density.tolist(), "cameras": cameras, **score}


def coverage(spots: np.ndarray, heading: np.ndarray, flat: np.ndarray, intrinsic: np.ndarray,
             video_of: list[int], density: np.ndarray, scale: float, size: np.ndarray) -> dict:
    """Which film saw which part of the footprint.

    A cell counts as seen by a frame when it lies in front of the camera, inside its field of view and within
    RANGE of the plan. Walls that hide things are ignored, so the numbers are an estimate, good for comparing
    films with each other and for finding the thin parts, not a measurement."""
    ys, xs = np.nonzero(density)
    centers = np.stack([xs + 0.5, ys + 0.5], axis=1)
    reach = RANGE * max(density.shape)
    videos = max(video_of) + 1
    seen_by = np.zeros((videos, len(centers)), dtype=bool)
    frames = np.zeros(len(centers), dtype=int)
    per_video_frames = [0] * videos
    for i, spot in enumerate(spots):
        per_video_frames[video_of[i]] += 1
        if flat[i] < MIN_FLAT:
            continue
        half = np.arctan(intrinsic[i][0][2] / intrinsic[i][0][0])
        offset = centers - spot
        distance = np.linalg.norm(offset, axis=1)
        cosine = (offset @ heading[i]) / np.maximum(distance, 1e-9)
        visible = (distance >= 1) & (distance <= reach) & (cosine >= np.cos(half))
        seen_by[video_of[i]] |= visible
        frames += visible
    total = max(len(centers), 1)
    once = seen_by.sum(axis=0) == 1
    result = {
        "videos": [
            {"frames": per_video_frames[v], "seen": int(seen_by[v].sum()),
             "unique": int((seen_by[v] & once).sum())}
            for v in range(videos)
        ],
        "footprint": int(len(centers)),
        "covered_pct": int(round(100 * (frames >= THIN_FRAMES).sum() / total)),
    }
    grid = np.zeros_like(density)
    grid[ys, xs] = np.minimum(frames, 9)
    result["seen"] = grid.tolist()

    thin = np.zeros(density.shape, dtype=np.uint8)
    thin[ys[frames < THIN_FRAMES], xs[frames < THIN_FRAMES]] = 1
    count, labels, stats, middles = cv2.connectedComponentsWithStats(thin, connectivity=8)
    result["thin"] = None
    if count > 1:
        biggest = 1 + int(np.argmax(stats[1:, cv2.CC_STAT_AREA]))
        area = int(stats[biggest, cv2.CC_STAT_AREA])
        if area >= max(8, 0.01 * total):
            result["thin"] = {
                "cells": area,
                "share_pct": int(round(100 * area / total)),
                "x": round(float(middles[biggest][0]) / density.shape[1], 3),
                "y": round(float(middles[biggest][1]) / density.shape[0], 3),
            }
    return result


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    scan = Path(sys.argv[1])
    (scan / "plan.json").write_text(json.dumps(build(scan)))
    print(f"wrote {scan / 'plan.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
