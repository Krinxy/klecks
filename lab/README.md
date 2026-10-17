# lab

Python playground for experiments and **ONNX export** of models (pose estimation, depth,
segmentation). Nothing here ships with the program: the Rust runtime loads the exported
`.onnx` files from the models directory.

```powershell
conda env create -f lab/environment-cuda.yml
conda activate klecks-lab
```

Only an NVIDIA/CUDA environment exists so far. Add an `environment-rocm.yml` (Linux only) if an
AMD machine ever shows up.
