# ---
# Owner: Krinxy <dominic.bechtold@icloud.com>
# Contributors: Krinxy <dominic.bechtold@icloud.com>
# Last modified: 2026-10-18
# ---
"""Scores a Brush training on its held-out frames without needing to know which frame is which.

Brush (--eval-split-every N --eval-save-to-disk) writes what the splat renders for the frames it never saw.
Each render is compared with the real pictures of the dataset and matched to the one it resembles most;
PSNR (dB) and SSIM (0..1) of that pair are reported, higher is better, plus crispness: edge energy of the
render relative to the real picture (1.0 = as crisp as the photo).

    python eval_dataset.py <eval_<step> dir> <dataset images dir>
"""
from __future__ import annotations

import sys
from pathlib import Path

import cv2
import numpy as np

MATCH_SIDE = 192   # size used to find the real picture a render belongs to
SCORE_SIDE = 640   # size used for the scores


def load(path: Path, side: int) -> np.ndarray:
    picture = cv2.imread(str(path))
    scale = side / max(picture.shape[:2])
    return cv2.resize(picture, (round(picture.shape[1] * scale), round(picture.shape[0] * scale)), interpolation=cv2.INTER_AREA).astype(np.float32)


def ssim(a: np.ndarray, b: np.ndarray) -> float:
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    blur = lambda x: cv2.GaussianBlur(x, (11, 11), 1.5)
    scores = []
    for channel in range(a.shape[2]):
        x, y = a[..., channel], b[..., channel]
        mu_x, mu_y = blur(x), blur(y)
        var_x, var_y, cov = blur(x * x) - mu_x**2, blur(y * y) - mu_y**2, blur(x * y) - mu_x * mu_y
        scores.append((((2 * mu_x * mu_y + c1) * (2 * cov + c2)) / ((mu_x**2 + mu_y**2 + c1) * (var_x + var_y + c2))).mean())
    return float(np.mean(scores))


def speckle(render: np.ndarray, real: np.ndarray) -> float:
    """Fine grain of the render on the smooth parts of the photo (walls, floor), relative to the photo: 1 = as calm as the photo, above = noisier."""
    grey = lambda x: cv2.cvtColor(x.astype(np.uint8), cv2.COLOR_BGR2GRAY).astype(np.float32)
    a, b = grey(render), grey(real)
    gradient = cv2.GaussianBlur(np.abs(cv2.Laplacian(cv2.GaussianBlur(b, (0, 0), 2.0), cv2.CV_32F)), (0, 0), 6.0)
    flat = gradient < np.percentile(gradient, 40)
    fine = lambda x: np.abs(x - cv2.GaussianBlur(x, (0, 0), 1.5))[flat].mean()
    return float(fine(a) / max(fine(b), 1e-6))


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    renders = sorted(Path(sys.argv[1]).glob("*.png"))
    pictures = sorted(p for p in Path(sys.argv[2]).rglob("*") if p.suffix.lower() in (".jpg", ".jpeg", ".png"))
    small = {}
    psnrs, ssims, crisp, noise = [], [], [], []
    for render_path in renders:
        render_small = load(render_path, MATCH_SIDE)
        best, best_error = None, 1e18
        for picture in pictures:
            if picture not in small:
                small[picture] = load(picture, MATCH_SIDE)
            candidate = small[picture]
            if candidate.shape != render_small.shape:
                continue
            error = float(((candidate - render_small) ** 2).mean())
            if error < best_error:
                best, best_error = picture, error
        if best is None:
            continue
        real, render = load(best, SCORE_SIDE), load(render_path, SCORE_SIDE)
        if render.shape != real.shape:
            render = cv2.resize(render, (real.shape[1], real.shape[0]), interpolation=cv2.INTER_AREA)
        mse = float(((render - real) ** 2).mean())
        psnrs.append(10 * np.log10(255.0**2 / max(mse, 1e-9)))
        ssims.append(ssim(render, real))
        edge = lambda x: float(cv2.Laplacian(cv2.cvtColor(x.astype(np.uint8), cv2.COLOR_BGR2GRAY), cv2.CV_32F).var())
        crisp.append(edge(render) / max(edge(real), 1e-6))
        noise.append(speckle(render, real))
    print(f"{Path(sys.argv[1]).parent.name}: PSNR {np.mean(psnrs):.2f} dB  SSIM {np.mean(ssims):.3f}  crispness {np.mean(crisp):.2f}  noise {np.mean(noise):.2f}  ({len(psnrs)} held-out frames)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
