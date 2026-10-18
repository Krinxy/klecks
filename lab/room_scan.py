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
from dataclasses import dataclass
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
VRAM_RESERVE_GB = 3.0  # headroom for the desktop, the app's own web view and allocator slack
VRAM_CAP_MARGIN_GB = 2.0  # memory left to everything else on the card (CUDA context, web views, desktop)
MIN_FRAMES = 8


def progress(stage: str, pct: int, message: str) -> None:
    print("@@" + json.dumps({"stage": stage, "pct": int(pct), "message": message}), flush=True)


# ── frames ────────────────────────────────────────────────────────────────────────────────────

FRAME_LONG_SIDE = 1920  # frames are stored at most this large (Brush trains at <= 1920 anyway)
HDR_TRANSFERS = {"arib-std-b67", "smpte2084"}  # HLG and PQ
TONEMAP = ("zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,"
           "tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv420p")


def probe_video(video: Path) -> dict:
    """Length, frame rate and colour transfer of a clip (ffprobe's JSON: HDR clips carry several blocks)."""
    result = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0",
         "-show_entries", "stream=nb_frames,r_frame_rate,duration,color_transfer:format=duration",
         "-of", "json", str(video)],
        capture_output=True, text=True,
    )
    data = json.loads(result.stdout or "{}")
    stream = (data.get("streams") or [{}])[0]
    numerator, _, denominator = str(stream.get("r_frame_rate", "30/1")).partition("/")
    fps = float(numerator) / float(denominator or 1) if float(denominator or 1) else 30.0
    duration = float(stream.get("duration") or data.get("format", {}).get("duration") or 0.1)
    counted = str(stream.get("nb_frames", ""))
    frames = int(counted) if counted.isdigit() else int(duration * fps)
    return {"fps": fps, "duration": max(duration, 0.1), "frames": max(frames, 1),
            "hdr": stream.get("color_transfer") in HDR_TRANSFERS}


def video_seconds(video: Path) -> float:
    return probe_video(video)["duration"]


def extract_frames(video: Path, out_dir: Path, tag: str, count: int) -> list[Path]:
    """Evenly spaced, upright, full-resolution frames from one clip, cut by ffmpeg.

    ffmpeg applies the phone's rotation tag itself (OpenCV does not) and understands every codec.
    HDR clips (HLG / PQ) are tone-mapped to SDR so their colours match the other films of the room.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    info = probe_video(video)
    # `fps` samples evenly in time (a `select` of hundreds of frame numbers overflows ffmpeg's parser)
    sample = f"fps={min(count, info['frames'])}/{info['duration']:.4f}"
    # Nothing downstream looks at more than FRAME_LONG_SIDE pixels, and tone mapping 4K frames is slow.
    shrink = (f"scale='if(gt(iw,ih),min({FRAME_LONG_SIDE},iw),-2)':'if(gt(iw,ih),-2,min({FRAME_LONG_SIDE},ih))'"
              ":flags=lanczos")
    attempts = [[sample, shrink, TONEMAP], [sample, shrink]] if info["hdr"] else [[sample, shrink]]  # tone mapping needs zscale
    problem = ""
    for chain in attempts:
        command = ["ffmpeg", "-v", "error", "-y", "-i", str(video), "-vf", ",".join(chain),
                   "-q:v", "2", str(out_dir / f"{tag}_%04d.jpg")]
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode == 0:
            break
        problem = result.stderr.strip()[-300:]
        for leftover in out_dir.glob(f"{tag}_*.jpg"):
            leftover.unlink()
    frames = sorted(out_dir.glob(f"{tag}_*.jpg"))
    if not frames:
        # a film that silently drops out would give a room that is missing a part: fail loudly instead
        raise SystemExit(f"Could not read frames from {video.name}: {problem or 'no frames came out'}")
    return frames[:count]


MIXED_LONG_SIDE = 518  # canvas side when portrait and landscape films are combined (same token count)


def fit(width: int, height: int, long_side: int) -> tuple[int, int]:
    """Size with the aspect kept, long side near `long_side`, both sides multiples of the patch size."""
    scale = long_side / max(width, height)
    return (max(PATCH, round(width * scale / PATCH) * PATCH), max(PATCH, round(height * scale / PATCH) * PATCH))


def load_for_vggt(paths: list[Path], long_side: int) -> tuple[torch.Tensor, list[tuple[int, int, int, int]]]:
    """Whole frames, aspect kept, sides rounded to the model's patch size.

    VGGT's own loader would centre-crop portrait frames to a square and throw most of the image away.
    A batch needs one shape, so when portrait and landscape films of a room are mixed every frame is
    centred on a common canvas with white padding. `boxes` says where each frame sits on its canvas as
    (x0, y0, width, height); the padding is ignored later.
    """
    images = [cv2.cvtColor(cv2.imread(str(path)), cv2.COLOR_BGR2RGB) for path in paths]
    shapes = {fit(image.shape[1], image.shape[0], long_side) for image in images}
    side = long_side if len(shapes) == 1 else min(long_side, MIXED_LONG_SIDE)
    sizes = [fit(image.shape[1], image.shape[0], side) for image in images]
    canvas_w, canvas_h = max(w for w, _ in sizes), max(h for _, h in sizes)

    batch, boxes = [], []
    for image, (new_w, new_h) in zip(images, sizes):
        resized = cv2.resize(image, (new_w, new_h), interpolation=cv2.INTER_AREA)
        x0, y0 = (canvas_w - new_w) // 2, (canvas_h - new_h) // 2
        canvas = np.full((canvas_h, canvas_w, 3), 255, np.uint8)
        canvas[y0:y0 + new_h, x0:x0 + new_w] = resized
        batch.append(torch.from_numpy(canvas).permute(2, 0, 1).float() / 255.0)
        boxes.append((x0, y0, new_w, new_h))
    return torch.stack(batch), boxes


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
                           intrinsic: torch.Tensor, depth: torch.Tensor, boxes: torch.Tensor,
                           tolerance: float) -> torch.Tensor:
    """Keeps a point unless a neighbouring frame sees clearly *empty* space where it claims a surface.

    Per neighbour: the point lands on a surface at about the same depth (support), in front of
    something nearer (hidden there, says nothing) or in front of something much farther (violation).
    A point is kept if at least one neighbour supports it or none contradicts it. `boxes` (x0, y0, x1, y1
    per frame, canvas pixels) keeps the test inside each neighbour's picture, not its padding.
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
        x0, y0, x1, y1 = boxes[j]
        inside = (z > 1e-6) & (u >= x0) & (u <= x1 - 1) & (v >= y0) & (v <= y1 - 1)
        grid = torch.stack([u / (width - 1) * 2 - 1, v / (height - 1) * 2 - 1], dim=-1)[None, None]
        seen = F.grid_sample(depth[j][None, None], grid, mode="nearest", align_corners=True)[0, 0, 0]
        relative = (seen - z) / safe
        support += (inside & (relative.abs() < tolerance)).float()
        violation += (inside & (relative > tolerance)).float()
    return (support >= 1) | (violation == 0)


# ── long films: parts that are lined up one after the other ───────────────────────────────────

MAX_ALIGN_ERROR = 0.35  # shared frames may disagree by this share of the scene depth, else the part is refused


@dataclass
class Part:
    """VGGT's answer for one window of frames, in its own coordinates until `line_up` moves it."""
    paths: list[Path]
    boxes: list[tuple[int, int, int, int]]
    extrinsic: torch.Tensor      # S,3,4 world -> camera
    intrinsic: torch.Tensor      # S,3,3
    depth: torch.Tensor          # S,H,W
    confidence: torch.Tensor     # S,H,W
    height: int
    width: int


def estimate_part(model: VGGT, paths: list[Path], long_side: int, dtype: torch.dtype) -> Part:
    images, boxes = load_for_vggt(paths, long_side)
    predictions = run_vggt(model, images, dtype)
    height, width = (int(n) for n in images.shape[-2:])
    extrinsic, intrinsic = pose_encoding_to_extri_intri(predictions["pose_enc"], (height, width))
    part = Part(paths, boxes, extrinsic[0].float(), intrinsic[0].float(),
                predictions["depth"][0, ..., 0].float(), predictions["depth_conf"][0].float(), height, width)
    del predictions, images
    return part


def centers_of(extrinsic: np.ndarray) -> np.ndarray:
    """Camera positions in world coordinates from world -> camera matrices."""
    return -np.einsum("sji,sj->si", extrinsic[:, :, :3], extrinsic[:, :, 3])


def rotation_mean(matrices: np.ndarray) -> np.ndarray:
    """The rotation closest to the average of several (SVD of the sum)."""
    u, _, vt = np.linalg.svd(matrices.sum(axis=0))
    rotation = u @ vt
    if np.linalg.det(rotation) < 0:
        u[:, -1] *= -1
        rotation = u @ vt
    return rotation


def line_up(part: Part, previous_extrinsic: np.ndarray, previous_depth: np.ndarray, shared: int):
    """Moves `part` into the global coordinates using the `shared` frames it has in common with the part
    before it. Returns the alignment error (share of the scene depth), or None if there is nothing shared.

    Rotation comes from the cameras of the shared frames, the scale from the depth the two parts see in
    those frames (so it also works when the camera only turns on the spot), the position from the camera
    centres.
    """
    if shared < 2:
        return None
    own_extrinsic = part.extrinsic[:shared].cpu().numpy()
    own_depth = np.array([float(part.depth[i].median()) for i in range(shared)])
    rotation = rotation_mean(np.einsum("mji,mjk->mik", previous_extrinsic[:, :, :3], own_extrinsic[:, :, :3]))
    scale = float(np.median(previous_depth / np.maximum(own_depth, 1e-6)))
    global_centers, own_centers = centers_of(previous_extrinsic), centers_of(own_extrinsic)
    moved = scale * (rotation @ own_centers.T).T
    shift = (global_centers - moved).mean(axis=0)
    error = float(np.median(np.linalg.norm(moved + shift - global_centers, axis=1)) / max(np.median(previous_depth), 1e-6))

    rotation_t = torch.tensor(rotation, dtype=torch.float32, device="cuda")
    shift_t = torch.tensor(shift, dtype=torch.float32, device="cuda")
    new_rotation = part.extrinsic[:, :, :3] @ rotation_t.T
    new_translation = scale * part.extrinsic[:, :, 3] - torch.einsum("sij,j->si", new_rotation, shift_t)
    part.extrinsic = torch.cat([new_rotation, new_translation[..., None]], dim=2)
    part.depth = part.depth * scale
    return error


def sample_counts(seconds: list[float], fps: float, minimum: int, cap: int) -> list[int]:
    """Frames per video: `fps` per second of film, at least `minimum`, shrunk to `cap` frames in total."""
    counts = [max(minimum, round(s * fps)) for s in seconds]
    while sum(counts) > cap and max(counts) > minimum:
        counts[counts.index(max(counts))] -= 1
    return counts


def plan_parts(total: int, window: int, overlap: int) -> list[tuple[int, int]]:
    """Overlapping [start, end) windows over `total` frames; each window starts where the last one
    ends, minus `overlap` shared frames. A tiny remainder is folded into the last window."""
    spans, start = [], 0
    while True:
        end = min(start + window, total)
        if total - end < overlap + 4:
            end = total
        spans.append((start, end))
        if end >= total:
            return spans
        start = end - overlap


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("videos", type=Path, nargs="+")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--window", type=int, default=56, help="frames VGGT looks at together (lowered to fit the GPU)")
    parser.add_argument("--overlap", type=int, default=12, help="frames shared by neighbouring windows")
    parser.add_argument("--fps", type=float, default=1.5, help="frames taken per second of film")
    parser.add_argument("--min-per-video", type=int, default=8)
    parser.add_argument("--max-frames", type=int, default=400, help="frames for the whole room")
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
    parser.add_argument("--reuse-frames", action="store_true", help="keep frames already cut in <out>/frames")
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
    args.out.mkdir(parents=True, exist_ok=True)

    # VGGT needs about 9.3 GB + 0.056 GB per frame at 392x686. When the card is full Windows quietly
    # swaps GPU memory out and everything crawls instead of failing, so size the window to what is free
    # and cap this process: going over raises an out-of-memory error, answered below with a smaller window.
    free_gb, total_gb = (value / 1e9 for value in torch.cuda.mem_get_info())
    torch.cuda.set_per_process_memory_fraction(max(0.5, (total_gb - VRAM_CAP_MARGIN_GB) / total_gb))
    affordable = int((free_gb - VRAM_RESERVE_GB - VGGT_BASE_GB) / VGGT_GB_PER_FRAME)
    window = max(MIN_FRAMES, min(args.window, affordable))
    if window < args.window:
        progress("frames", 1, f"Only {free_gb:.1f} GB of GPU memory is free, using {window} frames at a time")

    # 1. frames, spread over every video by its length
    progress("frames", 2, "Preparing frames")
    seconds = [video_seconds(v) for v in args.videos]
    counts = sample_counts(seconds, args.fps, args.min_per_video, args.max_frames)
    frame_paths: list[Path] = []
    for index, (video, count) in enumerate(zip(args.videos, counts)):
        tag = f"{index:02d}"
        existing = sorted((args.out / "frames").glob(f"{tag}_*.jpg")) if args.reuse_frames else []
        frame_paths += existing or extract_frames(video, args.out / "frames", tag, count)
        progress("frames", 2 + 6 * (index + 1) / len(args.videos), f"Preparing frames ({index + 1}/{len(args.videos)} videos)")
    if not frame_paths:
        print("Could not read any frame from the videos", file=sys.stderr)
        return 3

    # 2. the model
    progress("model", 8, "Loading the model (the first run downloads 5 GB)")
    dtype = torch.bfloat16 if torch.cuda.get_device_capability()[0] >= 8 else torch.float16
    model = VGGT.from_pretrained("facebook/VGGT-1B").to("cuda").eval()
    upscaler = Upscaler(args.upscale_model) if args.upscale_model else None

    # 3. window by window: cameras + depth, line up with the previous window, fuse into one cloud
    while True:
        try:
            outcome = reconstruct(model, upscaler, frame_paths, window, args, dtype)
            break
        except torch.OutOfMemoryError:
            gc.collect(); torch.cuda.empty_cache()
            window = int(window * 0.75)
            if window < MIN_FRAMES:
                print("Not enough GPU memory even for a handful of frames.", file=sys.stderr)
                return 4
            progress("inference", 12, f"GPU memory is tight, using {window} frames at a time")
    if outcome is None:
        print("The first part could not be reconstructed.", file=sys.stderr)
        return 5
    fusion, kept, stats = outcome
    del model, upscaler
    gc.collect(); torch.cuda.empty_cache()

    # 4. write
    progress("cloud", 96, "Building the 3D scene")
    points, colors, counts_per_voxel = fusion.result()
    del fusion
    full_points, full_colors = voxel_limit(points, colors, args.max_points)
    write_ply(args.out / "points.ply", full_points, full_colors)
    view_points, view_colors = voxel_limit(full_points, full_colors, args.view_points)
    write_ply(args.out / "scene.ply", view_points, view_colors)

    extrinsic = np.stack(kept["extrinsic"])
    intrinsic = np.stack(kept["intrinsic"])
    np.savez_compressed(args.out / "cameras.npz", extrinsic=extrinsic, intrinsic=intrinsic)
    per_video = [sum(1 for p in kept["paths"] if p.name.startswith(f"{i:02d}_")) for i in range(len(args.videos))]
    (args.out / "frames.json").write_text(json.dumps({
        "paths": [str(p) for p in kept["paths"]], "per_video": per_video,
        "height": stats["height"], "width": stats["width"], "boxes": [list(b) for b in kept["boxes"]]}))
    try:
        from room_plan import build as build_plan
        (args.out / "plan.json").write_text(json.dumps(build_plan(args.out)))
    except Exception as error:  # the plan is a nicety, never a reason to lose the scan
        print(f"No floor plan: {error}", file=sys.stderr)
    centers = centers_of(extrinsic)
    report = {
        "videos": [v.name for v in args.videos],
        "frames_per_video": per_video,
        "frames": len(kept["paths"]),
        "frames_sampled": len(frame_paths),
        "parts": stats["parts"],
        "parts_used": stats["parts_used"],
        "stopped": stats["stopped"],
        "worst_alignment_error": stats["worst_error"],
        "window": window,
        "points": int(len(full_points)),
        "view_points": int(len(view_points)),
        "views_per_point_mean": float(counts_per_voxel.mean()),
        "kept_after_consistency": round(stats["kept"] / max(stats["candidates"], 1), 3),
        "upscaled": bool(args.upscale_model),
        "detail_seconds": round(stats["dense_seconds"], 2),
        "total_seconds": round(time.time() - started, 1),
        "peak_vram_gb": round(torch.cuda.max_memory_allocated() / 1e9, 1),
        "camera_center_spread": float(np.linalg.norm(centers - centers.mean(0), axis=1).max()),
        "median_depth": stats["median_depth"],
    }
    (args.out / "report.json").write_text(json.dumps(report, indent=2))
    if stats["stopped"]:
        progress("inference", 90, stats["stopped"])
    if args.colmap:
        from export_colmap import export_dataset  # imported late: it imports this module back

        export_dataset(
            args.out, upscale_model=args.train_upscale_model, upscale_factor=args.train_upscale_factor,
            on_frame=lambda n, total: progress("cloud", 97 + 2 * n / total, "Preparing the training images"),
        )
    progress("done", 100, "Done")
    return 0


def reconstruct(model, upscaler, frame_paths, window, args, dtype):
    """All parts in order. Returns (fusion, kept cameras, stats), or None if even the first part fails.

    Stops early, keeping what is lined up so far, when a part does not fit the one before it.
    """
    spans = plan_parts(len(frame_paths), window, args.overlap)
    kept = {"paths": [], "boxes": [], "extrinsic": [], "intrinsic": []}
    stats = {"parts": len(spans), "parts_used": 0, "stopped": "", "worst_error": 0.0, "candidates": 0, "kept": 0,
             "dense_seconds": 0.0, "median_depth": 0.0, "height": 0, "width": 0}
    fusion: VoxelFusion | None = None
    previous = None  # (extrinsic in global coordinates, median depth) of the frames the next part shares

    for number, (first, last) in enumerate(spans):
        progress("inference", 12 + 80 * number / len(spans), f"Reconstructing part {number + 1} of {len(spans)}")
        part = estimate_part(model, frame_paths[first:last], args.long_side, dtype)
        shared = 0
        if previous is not None:
            shared = spans[number - 1][1] - first
            error = line_up(part, previous[0][-shared:], previous[1][-shared:], shared)
            if error is None or error > MAX_ALIGN_ERROR:
                stats["stopped"] = (f"Part {number + 1} of {len(spans)} does not fit the one before it "
                                    f"(off by {0 if error is None else error:.0%}); the room is built from the first {number}.")
                break
            stats["worst_error"] = max(stats["worst_error"], error)
        if fusion is None:
            stats["median_depth"] = float(part.depth.flatten()[::97].median())
            stats["height"], stats["width"] = part.height, part.width
            focal = float(part.intrinsic[:, 0, 0].mean()) * args.detail
            fusion = VoxelFusion(voxel=args.voxel_px * stats["median_depth"] / focal)

        started = time.time()
        fuse_part(part, shared, fusion, upscaler, args, stats)
        stats["dense_seconds"] += time.time() - started

        own = slice(shared, None)  # frames shared with the part before were already taken from that one
        extrinsic = part.extrinsic.cpu().numpy()
        kept["paths"] += part.paths[own]
        kept["boxes"] += part.boxes[own]
        kept["extrinsic"] += list(extrinsic[own])
        kept["intrinsic"] += list(part.intrinsic.cpu().numpy()[own])
        stats["parts_used"] += 1

        next_shared = (spans[number][1] - spans[number + 1][0]) if number + 1 < len(spans) else 0
        if next_shared:
            tail_depth = np.array([float(part.depth[i].median()) for i in range(len(part.paths) - next_shared, len(part.paths))])
            previous = (extrinsic[-next_shared:], tail_depth)
        del part
        gc.collect(); torch.cuda.empty_cache()
    return (fusion, kept, stats) if fusion is not None else None


def fuse_part(part: Part, skip: int, fusion: VoxelFusion, upscaler, args, stats) -> None:
    """Dense, sharp points of every frame of the part (except the first `skip`) into the shared cloud."""
    grid_h, grid_w = int(part.height * args.detail), int(part.width * args.detail)
    ys, xs = torch.meshgrid(torch.arange(grid_h, device="cuda"), torch.arange(grid_w, device="cuda"), indexing="ij")
    window = int(round(2.5 * args.detail)) * 2 + 1
    confidence_cut = float(torch.quantile(part.confidence.flatten()[::97], args.conf_percentile / 100.0))
    median_depth = float(part.depth.flatten()[::97].median())
    neighbours = nearest_views(part.extrinsic, args.consistency_views, median_depth) if args.consistency_views else None
    box_tensor = torch.tensor([[x, y, x + w, y + h] for x, y, w, h in part.boxes], dtype=torch.float32, device="cuda")

    for i in range(skip, len(part.paths)):
        source = cv2.cvtColor(cv2.imread(str(part.paths[i])), cv2.COLOR_BGR2RGB)
        rgb = torch.from_numpy(source).cuda().permute(2, 0, 1)[None].float() / 255.0
        if upscaler:
            rgb = upscaler(rgb)
        # the picture sits inside the canvas (see load_for_vggt); everything around it is padding
        bx, by, bw, bh = (int(round(v * args.detail)) for v in part.boxes[i])
        picture = F.interpolate(rgb, size=(bh, bw), mode="area")[0].permute(1, 2, 0) * 255.0
        color = torch.zeros((grid_h, grid_w, 3), device="cuda")
        color[by:by + bh, bx:bx + bw] = picture
        in_picture = torch.zeros((grid_h, grid_w), dtype=torch.bool, device="cuda")
        in_picture[by:by + bh, bx:bx + bw] = True

        d = F.interpolate(part.depth[i][None, None], size=(grid_h, grid_w), mode="bilinear", align_corners=False)
        c = F.interpolate(part.confidence[i][None, None], size=(grid_h, grid_w), mode="bilinear", align_corners=False)[0, 0]
        # Near an object border the blended depth floats in mid-air: drop pixels whose surroundings
        # span a big share of their own depth.
        span = F.max_pool2d(d, window, 1, window // 2) + F.max_pool2d(-d, window, 1, window // 2)
        d = d[0, 0]
        edge = (span[0, 0] / d.clamp(min=1e-6)) > args.edge_threshold
        valid = (c >= confidence_cut) & (d > 0) & ~edge & in_picture

        K, (R, t) = part.intrinsic[i], (part.extrinsic[i, :, :3], part.extrinsic[i, :, 3])
        fx, fy = K[0, 0] * grid_w / part.width, K[1, 1] * grid_h / part.height
        cx, cy = K[0, 2] * grid_w / part.width, K[1, 2] * grid_h / part.height
        z = d[valid]
        camera_points = torch.stack([(xs[valid] - cx) / fx * z, (ys[valid] - cy) / fy * z, z], dim=1)
        world_points = (camera_points - t) @ R                                  # R^T (Xc - t)
        pixel_colors = color[valid]
        stats["candidates"] += len(world_points)
        if neighbours is not None:
            ok = agrees_with_neighbours(world_points, neighbours[i], part.extrinsic, part.intrinsic, part.depth,
                                        box_tensor, args.consistency_tolerance)
            world_points, pixel_colors = world_points[ok], pixel_colors[ok]
        stats["kept"] += len(world_points)
        fusion.add(world_points, pixel_colors)
        del rgb, picture, color, in_picture, d, c, edge, valid, camera_points, world_points, pixel_colors


if __name__ == "__main__":
    raise SystemExit(main())
