# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-17
# ---
"""Scores a splat training by the frames it never saw.

Brush holds back every Nth frame (--eval-split-every N) and, with --eval-save-to-disk, writes what
the splat renders for them to <export dir>/eval_<step>/NNNN.png. This compares those renders with
the real video frames (PSNR in dB, SSIM 0..1; higher is better) and reports crispness: edge energy of
the render relative to the real frame (1.0 = as crisp as the video, below 1 = blurrier).

    python lab/eval_splat.py <runs>/room2 <runs>/room2/splat_eval/eval_8000
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import cv2
import numpy as np


def ssim(a: np.ndarray, b: np.ndarray) -> float:
    """Mean SSIM of two float images in [0, 255] (Gaussian window 11, sigma 1.5, per channel)."""
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    blur = lambda x: cv2.GaussianBlur(x, (11, 11), 1.5)
    scores = []
    for channel in range(a.shape[2]):
        x, y = a[..., channel], b[..., channel]
        mu_x, mu_y = blur(x), blur(y)
        var_x, var_y, cov = blur(x * x) - mu_x**2, blur(y * y) - mu_y**2, blur(x * y) - mu_x * mu_y
        score = ((2 * mu_x * mu_y + c1) * (2 * cov + c2)) / ((mu_x**2 + mu_y**2 + c1) * (var_x + var_y + c2))
        scores.append(score.mean())
    return float(np.mean(scores))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("run", type=Path, help="room_scan run folder (has frames.json)")
    parser.add_argument("eval_dir", type=Path, help="Brush eval_<step> folder with the rendered PNGs")
    args = parser.parse_args()

    frames = [Path(p) for p in json.loads((args.run / "frames.json").read_text())["paths"]]
    psnrs, ssims, crisp = [], [], []
    for render_path in sorted(args.eval_dir.glob("*.png")):
        real = cv2.imread(str(frames[int(render_path.stem)])).astype(np.float32)
        render = cv2.imread(str(render_path)).astype(np.float32)
        if render.shape[:2] != real.shape[:2]:
            interpolation = cv2.INTER_AREA if render.shape[0] > real.shape[0] else cv2.INTER_LANCZOS4
            render = cv2.resize(render, (real.shape[1], real.shape[0]), interpolation=interpolation)
        mse = float(((render - real) ** 2).mean())
        psnrs.append(10 * np.log10(255.0**2 / max(mse, 1e-9)))
        ssims.append(ssim(render, real))
        edge = lambda x: float(cv2.Laplacian(cv2.cvtColor(x.astype(np.uint8), cv2.COLOR_BGR2GRAY), cv2.CV_32F).var())
        crisp.append(edge(render) / max(edge(real), 1e-6))
    print(f"{args.eval_dir.parent.name}/{args.eval_dir.name}: PSNR {np.mean(psnrs):.2f} dB  SSIM {np.mean(ssims):.3f}  crispness {np.mean(crisp):.2f}  ({len(psnrs)} held-out frames)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
