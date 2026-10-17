# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-17
# ---
"""Writes a finished room_scan run as a COLMAP text model that Gaussian-splat trainers (Brush) read.

    <run>/colmap/images/*.jpg           the frames (optionally super-resolved)
    <run>/colmap/sparse/0/cameras.txt   one pinhole camera per frame, intrinsics scaled to the image size
    <run>/colmap/sparse/0/images.txt    world->camera poses
    <run>/colmap/sparse/0/points3D.txt  the fused cloud, thinned, as the starting point for training

VGGT poses are OpenCV world->camera ([R|t], x right, y down, z forward), the same convention COLMAP uses.

    python lab/export_colmap.py D:/Projects/klecks-runs/room1 [--upscale-model PATH] [--points 300000]
"""
from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

import cv2
import numpy as np
import torch

sys.path.insert(0, str(Path(__file__).parent))
from room_scan import Upscaler, read_ply_points, voxel_limit  # noqa: E402


def quaternion(rotation: np.ndarray) -> tuple[float, float, float, float]:
    """3x3 rotation -> (w, x, y, z)."""
    m = rotation
    trace = m[0, 0] + m[1, 1] + m[2, 2]
    if trace > 0:
        s = 0.5 / np.sqrt(trace + 1.0)
        return 0.25 / s, (m[2, 1] - m[1, 2]) * s, (m[0, 2] - m[2, 0]) * s, (m[1, 0] - m[0, 1]) * s
    if m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = 2.0 * np.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2])
        return (m[2, 1] - m[1, 2]) / s, 0.25 * s, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s
    if m[1, 1] > m[2, 2]:
        s = 2.0 * np.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2])
        return (m[0, 2] - m[2, 0]) / s, (m[0, 1] + m[1, 0]) / s, 0.25 * s, (m[1, 2] + m[2, 1]) / s
    s = 2.0 * np.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1])
    return (m[1, 0] - m[0, 1]) / s, (m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, 0.25 * s


def export_dataset(run: Path, points_count: int = 300_000, upscale_model: Path | None = None,
                   upscale_factor: float = 2.0, on_frame=None) -> Path:
    """Writes <run>/colmap from a finished room_scan run; returns the dataset folder."""
    meta = json.loads((run / "frames.json").read_text())
    cameras = np.load(run / "cameras.npz")
    extrinsic, intrinsic = cameras["extrinsic"], cameras["intrinsic"]
    frames = [Path(p) for p in meta["paths"]]
    model_h, model_w = meta["height"], meta["width"]

    root = run / "colmap"
    if root.exists():
        shutil.rmtree(root)
    (root / "images").mkdir(parents=True)
    (root / "sparse" / "0").mkdir(parents=True)

    upscaler = Upscaler(upscale_model) if upscale_model else None
    camera_lines, image_lines = [], []
    for index, frame in enumerate(frames):
        if on_frame is not None:
            on_frame(index, len(frames))
        image = cv2.imread(str(frame))
        if upscaler is not None:
            rgb = torch.from_numpy(cv2.cvtColor(image, cv2.COLOR_BGR2RGB)).cuda().permute(2, 0, 1)[None].float() / 255
            big = upscaler(rgb)
            h, w = (int(image.shape[0] * upscale_factor), int(image.shape[1] * upscale_factor))
            big = torch.nn.functional.interpolate(big, size=(h, w), mode="area")[0].permute(1, 2, 0)
            image = cv2.cvtColor((big * 255).clamp(0, 255).byte().cpu().numpy(), cv2.COLOR_RGB2BGR)
        height, width = image.shape[:2]
        name = f"{index:04d}.jpg"
        cv2.imwrite(str(root / "images" / name), image, [cv2.IMWRITE_JPEG_QUALITY, 97])

        K = intrinsic[index]
        fx, fy = K[0, 0] * width / model_w, K[1, 1] * height / model_h
        cx, cy = K[0, 2] * width / model_w, K[1, 2] * height / model_h
        camera_lines.append(f"{index + 1} PINHOLE {width} {height} {fx:.6f} {fy:.6f} {cx:.6f} {cy:.6f}")
        qw, qx, qy, qz = quaternion(extrinsic[index, :, :3])
        tx, ty, tz = extrinsic[index, :, 3]
        image_lines.append(f"{index + 1} {qw:.9f} {qx:.9f} {qy:.9f} {qz:.9f} {tx:.9f} {ty:.9f} {tz:.9f} {index + 1} {name}")
        image_lines.append("")  # no 2D points

    points, colors = read_ply_points(run / "points.ply")
    points, colors = voxel_limit(points, colors, points_count)
    with open(root / "sparse" / "0" / "points3D.txt", "w") as handle:
        handle.write("# POINT3D_ID X Y Z R G B ERROR TRACK[]\n")
        for n, (p, c) in enumerate(zip(points, colors), start=1):
            handle.write(f"{n} {p[0]:.5f} {p[1]:.5f} {p[2]:.5f} {c[0]} {c[1]} {c[2]} 0\n")
    (root / "sparse" / "0" / "cameras.txt").write_text(
        "# CAMERA_ID MODEL WIDTH HEIGHT PARAMS[]\n" + "\n".join(camera_lines) + "\n")
    (root / "sparse" / "0" / "images.txt").write_text(
        "# IMAGE_ID QW QX QY QZ TX TY TZ CAMERA_ID NAME\n" + "\n".join(image_lines) + "\n")
    print(f"{root}: {len(frames)} images ({width}x{height}), {len(points):,} start points")
    return root


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("run", type=Path)
    parser.add_argument("--points", type=int, default=300_000, help="size of the starting cloud")
    parser.add_argument("--upscale-model", type=Path, default=None,
                        help="super-resolve the training images with this model")
    parser.add_argument("--upscale-factor", type=float, default=2.0,
                        help="final size relative to the source frames when upscaling (model output is resized)")
    args = parser.parse_args()
    export_dataset(args.run, args.points, args.upscale_model, args.upscale_factor)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
