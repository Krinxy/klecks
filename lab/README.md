# lab

Python playground for experiments and **ONNX export** of models (pose estimation, depth,
segmentation). Nothing here ships with the program: the Rust runtime loads the exported
`.onnx` files from the models directory.

```powershell
conda env create -f lab/environment-cuda.yml          # or: conda create -p D:\Projects\klecks-lab python=3.11
conda activate klecks-lab
```

Only an NVIDIA/CUDA environment exists so far. Add an `environment-rocm.yml` (Linux only) if an
AMD machine ever shows up. Model weights land in `HF_HOME` (`D:\Models\huggingface`).

## Phase-0 experiments

| Script | What it does |
|---|---|
| `vggt_try.py` | video → 32 upright frames → VGGT camera poses + depth → coloured `points.ply`, `cameras.npz`, `report.json` |
| `render_points.py` | renders that cloud from four viewpoints (`views.png`) to judge the depth |

```powershell
python lab/vggt_try.py media/IMG_7695.MP4 --out D:/Projects/klecks-runs/img7695
python lab/render_points.py D:/Projects/klecks-runs/img7695
```

Outputs stay outside the repo (they show the owner's home).

### Findings (2026-10-17, RTX 4080, 5 phone clips of 1.4–13.5 s, standing + panning)

- VGGT-1B runs fine on the 4080: ~7 s for 32 frames at 518 px, ~11 GB VRAM, 5–8 million points.
- Even without walking through the room it recovers a coherent room layout (table, sofa, TV board,
  floor): a top-down view of the cloud reads like a floor plan. Unseen areas stay empty (black holes).
- **Phone videos are stored sideways plus a rotation tag** (`rotation=-90`); OpenCV ignores it.
  Frame extraction must rotate explicitly, otherwise the model sees images lying on their side.
- Weights are licensed CC-BY-NC-4.0: fine privately, a problem if klecks is ever distributed.
- Not done yet: Gaussian splat training (Brush), scale in metres, ONNX export of VGGT.
