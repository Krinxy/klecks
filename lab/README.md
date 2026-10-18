# lab

Python playground for experiments and the **temporary reconstruction backend**. Nothing here ships
with the program: the plan is an ONNX model run from Rust (see the root README).

```powershell
conda env create -f lab/environment-cuda.yml          # or: conda create -p D:\Projects\klecks-lab python=3.11
conda activate klecks-lab
```

Only an NVIDIA/CUDA environment exists so far. Model weights land in `HF_HOME` (`D:\Models\huggingface`).

## Scripts

| Script | What it does |
|---|---|
| `room_scan.py` | **The backend.** One or more videos of the same room → upright frames → VGGT cameras + depth for all frames at once → dense fused point cloud (`points.ply`, `scene.ply`) and, with `--colmap`, a COLMAP dataset for splat training |
| `export_colmap.py` | Writes `<run>/colmap` (images, cameras, start points) from a finished run; optional super-resolution of the training images |
| `eval_reproject.py` | Projects a point cloud back into the real cameras and scores it |
| `eval_splat.py` | Scores a Brush training on the frames it never saw (PSNR, SSIM, crispness) |
| `render_points.py` | Renders a point cloud from four viewpoints to look at it |
| `colmap_poses.py` | COLMAP 4 on the GPU (SIFT, exhaustive matching, global mapper) places every camera of the room together; undistorts the frames for Brush |
| `sfm_align.py` | COLMAP's cameras + VGGT's depth, scaled per frame to COLMAP's points; builds the cloud, and installs COLMAP's dataset as `<scan>/colmap` |
| `clean_splat.py` | Takes faint haze, runaway splats and isolated specks out of a trained splat (numpy) |
| `eval_dataset.py` | Scores a Brush run on its held-out frames without knowing which frame is which |
| `room_plan.py`, `detect_doors.py` | Floor plan with coverage scores; doors from OWLv2 + depth |

```powershell
python lab/room_scan.py media/*.MP4 --out D:/Projects/klecks-runs/room --colmap
D:/Projects/tools/brush/brush_app.exe D:/Projects/klecks-runs/room/colmap --total-steps 30000 --export-path D:/Projects/klecks-runs/room/splat
```

The app does exactly this (core/reconstruct runs both steps). Outputs stay outside the repo; they show the owner's home.

## Long films and whole rooms (2026-10-18)

- VGGT only handles 50-70 frames at once (16 GB card), so `room_scan.py` cuts every film at 1.5 frames per
  second, splits the frames of all films into windows of 56 with 12 shared frames and lines the windows up
  one after the other (rotation from the shared cameras, scale from the depth both windows see in those
  frames, position from the camera centres). Seven films incl. a 3.5 minute 4K walk-through: 387 frames,
  9 windows, worst alignment error 19 % of the scene depth, 6.3 minutes.
- Portrait and landscape films can be mixed (white-padded common canvas per window); HDR (HLG) clips
  are tone-mapped to SDR; the 180 degree rotation tag of the landscape clips is applied by ffmpeg.
- ffmpeg's `select` filter can't take hundreds of frame numbers (parser overflow); the `fps` filter can.

## Findings (2026-10-17, RTX 4080, five phone clips of 1.4–13.5 s, standing and panning)

**VGGT-1B** (cameras + depth, weights CC-BY-NC-4.0)
- 72 frames of 392×686 px fit in 13.5 GB VRAM and take about 45 s; all five clips are placed in one
  coordinate system (cameras end up up to 1.3 scene units apart).
- VGGT's own loader centre-crops portrait frames to a square (`crop` and `pad` both give 518×518 for
  720×1280 input), throwing away more than half of each frame. `room_scan.py` resizes the whole frame.
- Phone videos carry a rotation tag (`rotation=-90`) that OpenCV ignores; frames are rotated explicitly.

**Point cloud** (fusing per-frame depth)
- Depth of the same surface differs by several percent between frames, so overlapping frames give a
  speckled, layered wall instead of one surface. Edge filtering and a neighbour-consistency check help a
  little (colour error 25.3 → 24.5 of 255) but cannot fix it: the cloud is a quick preview only.

**Gaussian splats** (Brush v0.3.0, Apache-2.0, COLMAP text dataset from the VGGT cameras)
- 30,000 steps on 72 images take about 10 minutes and give ~1.1 M splats (253 MB). From the filming
  positions the result looks like a photo (sofa, plush toy, radiator, table); seen from a side it falls
  apart into streaks, because standing and turning gives no parallax, so depth stays undetermined.
- Held-out frames (every 8th, 8,000 steps): PSNR 19.3 dB, SSIM 0.76, **crispness 0.27** (edge energy
  relative to the video frame, 1.0 = as crisp as the video).
- **Upscaling the training images 2× with 4x-UltraSharp gave no gain** (19.28 dB, SSIM 0.760,
  crispness 0.24) and made training 60 % slower. Stronger opacity regularisation raised SSIM but made
  the pictures visibly blurrier; SH degree 0/1 and a scale penalty changed nothing worthwhile.
  Defaults stay. The limit is camera alignment and coverage, not image resolution.

**What would actually help:** a video in which the camera *moves through* the room (walk a loop, keep
turning less, see `docs/capture-guide.md`).

## Speed notes
- The 4x-UltraSharp model needs fp16 + `channels_last` (≈1.2 s per 720p frame; without it 15–27 s).
- Brush trains at ≈ 60 steps/s here and writes nothing to the console; progress is read from its
  periodic exports.

## Camera placement: why the room looked smeared (2026-10-18)

Seven films of one living room (387 frames, one long 4K walk of 307) gave a splat that looked like smoke.

- **The data was fine, the camera placement was not.** VGGT places 56-frame windows and chains them. Against a global COLMAP
  solution (373 of 387 frames placed, mean reprojection error 1.1 px) VGGT's *depth of one frame* is very accurate (inside a frame
  it deviates from COLMAP's sparse points by a median 1.8 %), but its **scale differs by +-25 % from frame to frame** (3.4 to 5.5
  COLMAP units per VGGT unit). Chaining windows cannot fix that; it is the smear.
- Same frames, Brush 8,000 steps, held-out frames: **VGGT cameras 18.18 dB / SSIM 0.734 / crispness 0.13; COLMAP cameras
  20.06 dB / SSIM 0.781 / crispness 0.18** (sparse start points only).
- COLMAP sometimes places a few frames (blurred, plain wall) absurdly far away (6 of 373, one at 19,000 units): `colmap_poses.py`
  removes them (`image_deleter`) before undistorting.
- 83 of the walk's 307 frames are blurred by hand movement (Laplacian variance below 45 % of the film's median): `drop_blurry`
  removes up to a quarter of a film's worst frames.
- `clean_splat.py` removes 37 % of a splat's rows (mostly nearly invisible ones) with no visible change: it makes the file smaller
  and faster to load but is not what smears the room.
- Frame extraction: keyframes only (`-skip_frame nokey`) is 10x faster than decoding every frame of a 4K HDR film; NVDEC was not
  faster once only keyframes are decoded.
