# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-17
# ---
"""Renders a point cloud (.ply from vggt_try.py) from novel viewpoints into one PNG.

The first camera sits at the origin looking along +z (VGGT's world frame). Views away from
that camera show whether the estimated depth is plausible. Pure numpy + OpenCV, CPU-only
because it is just plotting, not ML.

    python lab/render_points.py D:/Projects/klecks-runs/img7695
"""
from __future__ import annotations

import sys
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


def look_at(eye: np.ndarray, target: np.ndarray, up: np.ndarray) -> np.ndarray:
    forward = target - eye
    forward /= np.linalg.norm(forward)
    right = np.cross(forward, up)
    right /= np.linalg.norm(right)
    down = np.cross(forward, right)
    return np.stack([right, down, forward])  # rows: camera axes in world


def render(points, colors, eye, target, up, size=(640, 400), focal=420.0) -> np.ndarray:
    width, height = size
    axes = look_at(np.array(eye, float), np.array(target, float), np.array(up, float))
    cam = (points - np.array(eye, np.float32)) @ axes.T.astype(np.float32)
    front = cam[:, 2] > 0.05
    cam, col = cam[front], colors[front]
    u = (cam[:, 0] / cam[:, 2] * focal + width / 2).astype(int)
    v = (cam[:, 1] / cam[:, 2] * focal + height / 2).astype(int)
    inside = (u >= 0) & (u < width) & (v >= 0) & (v < height)
    u, v, z, col = u[inside], v[inside], cam[inside, 2], col[inside]
    order = np.argsort(-z)  # far first, near points overwrite
    image = np.full((height, width, 3), 10, np.uint8)
    image[v[order], u[order]] = col[order][:, ::-1]  # RGB -> BGR for OpenCV
    return cv2.dilate(image, np.ones((2, 2), np.uint8))


def main() -> int:
    run = Path(sys.argv[1])
    points, colors = read_ply(run / "points.ply")
    keep = np.random.default_rng(0).choice(len(points), size=min(2_000_000, len(points)), replace=False)
    points, colors = points[keep], colors[keep]
    centre = np.median(points, axis=0)
    spread = float(np.linalg.norm(points - centre, axis=1).mean())
    up = (0.0, -1.0, 0.0)  # VGGT: y points down

    views = {
        "original camera": ((0, 0, 0), (0, 0, 1)),
        "0.4 to the right": ((0.4 * spread, 0, 0), tuple(centre)),
        "0.4 to the left": ((-0.4 * spread, 0, 0), tuple(centre)),
        "from above": ((centre[0], centre[1] - 2.2 * spread, centre[2] - 0.2 * spread), tuple(centre)),
    }
    tiles = []
    for name, (eye, target) in views.items():
        tile = render(points, colors, eye, target, up)
        cv2.putText(tile, name, (10, 24), cv2.FONT_HERSHEY_SIMPLEX, 0.7, (255, 255, 255), 2, cv2.LINE_AA)
        tiles.append(tile)
    sheet = np.vstack([np.hstack(tiles[:2]), np.hstack(tiles[2:])])
    out = run / "views.png"
    cv2.imwrite(str(out), sheet)
    print(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
