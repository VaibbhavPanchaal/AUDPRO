#!/usr/bin/env python3
"""python-sidecar/copy_windows_dlls.py

Locates NVIDIA CUDA/cuDNN and Intel OpenMP DLLs inside the active Python
environment (site-packages) and copies them into the PyInstaller output
directory so the frozen sidecar can load GPU runtimes.

Usage (Windows, venv active, after pyinstaller):
    python copy_windows_dlls.py [--dist-dir dist\\audio-processor-x86_64-pc-windows-msvc]
"""

from __future__ import annotations

import argparse
import shutil
import sys
from pathlib import Path

# DLLs required by CTranslate2 / torch cu124 wheels at runtime.
TARGET_DLLS = [
    "cublas64_12.dll",
    "cublasLt64_12.dll",
    "cudart64_12.dll",
    "cufft64_11.dll",
    "cudnn_ops_infer64_9.dll",
    "cudnn_cnn_infer64_9.dll",
    "cudnn64_9.dll",
    "libiomp5md.dll",
    "onnxruntime.dll",
]


def candidate_dirs() -> list[Path]:
    dirs: list[Path] = []

    import site

    for site_dir in site.getsitepackages():
        dirs.append(Path(site_dir))

    dirs.append(Path(sys.prefix) / "Library" / "bin")

    # torch ships its own DLL bundle.
    try:
        import torch
        dirs.append(Path(torch.__file__).parent / "lib")
    except Exception:
        pass

    # NVIDIA pip wheels (nvidia-cublas-cu12 etc.).
    for base in list(dirs):
        for pattern in ("nvidia/*/bin", "nvidia/*/lib"):
            dirs.extend(base.parent.glob(pattern))

    return [d for d in dirs if d.is_dir()]
def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dist-dir",
        default="dist/audio-processor-x86_64-pc-windows-msvc",
    )
    args = parser.parse_args()

    target = Path(args.dist_dir)
    if not target.is_dir():
        print(f"ERROR: dist directory not found: {target}", file=sys.stderr)
        return 1

    copied: list[str] = []
    missing: list[str] = []

    for dll in TARGET_DLLS:
        found = False
        for directory in candidate_dirs():
            match = directory / dll
            if match.is_file():
                shutil.copy2(match, target / dll)
                copied.append(f"{dll} <- {match}")
                found = True
                break
        if not found:
            missing.append(dll)

    print(f"Copied {len(copied)} DLL(s) into {target}:")
    for entry in copied:
        print(f"  + {entry}")

    if missing:
        print("Not found (OK for CPU-only builds):")
        for entry in missing:
            print(f"  - {entry}")

    print("NOTE: set HF_HUB_OFFLINE=1 in the build shell for air-gapped builds.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
