# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-17
# ---
"""One or more videos of the same room -> one dense, coloured 3D point cloud.

1. Frames from every video, upright, shared out by video length.
2. VGGT estimates cameras and depth for ALL frames in one pass, so clips line up with each other.
3. Every frame's depth is upsampled and unprojected; colours come from a super-resolution model
   (e.g. 4x-UltraSharp) so edges and texture stay crisp. Views are fused on the GPU (voxel average).

Used by the app as a *temporary* backend (see core/reconstruct); an ONNX model run from Rust is
meant to replace it. GPU only: exits with code 2 if CUDA is missing, klecks never runs ML on the CPU.

Progress is printed as JSON lines prefixed with ``@@`` so the app can parse them:
    @@{"stage": "inference", "pct": 40, "message": "Estimating depth"}

    python lab/room_scan.py media/*.MP4 --out D:/Projects/klecks-runs/room \
        --upscale-model D:/Models/upscale/4x-UltraSharp.safetensors
"""
from __future__ import annotations

import argparse
import gc
import json
import subprocess
import sys
import time
from pathlib import Path

import cv2
import numpy as np
import torch
import torch.nn.functional as F

from vggt.models.vggt import VGGT
from vggt.utils.pose_enc import pose_encoding_to_extri_intri

PATCH = 14  # VGGT works on 14x14 pixel patches
# Measured on an RTX 4080 at 392x686 px per frame (peak allocated: 32 frames 11.1 GB, 64 frames 12.9 GB)
VGGT_BASE_GB = 9.3
VGGT_GB_PER_FRAME = 0.056
VRAM_RESERVE_GB = 2.5  # headroom for the desktop, the app's own web view and allocator slack
MIN_FRAMES = 8


def progress(stage: str, pct: int, message: str) -> None:
    print("@@" + json.dumps({"stage": stage, "pct": int(pct), "message": message}), flush=True)


# ── frames ────────────────────────────────────────────────────────────────────────────────────

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


def video_seconds(video: Path) -> float:
    cap = cv2.VideoCapture(str(video))
    frames, fps = cap.get(cv2.CAP_PROP_FRAME_COUNT), cap.get(cv2.CAP_PROP_FPS) or 30.0
    cap.release()
    return max(frames / fps, 0.1)


def split_budget(seconds: list[float], total: int, minimum: int = 8) -> list[int]:
    """Shares `total` frames across videos by length, but gives every video at least `minimum`."""
    counts = [max(minimum, round(total * s / sum(seconds))) for s in seconds]
    while sum(counts) > total and max(counts) > minimum:
        counts[counts.index(max(counts))] -= 1
    return counts


def extract_frames(video: Path, out_dir: Path, tag: str, count: int) -> list[Path]:
    """Evenly spaced, upright, full-resolution frames from one clip."""
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
        path = out_dir / f"{tag}_{n:04d}.jpg"
        cv2.imwrite(str(path), frame, [cv2.IMWRITE_JPEG_QUALITY, 97])
        paths.append(path)
    cap.release()
    return paths


def load_for_vggt(paths: list[Path], long_side: int) -> torch.Tensor:
    """Whole frame, aspect kept, sides rounded to the model's patch size.

    VGGT's own loader would centre-crop portrait frames to a square and throw most of the image away.
    """
    batch = []
    for path in paths:
        image = cv2.cvtColor(cv2.imread(str(path)), cv2.COLOR_BGR2RGB)
        height, width = image.shape[:2]
        scale = long_side / max(height, width)
        new_w = max(PATCH, round(width * scale / PATCH) * PATCH)
        new_h = max(PATCH, round(height * scale / PATCH) * PATCH)
        resized = cv2.resize(image, (new_w, new_h), interpolation=cv2.INTER_AREA)
        batch.append(torch.from_numpy(resized).permute(2, 0, 1).float() / 255.0)
    return torch.stack(batch)


# ── super-resolution ──────────────────────────────────────────────────────────────────────────

class Upscaler:
    """Tiled super-resolution with any image model spandrel can load (ESRGAN, SwinIR, ...)."""

    def __init__(self, model_path: Path, tile: int = 288, overlap: int = 24):
        from spandrel import ImageModelDescriptor, ModelLoader

        descriptor = ModelLoader().load_from_file(str(model_path))
        if not isinstance(descriptor, ImageModelDescriptor):
            raise ValueError(f"{model_path.name} is not an image upscaling model")
        self.model = descriptor.to("cuda").eval()
        if descriptor.supports_half:
            self.model = self.model.half()
        self.model.model.to(memory_format=torch.channels_last)  # ~1.6x faster convolutions in fp16
        self.half = descriptor.supports_half
        self.scale = descriptor.scale
        self.tile, self.overlap = tile, overlap

    @torch.no_grad()
    def __call__(self, rgb: torch.Tensor) -> torch.Tensor:
        """rgb: 1x3xHxW in [0,1] on the GPU -> 1x3x(H*scale)x(W*scale)."""
        _, _, height, width = rgb.shape
        s, step = self.scale, self.tile - 2 * self.overlap
        out = torch.zeros((1, 3, height * s, width * s), device=rgb.device, dtype=rgb.dtype)
        for y in range(0, height, step):
            for x in range(0, width, step):
                y0, x0 = max(y - self.overlap, 0), max(x - self.overlap, 0)
                y1, x1 = min(y + step + self.overlap, height), min(x + step + self.overlap, width)
                patch = rgb[:, :, y0:y1, x0:x1]
                patch = (patch.half() if self.half else patch).contiguous(memory_format=torch.channels_last)
                result = self.model(patch)
                # keep only the part that belongs to this tile (drop the overlap margin)
                top, left = (y - y0) * s, (x - x0) * s
                bottom, right = top + min(step, height - y) * s, left + min(step, width - x) * s
                out[:, :, y * s:(y + min(step, height - y)) * s, x * s:(x + min(step, width - x)) * s] = \
                    result[:, :, top:bottom, left:right].to(out.dtype)
        return out.clamp_(0, 1)


# ── fusing views ──────────────────────────────────────────────────────────────────────────────

class VoxelFusion:
    """Averages every point that falls in the same voxel, entirely on the GPU."""

    OFFSET = 1 << 20  # keeps voxel indices positive; 21 bits per axis

    def __init__(self, voxel: float, device: str = "cuda"):
        self.voxel, self.device = voxel, device
        self.keys = torch.empty(0, dtype=torch.int64, device=device)
        self.sums = torch.empty((0, 6), dtype=torch.float32, device=device)
        self.counts = torch.empty(0, dtype=torch.float32, device=device)

    def _keys(self, points: torch.Tensor, voxel: float) -> torch.Tensor:
        cells = torch.floor(points / voxel).to(torch.int64) + self.OFFSET
        return (cells[:, 0] << 42) | (cells[:, 1] << 21) | cells[:, 2]

    def add(self, points: torch.Tensor, colors: torch.Tensor) -> None:
        keys = torch.cat([self.keys, self._keys(points, self.voxel)])
        values = torch.cat([self.sums, torch.cat([points, colors], dim=1)])
        weights = torch.cat([self.counts, torch.ones(len(points), device=self.device)])
        self.keys, inverse = torch.unique(keys, return_inverse=True)
        self.sums = torch.zeros((len(self.keys), 6), device=self.device).index_add_(0, inverse, values)
        self.counts = torch.zeros(len(self.keys), device=self.device).index_add_(0, inverse, weights)
        # old voxels already hold sums and their counts, new points each count once

    def result(self) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
        mean = self.sums / self.counts[:, None]
        return (mean[:, :3].cpu().numpy(), mean[:, 3:].clamp(0, 255).cpu().numpy().astype(np.uint8),
                self.counts.cpu().numpy())


def voxel_limit(points: np.ndarray, colors: np.ndarray, limit: int) -> tuple[np.ndarray, np.ndarray]:
    """Coarsens a finished cloud until at most `limit` points remain (viewer / size caps)."""
    if len(points) <= limit:
        return points, colors
    p = torch.from_numpy(points).cuda()
    c = torch.from_numpy(colors).float().cuda()
    extent = float((p.max(0).values - p.min(0).values).max())
    voxel = extent / 1500.0
    while True:
        cells = torch.floor((p - p.min(0).values) / voxel).to(torch.int64)
        keys = (cells[:, 0] << 42) | (cells[:, 1] << 21) | cells[:, 2]
        unique, inverse = torch.unique(keys, return_inverse=True)
        if len(unique) <= limit:
            n = torch.zeros(len(unique), device="cuda").index_add_(0, inverse, torch.ones(len(p), device="cuda"))
            ps = torch.zeros((len(unique), 3), device="cuda").index_add_(0, inverse, p) / n[:, None]
            cs = torch.zeros((len(unique), 3), device="cuda").index_add_(0, inverse, c) / n[:, None]
            return ps.cpu().numpy(), cs.clamp(0, 255).cpu().numpy().astype(np.uint8)
        voxel *= 1.1


def read_ply_points(path: Path) -> tuple[np.ndarray, np.ndarray]:
    """Reads a cloud written by write_ply (float32 xyz + uint8 rgb)."""
    with open(path, "rb") as handle:
        count = 0
        for line in iter(handle.readline, b"end_header\n"):
            if line.startswith(b"element vertex"):
                count = int(line.split()[-1])
        record = np.dtype([("p", "<f4", 3), ("c", "u1", 3)])
        data = np.frombuffer(handle.read(), dtype=record, count=count)
    return data["p"].astype(np.float32), data["c"].copy()


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


# ── main ──────────────────────────────────────────────────────────────────────────────────────

def run_vggt(model: VGGT, images: torch.Tensor, dtype: torch.dtype) -> dict[str, torch.Tensor]:
    with torch.no_grad(), torch.autocast("cuda", dtype=dtype):
        return model(images.cuda())


def nearest_views(extrinsic: torch.Tensor, count: int, scale: float) -> torch.Tensor:
    """For every frame, the `count` frames that look at the most similar part of the room."""
    rotation, translation = extrinsic[:, :, :3], extrinsic[:, :, 3]
    centers = -torch.einsum("sji,sj->si", rotation, translation)
    axes = rotation[:, 2, :]                                         # optical axis in world space
    angle = torch.acos((axes @ axes.T).clamp(-1, 1))
    shift = torch.cdist(centers, centers) / scale
    score = angle + 0.5 * shift
    score.fill_diagonal_(float("inf"))
    return score.topk(min(count, len(score) - 1), largest=False).indices


def agrees_with_neighbours(world: torch.Tensor, neighbours: torch.Tensor, extrinsic: torch.Tensor,
                           intrinsic: torch.Tensor, depth: torch.Tensor, tolerance: float) -> torch.Tensor:
    """Keeps a point unless a neighbouring frame sees clearly *empty* space where it claims a surface.

    Per neighbour: the point lands on a surface at about the same depth (support), in front of
    something nearer (hidden there, says nothing) or in front of something much farther (violation).
    A point is kept if at least one neighbour supports it or none contradicts it.
    """
    _, height, width = depth.shape
    support = torch.zeros(len(world), device=world.device)
    violation = torch.zeros_like(support)
    for j in neighbours.tolist():
        R, t, K = extrinsic[j, :, :3], extrinsic[j, :, 3], intrinsic[j]
        cam = world @ R.T + t
        z = cam[:, 2]
        safe = z.clamp(min=1e-6)
        u = cam[:, 0] / safe * K[0, 0] + K[0, 2]
        v = cam[:, 1] / safe * K[1, 1] + K[1, 2]
        inside = (z > 1e-6) & (u >= 0) & (u <= width - 1) & (v >= 0) & (v <= height - 1)
        grid = torch.stack([u / (width - 1) * 2 - 1, v / (height - 1) * 2 - 1], dim=-1)[None, None]
        seen = F.grid_sample(depth[j][None, None], grid, mode="nearest", align_corners=True)[0, 0, 0]
        relative = (seen - z) / safe
        support += (inside & (relative.abs() < tolerance)).float()
        violation += (inside & (relative > tolerance)).float()
    return (support >= 1) | (violation == 0)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("videos", type=Path, nargs="+")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--frames", type=int, default=72, help="frames shared across all videos")
    parser.add_argument("--long-side", type=int, default=686, help="VGGT input size of the long image side")
    parser.add_argument("--detail", type=float, default=2.0, help="dense grid = VGGT grid x this")
    parser.add_argument("--upscale-model", type=Path, default=None, help="ESRGAN-style .safetensors/.pth")
    parser.add_argument("--conf-percentile", type=float, default=40.0)
    parser.add_argument("--edge-threshold", type=float, default=0.06,
                        help="drop points whose neighbourhood spans more than this fraction of their depth")
    parser.add_argument("--consistency-views", type=int, default=4,
                        help="neighbouring frames each point is checked against (0 = off)")
    parser.add_argument("--consistency-tolerance", type=float, default=0.03)
    parser.add_argument("--voxel-px", type=float, default=2.0, help="fusion voxel size in dense-grid pixels")
    parser.add_argument("--reuse", action="store_true",
                        help="skip frame extraction and VGGT if vggt.npz from an earlier run is present")
    parser.add_argument("--max-points", type=int, default=6_000_000)
    parser.add_argument("--view-points", type=int, default=3_000_000)
    parser.add_argument("--colmap", action="store_true",
                        help="also write <out>/colmap, the dataset a splat trainer (Brush) reads")
    parser.add_argument("--train-upscale-model", type=Path, default=None,
                        help="super-resolve the training images (sharper splats) with this model")
    parser.add_argument("--train-upscale-factor", type=float, default=2.0)
    args = parser.parse_args()

    if not torch.cuda.is_available():
        print("No CUDA GPU found. klecks never runs ML on the CPU; aborting.", file=sys.stderr)
        return 2
    started = time.time()
    timings: dict[str, float] = {}
    peak_vram = 0.0

    # VGGT needs about 9.3 GB + 0.056 GB per frame at 392x686. When the card is full Windows quietly
    # swaps GPU memory out and everything crawls instead of failing, so size the batch to what is free.
    free_gb = torch.cuda.mem_get_info()[0] / 1e9
    affordable = int((free_gb - VRAM_RESERVE_GB - VGGT_BASE_GB) / VGGT_GB_PER_FRAME)
    if affordable < args.frames:
        args.frames = max(MIN_FRAMES, affordable)
        progress("frames", 1, f"Only {free_gb:.1f} GB of GPU memory is free, using {args.frames} frames")
    args.out.mkdir(parents=True, exist_ok=True)
    cache, frames_file = args.out / "vggt.npz", args.out / "frames.json"

    if args.reuse and cache.exists() and frames_file.exists():
        # 1+2 from an earlier run: same frames, same cameras and depth
        meta = json.loads(frames_file.read_text())
        frame_paths, per_video = [Path(p) for p in meta["paths"]], meta["per_video"]
        height, width = meta["height"], meta["width"]
        saved = np.load(cache)
        extrinsic, intrinsic = (torch.from_numpy(saved[k]).cuda() for k in ("extrinsic", "intrinsic"))
        depth, confidence = (torch.from_numpy(saved[k]).cuda() for k in ("depth", "confidence"))
        progress("inference", 20, "Reusing cameras and depth from an earlier run")
    else:
        # 1. frames
        progress("frames", 2, "Preparing frames")
        seconds = [video_seconds(v) for v in args.videos]
        budget = split_budget(seconds, args.frames)
        frame_paths, per_video = [], []
        for index, (video, count) in enumerate(zip(args.videos, budget)):
            got = extract_frames(video, args.out / "frames", f"{index:02d}", count)
            frame_paths += got
            per_video.append(len(got))
        if not frame_paths:
            print("Could not read any frame from the videos", file=sys.stderr)
            return 3
        timings["frames"] = time.time() - started

        # 2. cameras + depth for all frames at once
        progress("model", 8, "Loading the model (the first run downloads 5 GB)")
        dtype = torch.bfloat16 if torch.cuda.get_device_capability()[0] >= 8 else torch.float16
        model = VGGT.from_pretrained("facebook/VGGT-1B").to("cuda").eval()

        progress("inference", 20, f"Estimating cameras and depth for {len(frame_paths)} frames")
        tick = time.time()
        images = load_for_vggt(frame_paths, args.long_side)
        while True:
            try:
                predictions = run_vggt(model, images, dtype)
                break
            except torch.OutOfMemoryError:
                # Out of GPU memory: use fewer frames rather than give up.
                gc.collect(); torch.cuda.empty_cache()
                keep = np.linspace(0, len(images) - 1, int(len(images) * 0.75), dtype=int)
                if len(keep) < 8:
                    print("Not enough GPU memory even for 8 frames.", file=sys.stderr)
                    return 4
                images = images[keep]
                frame_paths = [frame_paths[i] for i in keep]
                per_video = [int(sum(1 for k in keep if lo <= k < hi))
                             for lo, hi in zip(np.cumsum([0] + per_video[:-1]), np.cumsum(per_video))]
                progress("inference", 20, f"GPU memory is tight, using {len(frame_paths)} frames")
        torch.cuda.synchronize()
        timings["inference"] = time.time() - tick
        peak_vram = torch.cuda.max_memory_allocated() / 1e9

        height, width = (int(n) for n in images.shape[-2:])
        extrinsic, intrinsic = pose_encoding_to_extri_intri(predictions["pose_enc"], (height, width))
        extrinsic, intrinsic = extrinsic[0].float(), intrinsic[0].float()           # S,3,4 / S,3,3
        depth = predictions["depth"][0, ..., 0].float()                             # S,H,W
        confidence = predictions["depth_conf"][0].float()                           # S,H,W
        del predictions, model
        gc.collect(); torch.cuda.empty_cache()
        np.savez_compressed(cache, extrinsic=extrinsic.cpu().numpy(), intrinsic=intrinsic.cpu().numpy(),
                            depth=depth.cpu().numpy(), confidence=confidence.cpu().numpy())
        frames_file.write_text(json.dumps({"paths": [str(p) for p in frame_paths], "per_video": per_video,
                                           "height": height, "width": width}))
    conf_threshold = float(torch.quantile(confidence.flatten()[::97], args.conf_percentile / 100.0))

    # 3. dense, sharp points per frame, fused on the GPU
    upscaler = Upscaler(args.upscale_model) if args.upscale_model else None
    grid_h, grid_w = int(height * args.detail), int(width * args.detail)
    median_depth = float(depth.flatten()[::97].median())
    focal = float(intrinsic[:, 0, 0].mean()) * grid_w / width
    fusion = VoxelFusion(voxel=args.voxel_px * median_depth / focal)
    ys, xs = torch.meshgrid(torch.arange(grid_h, device="cuda"), torch.arange(grid_w, device="cuda"), indexing="ij")
    window = int(round(2.5 * args.detail)) * 2 + 1
    neighbours = nearest_views(extrinsic, args.consistency_views, median_depth) if args.consistency_views else None
    candidates = kept = 0

    tick = time.time()
    for i, path in enumerate(frame_paths):
        progress("detail", 25 + 70 * i / len(frame_paths), f"Sharpening details ({i + 1}/{len(frame_paths)})")
        source = cv2.cvtColor(cv2.imread(str(path)), cv2.COLOR_BGR2RGB)
        rgb = torch.from_numpy(source).cuda().permute(2, 0, 1)[None].float() / 255.0
        if upscaler:
            rgb = upscaler(rgb)
        color = F.interpolate(rgb, size=(grid_h, grid_w), mode="area")[0].permute(1, 2, 0) * 255.0   # grid_h,grid_w,3

        d = F.interpolate(depth[i][None, None], size=(grid_h, grid_w), mode="bilinear", align_corners=False)
        c = F.interpolate(confidence[i][None, None], size=(grid_h, grid_w), mode="bilinear", align_corners=False)[0, 0]
        # Near an object border the blended depth floats in mid-air: drop pixels whose surroundings
        # span a big share of their own depth.
        span = F.max_pool2d(d, window, 1, window // 2) + F.max_pool2d(-d, window, 1, window // 2)
        d = d[0, 0]
        edge = (span[0, 0] / d.clamp(min=1e-6)) > args.edge_threshold
        valid = (c >= conf_threshold) & (d > 0) & ~edge

        K, (R, t) = intrinsic[i], (extrinsic[i, :, :3], extrinsic[i, :, 3])
        fx, fy = K[0, 0] * grid_w / width, K[1, 1] * grid_h / height
        cx, cy = K[0, 2] * grid_w / width, K[1, 2] * grid_h / height
        z = d[valid]
        camera_points = torch.stack([(xs[valid] - cx) / fx * z, (ys[valid] - cy) / fy * z, z], dim=1)
        world_points = (camera_points - t) @ R                                  # R^T (Xc - t)
        pixel_colors = color[valid]
        candidates += len(world_points)
        if neighbours is not None:
            ok = agrees_with_neighbours(world_points, neighbours[i], extrinsic, intrinsic, depth,
                                        args.consistency_tolerance)
            world_points, pixel_colors = world_points[ok], pixel_colors[ok]
        kept += len(world_points)
        fusion.add(world_points, pixel_colors)
        del rgb, color, d, c, edge, valid, camera_points, world_points, pixel_colors
    timings["detail"] = time.time() - tick
    del upscaler
    gc.collect(); torch.cuda.empty_cache()

    # 4. write
    progress("cloud", 96, "Building the 3D scene")
    points, colors, counts = fusion.result()
    del fusion
    full_points, full_colors = voxel_limit(points, colors, args.max_points)
    write_ply(args.out / "points.ply", full_points, full_colors)
    view_points, view_colors = voxel_limit(full_points, full_colors, args.view_points)
    write_ply(args.out / "scene.ply", view_points, view_colors)

    centers = -np.einsum("sji,sj->si", extrinsic[:, :, :3].cpu().numpy(), extrinsic[:, :, 3].cpu().numpy())
    spread = float(np.linalg.norm(centers - centers.mean(0), axis=1).max())
    report = {
        "videos": [v.name for v in args.videos],
        "frames_per_video": per_video,
        "frames": len(frame_paths),
        "points": int(len(full_points)),
        "view_points": int(len(view_points)),
        "views_per_point_mean": float(counts.mean()),
        "kept_after_consistency": round(kept / max(candidates, 1), 3),
        "upscaled": bool(args.upscale_model),
        "inference_seconds": round(timings.get("inference", 0.0), 2),
        "detail_seconds": round(timings["detail"], 2),
        "total_seconds": round(time.time() - started, 1),
        "peak_vram_gb": round(peak_vram, 1),
        "camera_center_spread": spread,
        "median_depth": median_depth,
    }
    (args.out / "report.json").write_text(json.dumps(report, indent=2))
    np.savez_compressed(args.out / "cameras.npz", extrinsic=extrinsic.cpu().numpy(), intrinsic=intrinsic.cpu().numpy())
    if args.colmap:
        from export_colmap import export_dataset  # imported late: it imports this module back

        export_dataset(
            args.out, upscale_model=args.train_upscale_model, upscale_factor=args.train_upscale_factor,
            on_frame=lambda n, total: progress("cloud", 97 + 2 * n / total, "Preparing the training images"),
        )
    progress("done", 100, "Done")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
