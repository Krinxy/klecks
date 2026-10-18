# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-18
# ---
"""Camera positions of a scan from a global structure-from-motion run (COLMAP 4, on the GPU).

VGGT places cameras window by window; the windows are chained and their small errors add up, which
smears the splat. COLMAP matches the frames of the whole room against each other, finds loops between
films and optimises every camera together. The result is the dataset Brush trains on.

    python colmap_poses.py <scan dir> [--mapper global|incremental] [--features sift|aliked]

Writes <scan dir>/sfm/: images/<video>/..., database.db, sparse/0 (the model), train/ (undistorted images and a
text model, ready for Brush) and sfm.json (what was registered, how well).
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

import numpy as np

COLMAP = os.environ.get("KLECKS_COLMAP") or shutil.which("colmap") or r"D:\Projects\tools\colmap\bin\colmap.exe"
MAX_EXHAUSTIVE = 900   # more frames than this are matched along the film and against a vocabulary tree instead
SIFT_FEATURES = 8192


def run(command: list[str], log: Path, label: str) -> float:
    """One COLMAP step; its output goes to the log. Returns the seconds it took."""
    started = time.time()
    print(f"[{time.strftime('%H:%M:%S')}] {label}", flush=True)
    with open(log, "a", encoding="utf-8") as handle:
        handle.write(f"\n=== {label}\n{' '.join(command)}\n")
        result = subprocess.run(command, stdout=handle, stderr=subprocess.STDOUT)
    if result.returncode != 0:
        raise SystemExit(f"{label} failed (exit {result.returncode}); see {log}")
    return time.time() - started


def prepare_images(scan: Path, workspace: Path) -> list[str]:
    """Frames grouped by film (one camera per film: its own lens settings and orientation)."""
    frames = sorted((scan / "frames").glob("*.jpg"))
    if not frames:
        raise SystemExit("The scan has no frames.")
    shutil.rmtree(workspace / "images", ignore_errors=True)
    names = []
    for frame in frames:
        folder = workspace / "images" / frame.name.split("_")[0]
        folder.mkdir(parents=True, exist_ok=True)
        target = folder / frame.name
        try:
            os.link(frame, target)  # same drive: no copy
        except OSError:
            shutil.copy2(frame, target)
        names.append(f"{folder.name}/{frame.name}")
    return names


def analyse(model: Path, log: Path) -> dict:
    result = subprocess.run([COLMAP, "model_analyzer", "--path", str(model)], capture_output=True, text=True)
    text = result.stdout + result.stderr
    with open(log, "a", encoding="utf-8") as handle:
        handle.write("\n=== model_analyzer\n" + text)
    number = lambda pattern: float(m.group(1)) if (m := re.search(pattern, text)) else None
    return {
        "registered": int(number(r"Registered images:\s*(\d+)") or 0),
        "points": int(number(r"Points:\s*(\d+)") or 0),
        "mean_track_length": number(r"Mean track length:\s*([\d.]+)"),
        "mean_reprojection_error_px": number(r"Mean reprojection error:\s*([\d.]+)"),
    }


FAR_FACTOR = 6.0   # a camera further from the middle than this x the typical one is a failed placement


def drop_far_frames(model: Path, workspace: Path, log: Path) -> tuple[Path, int]:
    """COLMAP sometimes places a handful of frames (a blurred one, a plain wall) absurdly far away.
    Those poses are wrong and would poison the training: take them out of the model."""
    from sfm_align import read_model

    text = workspace / "model_txt"
    shutil.rmtree(text, ignore_errors=True)
    text.mkdir()
    run([COLMAP, "model_converter", "--input_path", str(model), "--output_path", str(text), "--output_type", "TXT"], log, "Checking the camera positions")
    images, _ = read_model(text)
    names = list(images)
    positions = np.stack([-images[name][0].T @ images[name][1] for name in names])  # camera centres
    distance = np.linalg.norm(positions - np.median(positions, axis=0), axis=1)
    far = [name for name, d in zip(names, distance) if d > FAR_FACTOR * float(np.median(distance))]
    if not far:
        return model, 0
    listing = workspace / "far_frames.txt"
    listing.write_text("\n".join(far) + "\n", encoding="utf-8")
    cleaned = workspace / "sparse_clean"
    shutil.rmtree(cleaned, ignore_errors=True)
    cleaned.mkdir()
    run([COLMAP, "image_deleter", "--input_path", str(model), "--output_path", str(cleaned), "--image_names_path", str(listing)], log, f"Removing {len(far)} badly placed frames")
    return cleaned, len(far)


def solve(scan: Path, mapper: str = "global", features: str = "sift", reuse: bool = False) -> dict:
    """`reuse` keeps an existing placement of the cameras (features, matches and mapping) and only redoes what follows."""
    workspace = scan / "sfm"
    workspace.mkdir(exist_ok=True)
    log = workspace / "colmap.log"
    database = workspace / "database.db"
    sparse = workspace / "sparse"
    resumed = reuse and (sparse / "0" / "images.bin").exists()
    if not resumed:
        log.write_text("", encoding="utf-8")
        database.unlink(missing_ok=True)
    timings: dict[str, float] = {}

    names = prepare_images(scan, workspace) if not resumed else [f"{d.name}/{f.name}" for d in sorted((workspace / "images").iterdir()) for f in sorted(d.glob("*.jpg"))]
    images = workspace / "images"
    if resumed:
        return finish(workspace, sparse, log, names, mapper, features, timings)
    extract = [COLMAP, "feature_extractor", "--database_path", str(database), "--image_path", str(images),
               "--ImageReader.single_camera_per_folder", "1", "--ImageReader.camera_model", "SIMPLE_RADIAL",
               "--FeatureExtraction.use_gpu", "1", "--FeatureExtraction.max_image_size", "1920"]
    if features == "aliked":
        extract += ["--FeatureExtraction.type", "ALIKED_N16ROT"]
    else:
        extract += ["--SiftExtraction.max_num_features", str(SIFT_FEATURES)]
    timings["features"] = run(extract, log, f"Features ({features}) of {len(names)} frames")

    if len(names) <= MAX_EXHAUSTIVE:
        timings["matching"] = run([COLMAP, "exhaustive_matcher", "--database_path", str(database),
                                   "--FeatureMatching.use_gpu", "1"], log, "Matching every frame with every other")
    else:
        timings["matching"] = run([COLMAP, "sequential_matcher", "--database_path", str(database),
                                   "--FeatureMatching.use_gpu", "1", "--SequentialMatching.loop_detection", "0"],
                                  log, "Matching neighbouring frames")

    shutil.rmtree(sparse, ignore_errors=True)
    sparse.mkdir()
    command = "global_mapper" if mapper == "global" else "mapper"
    timings["mapping"] = run([COLMAP, command, "--database_path", str(database), "--image_path", str(images),
                              "--output_path", str(sparse)], log, f"Placing the cameras ({command})")
    return finish(workspace, sparse, log, names, mapper, features, timings)


def finish(workspace: Path, sparse: Path, log: Path, names: list[str], mapper: str, features: str, timings: dict[str, float]) -> dict:
    images = workspace / "images"
    command = "global_mapper" if mapper == "global" else "mapper"

    # the largest model: the film frames that belong together
    models = sorted((p for p in sparse.iterdir() if p.is_dir()), key=lambda p: -(p / "images.bin").stat().st_size if (p / "images.bin").exists() else 0)
    if not models:
        raise SystemExit("COLMAP could not place any camera.")
    best, removed = drop_far_frames(models[0], workspace, log)
    stats = analyse(best, log)
    stats["removed_far_frames"] = removed
    # the final model as text, with the pixel positions of its observations (sfm_align scales the depth with them)
    final_text = workspace / "model_txt"
    shutil.rmtree(final_text, ignore_errors=True)
    final_text.mkdir()
    run([COLMAP, "model_converter", "--input_path", str(best), "--output_path", str(final_text), "--output_type", "TXT"], log, "Writing the camera list")
    stats.update({"frames": len(names), "mapper": command, "features": features, "models": len(models), "seconds": {k: round(v) for k, v in timings.items()}})

    # undistorted images and a text model: what Brush trains on
    train = workspace / "train"
    shutil.rmtree(train, ignore_errors=True)
    timings["undistort"] = run([COLMAP, "image_undistorter", "--image_path", str(images), "--input_path", str(best),
                                "--output_path", str(train), "--output_type", "COLMAP", "--max_image_size", "1920"], log, "Undistorting")
    text_model = train / "sparse" / "0"
    text_model.mkdir(parents=True, exist_ok=True)
    run([COLMAP, "model_converter", "--input_path", str(train / "sparse"), "--output_path", str(text_model), "--output_type", "TXT"], log, "Writing the text model")
    stats["train_dir"] = str(train)
    (workspace / "sfm.json").write_text(json.dumps(stats, indent=2))
    return stats


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("scan", type=Path)
    parser.add_argument("--mapper", choices=["global", "incremental"], default="global")
    parser.add_argument("--features", choices=["sift", "aliked"], default="sift")
    parser.add_argument("--reuse", action="store_true", help="keep an existing camera placement, redo only what follows")
    args = parser.parse_args()
    stats = solve(args.scan, args.mapper, args.features, args.reuse)
    print(json.dumps(stats, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
