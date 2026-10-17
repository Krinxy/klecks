# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-17
# ---
"""video -> VGGT camera poses + depth -> coloured point cloud.

Used by the app as a *temporary* backend (see core/reconstruct); the plan is to replace it with
an ONNX model run from Rust. GPU only: exits with code 2 if CUDA is missing, klecks never runs
ML on the CPU.

Progress is printed as JSON lines prefixed with ``@@`` so the app can parse them:
    @@{"stage": "inference", "pct": 72, "message": "Estimating depth"}

    python lab/vggt_try.py media/IMG_7695.MP4 --out D:/Projects/klecks-runs/img7695
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

import cv2
import numpy as np
import torch

from vggt.models.vggt import VGGT
from vggt.utils.geometry import unproject_depth_map_to_point_map
from vggt.utils.load_fn import load_and_preprocess_images
from vggt.utils.pose_enc import pose_encoding_to_extri_intri


def progress(stage: str, pct: int, message: str) -> None:
    print("@@" + json.dumps({"stage": stage, "pct": pct, "message": message}), flush=True)


def display_rotation(video: Path) -> int | None:
    """Phones store portrait video sideways plus a rotation tag; OpenCV ignores the tag."""
    result = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0",
         "-show_entries", "stream_side_data=rotation", "-of", "csv=p=0", str(video)],
        capture_output=True, text=True,
    )
    text = result.stdout.strip().split("\n")[0].strip()
    degrees = int(float(text)) if text else 0
    # ffprobe reports the counter-clockwise rotation the display matrix applies.
    return {-90: cv2.ROTATE_90_CLOCKWISE, 90: cv2.ROTATE_90_COUNTERCLOCKWISE,
            180: cv2.ROTATE_180, -180: cv2.ROTATE_180}.get(degrees)


def extract_frames(video: Path, out_dir: Path, count: int) -> list[Path]:
    """Evenly spaced, upright frames across the whole clip."""
    out_dir.mkdir(parents=True, exist_ok=True)
    rotation = display_rotation(video)
    cap = cv2.VideoCapture(str(video))
    cap.set(cv2.CAP_PROP_ORIENTATION_AUTO, 0)  # we rotate ourselves, once and explicitly
    total = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
    picks = np.linspace(0, total - 1, num=min(count, total), dtype=int)
    paths: list[Path] = []
    for n, index in enumerate(picks):
        cap.set(cv2.CAP_PROP_POS_FRAMES, int(index))
        ok, frame = cap.read()
        if not ok:
            continue
        if rotation is not None:
            frame = cv2.rotate(frame, rotation)
        path = out_dir / f"{n:04d}.jpg"
        cv2.imwrite(str(path), frame, [cv2.IMWRITE_JPEG_QUALITY, 95])
        paths.append(path)
    cap.release()
    return paths


def write_ply(path: Path, points: np.ndarray, colors: np.ndarray) -> None:
    header = (
        "ply\nformat binary_little_endian 1.0\n"
        f"element vertex {len(points)}\n"
        "property float x\nproperty float y\nproperty float z\n"
        "property uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n"
    )
    record = np.dtype([("p", "<f4", 3), ("c", "u1", 3)])
    data = np.empty(len(points), dtype=record)
    data["p"] = points.astype("<f4")
    data["c"] = colors.astype("u1")
    with open(path, "wb") as handle:
        handle.write(header.encode("ascii"))
        handle.write(data.tobytes())


def voxel_downsample(points: np.ndarray, colors: np.ndarray, limit: int) -> tuple[np.ndarray, np.ndarray]:
    """Keeps one point per voxel, growing the voxel until at most `limit` points remain."""
    if len(points) <= limit:
        return points, colors
    extent = float(np.ptp(points, axis=0).max())
    voxel = extent / 700.0
    origin = points.min(axis=0)
    while True:
        cells = np.floor((points - origin) / voxel).astype(np.int64)
        keys = (cells[:, 0] * 73856093) ^ (cells[:, 1] * 19349663) ^ (cells[:, 2] * 83492791)
        _, first = np.unique(keys, return_index=True)
        if len(first) <= limit:
            return points[first], colors[first]
        voxel *= 1.2


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("video", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--frames", type=int, default=32)
    parser.add_argument("--conf-percentile", type=float, default=40.0,
                        help="drop the least confident N percent of points")
    parser.add_argument("--view-points", type=int, default=1_200_000,
                        help="size of the downsampled scene.ply used by the viewer")
    args = parser.parse_args()

    if not torch.cuda.is_available():
        print("No CUDA GPU found. klecks never runs ML on the CPU; aborting.", file=sys.stderr)
        return 2

    progress("frames", 2, "Preparing frames")
    frames = extract_frames(args.video, args.out / "frames", args.frames)
    if not frames:
        print(f"Could not read any frame from {args.video}", file=sys.stderr)
        return 3

    progress("model", 10, "Loading model (the first run downloads 5 GB)")
    device = "cuda"
    dtype = torch.bfloat16 if torch.cuda.get_device_capability()[0] >= 8 else torch.float16
    model = VGGT.from_pretrained("facebook/VGGT-1B").to(device).eval()
    images = load_and_preprocess_images([str(p) for p in frames]).to(device)

    progress("inference", 60, "Estimating cameras and depth")
    started = time.time()
    with torch.no_grad(), torch.autocast("cuda", dtype=dtype):
        predictions = model(images)
    torch.cuda.synchronize()
    inference_seconds = time.time() - started
    peak_vram = torch.cuda.max_memory_allocated() / 1e9

    progress("cloud", 80, "Building the 3D point cloud")
    extrinsic, intrinsic = pose_encoding_to_extri_intri(predictions["pose_enc"], images.shape[-2:])
    extrinsic = extrinsic.squeeze(0).float().cpu().numpy()      # S,3,4 world->camera
    intrinsic = intrinsic.squeeze(0).float().cpu().numpy()
    depth = predictions["depth"].squeeze(0).float().cpu().numpy()          # S,H,W,1
    confidence = predictions["depth_conf"].squeeze(0).float().cpu().numpy()  # S,H,W
    del predictions, model
    torch.cuda.empty_cache()

    world = unproject_depth_map_to_point_map(depth, extrinsic, intrinsic)  # S,H,W,3
    rgb = (images.float().cpu().numpy().transpose(0, 2, 3, 1) * 255).clip(0, 255)  # S,H,W,3

    keep = confidence >= np.percentile(confidence, args.conf_percentile)
    points, colors = world[keep], rgb[keep]
    write_ply(args.out / "points.ply", points, colors)

    view_points, view_colors = voxel_downsample(points, colors, args.view_points)
    write_ply(args.out / "scene.ply", view_points, view_colors)

    # How much did the camera actually move? Pure panning gives ~0.
    rotation, translation = extrinsic[:, :, :3], extrinsic[:, :, 3]
    centers = -np.einsum("sji,sj->si", rotation, translation)
    spread = float(np.linalg.norm(centers - centers.mean(0), axis=1).max())
    scene = float(np.linalg.norm(points - np.median(points, axis=0), axis=1).mean())

    report = {
        "video": args.video.name,
        "frames": len(frames),
        "points": int(len(points)),
        "view_points": int(len(view_points)),
        "inference_seconds": round(inference_seconds, 2),
        "peak_vram_gb": round(peak_vram, 1),
        "camera_center_spread": spread,
        "mean_point_distance_from_median": scene,
        "spread_to_scene_ratio": spread / scene if scene else None,
        "median_confidence": float(np.median(confidence)),
    }
    (args.out / "report.json").write_text(json.dumps(report, indent=2))
    np.savez_compressed(args.out / "cameras.npz", extrinsic=extrinsic, intrinsic=intrinsic)
    progress("done", 100, "Done")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
