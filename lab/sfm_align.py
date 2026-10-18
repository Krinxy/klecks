# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-18
# ---
"""Puts COLMAP's cameras and VGGT's depth together.

VGGT's depth of a single frame is very good, but it comes with an unknown scale, and that scale differs by up to
a quarter from window to window; chaining the windows therefore smears the room. COLMAP places every camera of the
room together and gets them to agree to about a pixel. So the cameras come from COLMAP and each frame's VGGT depth
is scaled to fit the sparse points COLMAP saw in that frame. The cloud, the floor plan and the doors are then built
from those depths in COLMAP's coordinates, the same coordinates Brush trains the splat in.
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).parent))

MIN_REGISTERED = 0.6     # share of the frames COLMAP must have placed, else the VGGT cameras are kept
MIN_OBSERVATIONS = 12    # sparse points a frame needs to have its depth scaled
MAX_SPREAD = 0.25        # depth and sparse points must agree within this (median deviation / median) inside a frame
EDGE_LIMIT = 0.08        # depth jumps (relative to the depth) above this are object edges, where VGGT depth is unreliable
START_POINTS = 300_000   # dense points the training starts from (on top of COLMAP's own sparse ones)
SAMPLE_STEP = 2          # of the stored depth pixels, every Nth one in both directions goes into the cloud


def quaternion_to_matrix(q: tuple[float, float, float, float]) -> np.ndarray:
    w, x, y, z = q
    return np.array([
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ])


def read_model(directory: Path) -> tuple[dict, dict]:
    """A COLMAP text model: images by name -> (rotation, translation, observations [x, y, point id]) and points by id."""
    points = {}
    for line in (directory / "points3D.txt").read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#"):
            parts = line.split()
            points[int(parts[0])] = np.array([float(parts[1]), float(parts[2]), float(parts[3])])
    rows = [line for line in (directory / "images.txt").read_text(encoding="utf-8").splitlines() if not line.startswith("#")]
    images = {}
    for head, observed in zip(rows[0::2], rows[1::2]):
        parts = head.split()
        if len(parts) < 10:
            continue
        rotation = quaternion_to_matrix(tuple(float(v) for v in parts[1:5]))
        translation = np.array([float(v) for v in parts[5:8]])
        seen = np.array(observed.split(), dtype=float).reshape(-1, 3) if observed.strip() else np.zeros((0, 3))
        images[parts[9]] = (rotation, translation, seen)
    return images, points


def frame_key(path: Path) -> str:
    return f"{path.name.split('_')[0]}/{path.name}"


def depth_scale(depth: np.ndarray, box: list[int], size: tuple[int, int], stride: int, model: tuple, points: dict) -> tuple[float, float, int] | None:
    """How many COLMAP units one unit of this frame's VGGT depth is: the median over the sparse points seen in the
    frame. Returns (scale, spread, count) or None when too few points are seen to say."""
    rotation, translation, seen = model
    bx, by, bw, bh = box
    width, height = size
    ratios = []
    for u, v, point_id in seen:
        point = points.get(int(point_id))
        if point is None:
            continue
        z = float((rotation @ point + translation)[2])
        if z <= 0:
            continue
        x, y = int((bx + u * bw / width) / stride), int((by + v * bh / height) / stride)
        if 0 <= y < depth.shape[0] and 0 <= x < depth.shape[1] and depth[y, x] > 1e-4:
            ratios.append(z / float(depth[y, x]))
    if len(ratios) < MIN_OBSERVATIONS:
        return None
    ratios = np.array(ratios)
    median = float(np.median(ratios))
    return median, float(np.median(np.abs(ratios - median)) / median), len(ratios)


def place_cameras(scan: Path, kept: dict, stride: int, progress, reuse: bool = False) -> dict | None:
    """Runs COLMAP on the frames of the scan and fits VGGT's depth to it; None when that did not work out.

    Returns {keep: indices of the frames COLMAP placed, extrinsic: their COLMAP poses, scale: depth scale per kept
    frame (0 where the depth does not fit), stats}."""
    from colmap_poses import solve

    stats = solve(scan, reuse=reuse)
    frames = len(kept["paths"])
    if stats.get("registered", 0) < MIN_REGISTERED * frames:
        print(f"COLMAP placed only {stats.get('registered', 0)} of {frames} frames: keeping the VGGT cameras", file=sys.stderr)
        return None
    images, points = read_model(scan / "sfm" / "model_txt")
    keep, extrinsic, scales, spreads = [], [], [], []
    for index, path in enumerate(kept["paths"]):
        model = images.get(frame_key(path))
        if model is None:
            continue
        keep.append(index)
        extrinsic.append(np.concatenate([model[0], model[1][:, None]], axis=1))
        found = depth_scale(kept["depth"][index].astype(np.float32), kept["boxes"][index], Image.open(path).size, stride, model, points)
        if found is not None and found[1] <= MAX_SPREAD:
            scales.append(found[0])
            spreads.append(found[1])
        else:
            scales.append(0.0)
    usable = int(sum(1 for s in scales if s > 0))
    if usable < 8:
        print("The depth of too few frames fits COLMAP's points: keeping the VGGT cameras", file=sys.stderr)
        return None
    stats.update({
        "frames_with_depth": usable,
        "depth_scale_spread_pct": round(100 * float(np.median(spreads)), 1),
        "depth_scale_range": [round(float(np.percentile([s for s in scales if s > 0], q)), 2) for q in (10, 50, 90)],
    })
    return {"keep": keep, "extrinsic": np.stack(extrinsic), "scale": np.array(scales), "stats": stats}


def cloud_from_depths(kept: dict, stride: int, confidence_percentile: float) -> tuple[np.ndarray, np.ndarray, float]:
    """The room as a coloured cloud from every frame's scaled depth (0 where it does not fit), in COLMAP's coordinates.
    Returns (points, colors, median depth)."""
    sample = np.concatenate([c[::7, ::7].ravel() for c in kept["conf"]]) if kept.get("conf") else np.zeros(1)
    cut = float(np.percentile(sample, confidence_percentile))
    clouds, colours, depths = [], [], []
    for index, path in enumerate(kept["paths"]):
        depth = kept["depth"][index].astype(np.float32)
        if not depth.any():
            continue
        confidence = kept["conf"][index].astype(np.float32) if kept.get("conf") else np.full_like(depth, cut + 1.0)
        height, width = depth.shape
        # object edges: depth jumps relative to the depth itself
        jump = (np.abs(cv2.Sobel(depth, cv2.CV_32F, 1, 0, ksize=3)) + np.abs(cv2.Sobel(depth, cv2.CV_32F, 0, 1, ksize=3))) / np.maximum(depth, 1e-6)
        ok = (depth > 1e-4) & (confidence >= cut) & (jump < EDGE_LIMIT * 4)
        ys, xs = np.mgrid[0:height:SAMPLE_STEP, 0:width:SAMPLE_STEP]
        ys, xs = ys.ravel(), xs.ravel()
        ok = ok[ys, xs]
        ys, xs = ys[ok], xs[ok]
        if not len(xs):
            continue
        z = depth[ys, xs]
        bx, by, bw, bh = kept["boxes"][index]
        u, v = xs * stride, ys * stride
        inside = (u >= bx) & (u < bx + bw) & (v >= by) & (v < by + bh)
        u, v, z = u[inside], v[inside], z[inside]
        if not len(u):
            continue
        k = kept["intrinsic"][index]
        camera = np.stack([(u - k[0, 2]) / k[0, 0] * z, (v - k[1, 2]) / k[1, 1] * z, z], axis=1)
        rotation, translation = kept["extrinsic"][index][:, :3], kept["extrinsic"][index][:, 3]
        clouds.append((camera - translation) @ rotation)  # R^T (X - t)
        picture = cv2.cvtColor(cv2.imread(str(path)), cv2.COLOR_BGR2RGB)
        frame_h, frame_w = picture.shape[:2]
        fu = np.clip(((u - bx) * frame_w / bw).astype(int), 0, frame_w - 1)
        fv = np.clip(((v - by) * frame_h / bh).astype(int), 0, frame_h - 1)
        colours.append(picture[fv, fu])
        depths.append(z[::50])
    if not clouds:
        raise ValueError("no depth to build a cloud from")
    return np.concatenate(clouds).astype(np.float32), np.concatenate(colours), float(np.median(np.concatenate(depths)))


def install_training_set(scan: Path, points: np.ndarray, colors: np.ndarray) -> Path:
    """Makes <scan>/colmap the dataset Brush trains on: COLMAP's undistorted images and cameras, and a denser start."""
    from room_scan import voxel_limit

    train = scan / "sfm" / "train"
    target = scan / "colmap"
    if target.exists():
        shutil.rmtree(target)
    shutil.move(str(train), str(target))
    sparse = target / "sparse" / "0" / "points3D.txt"
    thin_points, thin_colors = voxel_limit(points, colors, START_POINTS)
    own = [line for line in sparse.read_text(encoding="utf-8").splitlines() if line and not line.startswith("#")]
    with open(sparse, "w", encoding="utf-8") as handle:
        handle.write("# POINT3D_ID X Y Z R G B ERROR TRACK[]\n")
        number = 0
        for line in own:
            parts = line.split()
            number += 1
            handle.write(f"{number} {parts[1]} {parts[2]} {parts[3]} {parts[4]} {parts[5]} {parts[6]} 0\n")
        for p, c in zip(thin_points, thin_colors):
            number += 1
            handle.write(f"{number} {p[0]:.5f} {p[1]:.5f} {p[2]:.5f} {c[0]} {c[1]} {c[2]} 0\n")
    return target
