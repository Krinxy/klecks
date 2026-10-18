# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-18
# ---
"""Finds the doors of a scanned room and puts them on its floor plan.

OWLv2 (Apache-2.0, zero-shot detector) looks for doors and doorways in every frame, on the GPU. The depth and
camera of that frame turn each box into a spot on the plan; detections of the same door from different frames
are merged. Writes the `doors` list into plan.json:

    python detect_doors.py <scan dir>

Needs plan.json (room_plan.py), cameras.npz, frames.json and depth.npz (or vggt.npz of older scans).
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import torch
from PIL import Image

MODEL = "google/owlv2-base-patch16-ensemble"
QUERIES = ["a door", "an open door", "a doorway"]
THRESHOLD = 0.25
NMS_IOU = 0.3
MIN_RATIO = 1.3      # a door is taller than wide
MAX_RATIO = 4.0
MERGE_RADIUS = 0.09  # of the plan's longer side
MIN_FRAMES = 3       # a door seen by one frame only is kept when the detector is sure of it
SURE = 0.5


def load_depth(scan: Path) -> list[np.ndarray] | None:
    """One depth map per frame (they can differ in size from window to window); older scans stored a single array."""
    if (scan / "depth.npz").exists():
        data = np.load(scan / "depth.npz")
        if "depth" in data.files:
            return [frame.astype(np.float32) for frame in data["depth"]]
        return [data[f"d{i}"].astype(np.float32) for i in range(len(data.files))]
    if (scan / "vggt.npz").exists():
        return [frame.astype(np.float32) for frame in np.load(scan / "vggt.npz")["depth"]]
    return None


def detect_frames(paths: list[str]) -> list[list[tuple[float, float, float, float, float]]]:
    """Per frame: boxes (x0, y0, x1, y1 as 0-1 shares of the picture, score) after non-maximum suppression."""
    from torchvision.ops import nms
    from transformers import Owlv2ForObjectDetection, Owlv2Processor

    if not torch.cuda.is_available():
        raise SystemExit("No CUDA GPU found. klecks never runs ML on the CPU; aborting.")
    processor = Owlv2Processor.from_pretrained(MODEL)
    model = Owlv2ForObjectDetection.from_pretrained(MODEL).to("cuda").eval()
    out = []
    for path in paths:
        image = Image.open(path).convert("RGB")
        width, height = image.size
        inputs = processor(text=[QUERIES], images=image, return_tensors="pt").to("cuda")
        with torch.no_grad():
            result = model(**inputs)
        side = max(width, height)  # OWLv2 pads the picture to a square at the bottom and right
        found = processor.image_processor.post_process_object_detection(
            result, threshold=THRESHOLD, target_sizes=torch.tensor([[side, side]]))[0]
        boxes, scores = found["boxes"], found["scores"]
        keep = nms(boxes, scores, NMS_IOU) if len(boxes) else []
        out.append([(float(boxes[i][0]) / width, float(boxes[i][1]) / height,
                     float(boxes[i][2]) / width, float(boxes[i][3]) / height, float(scores[i])) for i in keep])
        del inputs, result
    del model
    torch.cuda.empty_cache()
    return out


def locate(scan: Path, plan: dict, detections: list, depth: list[np.ndarray]) -> list[dict]:
    """Boxes -> places on the plan: `{x, y, width, score, frame, video}` in plan cells."""
    cameras = np.load(scan / "cameras.npz")
    extrinsic, intrinsic = cameras["extrinsic"], cameras["intrinsic"]
    frames = json.loads((scan / "frames.json").read_text())
    # canvas of every frame; older scans had one for all
    canvases = frames.get("canvas") or [[frames.get("height") or depth[0].shape[0], frames.get("width") or depth[0].shape[1]]] * len(depth)
    boxes = frames.get("boxes") or [[0, 0, c[1], c[0]] for c in canvases]
    basis = np.array(plan["transform"]["basis"])
    low, scale = np.array(plan["transform"]["low"]), plan["transform"]["scale"]

    places = []
    for index, found in enumerate(detections):
        bx, by, bw, bh = boxes[index]
        shrink = canvases[index][1] / depth[index].shape[1]  # depth.npz is stored smaller than the canvas
        rotation, translation = extrinsic[index][:, :3], extrinsic[index][:, 3]
        fx, fy, cx = intrinsic[index][0][0], intrinsic[index][1][1], intrinsic[index][0][2]
        for x0, y0, x1, y1, score in found:
            u0, u1 = bx + x0 * bw, bx + x1 * bw            # canvas pixels
            v0, v1 = by + y0 * bh, by + y1 * bh
            core = depth[index][int((v0 + 0.3 * (v1 - v0)) / shrink):int((v0 + 0.7 * (v1 - v0)) / shrink) + 1,
                                int((u0 + 0.3 * (u1 - u0)) / shrink):int((u0 + 0.7 * (u1 - u0)) / shrink) + 1]
            if core.size == 0 or not np.isfinite(core).any():
                continue
            z = float(np.median(core[np.isfinite(core)]))
            if z <= 0:
                continue
            width_world, height_world = (u1 - u0) / fx * z, (v1 - v0) / fy * z
            if not MIN_RATIO <= height_world / max(width_world, 1e-9) <= MAX_RATIO:
                continue
            middle_u, middle_v = (u0 + u1) / 2, (v0 + v1) / 2
            camera_point = z * np.array([(middle_u - cx) / fx, (middle_v - intrinsic[index][1][2]) / fy, 1.0])
            world = rotation.T @ (camera_point - translation)
            flat = (world @ basis - low) * scale
            places.append({"x": float(flat[0]), "y": float(flat[1]), "width": float(width_world * scale),
                           "score": score, "frame": index})
    return places


def merge(places: list[dict], video_of: list[int], reach: float) -> list[dict]:
    """Greedy clustering: the strongest detection claims everything within `reach` cells."""
    doors, left = [], sorted(places, key=lambda p: -p["score"])
    while left:
        lead = left[0]
        near = [p for p in left if np.hypot(p["x"] - lead["x"], p["y"] - lead["y"]) <= reach]
        left = [p for p in left if p not in near]
        weights = np.array([p["score"] for p in near])
        frames = sorted({p["frame"] for p in near})
        if len(frames) < MIN_FRAMES and lead["score"] < SURE:
            continue
        doors.append({
            "x": round(float(np.average([p["x"] for p in near], weights=weights)), 2),
            "y": round(float(np.average([p["y"] for p in near], weights=weights)), 2),
            "width": round(float(np.median([p["width"] for p in near])), 2),
            "score": round(float(weights.max()), 2),
            "frames": len(frames),
            "videos": sorted({video_of[f] for f in frames}),
        })
    return sorted(doors, key=lambda d: -d["score"] * d["frames"])


def find_doors(scan: Path) -> list[dict]:
    plan = json.loads((scan / "plan.json").read_text())
    depth = load_depth(scan)
    if depth is None or "transform" not in plan:
        raise SystemExit("Needs depth.npz and a plan.json that has a transform; scan the room again.")
    frames = json.loads((scan / "frames.json").read_text())["paths"]
    video_of = [int(Path(p).name.split("_")[0]) for p in frames]
    detections = detect_frames(frames)
    places = locate(scan, plan, detections, depth)
    reach = MERGE_RADIUS * max(plan["width"], plan["height"])
    return merge(places, video_of, reach)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    scan = Path(sys.argv[1])
    doors = find_doors(scan)
    plan = json.loads((scan / "plan.json").read_text())
    plan["doors"] = doors
    (scan / "plan.json").write_text(json.dumps(plan))
    print(f"{len(doors)} doors: {[(d['x'], d['y'], d['score'], d['frames']) for d in doors]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
