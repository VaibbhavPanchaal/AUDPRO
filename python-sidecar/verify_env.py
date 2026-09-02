#!/usr/bin/env python3
"""python-sidecar/verify_env.py

Offline environment readiness probe for the speech intelligence sidecar.

Verifies, WITHOUT any network access:
  1. Offline mode env vars (HF_HUB_OFFLINE=1, TRANSFORMERS_OFFLINE=1) are
     forced active before any HF/transformers import.
  2. PyTorch imports and reports CUDA availability; falls back to CPU
     cleanly when no GPU is present.
  3. Required third-party libraries import and report their versions.

Outputs a single JSON report object to stdout (exit code 0 if the core
stack is usable, 1 otherwise). Diagnostics go to stderr only.
"""

from __future__ import annotations

import json
import os
import platform
import sys

# ---------------------------------------------------------------------------
# Air-gap enforcement — MUST run before any HF/transformers/torch import.
# ---------------------------------------------------------------------------
os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_DATASETS_OFFLINE"] = "1"
# Silence HF hub lookups entirely in offline mode.
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")

REPORT: dict = {
    "offline_env": {},
    "python": {},
    "hardware": {},
    "libraries": {},
    "ok": False,
}


def check_offline_env() -> dict:
    return {
        "HF_HUB_OFFLINE": os.environ.get("HF_HUB_OFFLINE") == "1",
        "TRANSFORMERS_OFFLINE": os.environ.get("TRANSFORMERS_OFFLINE") == "1",
        "HF_DATASETS_OFFLINE": os.environ.get("HF_DATASETS_OFFLINE") == "1",
    }


def check_python() -> dict:
    return {
        "version": platform.python_version(),
        "implementation": platform.python_implementation(),
        "executable": sys.executable,
        "frozen": getattr(sys, "frozen", False),
    }


def check_hardware() -> dict:
    hw: dict = {"cuda_available": False, "device": "cpu"}

    try:
        import torch

        hw["torch_version"] = torch.__version__
        hw["cuda_available"] = bool(torch.cuda.is_available())

        if hw["cuda_available"]:
            hw["device"] = "cuda"
            hw["cuda_device_count"] = torch.cuda.device_count()
            hw["cuda_device_name"] = torch.cuda.get_device_name(0)
            props = torch.cuda.get_device_properties(0)
            hw["vram_total_gb"] = round(props.total_memory / (1024**3), 2)
        else:
            # Clean CPU fallback — never raise.
            hw["device"] = "cpu"
            hw["note"] = (
                "CUDA unavailable; pipeline will run on CPU with INT8 compute."
            )
    except Exception as exc:  # noqa: BLE001 — report, don't crash
        hw["torch_error"] = str(exc)

    return hw


def check_libraries() -> dict:
    libs: dict = {}

    for name in (
        "ctranslate2",
        "whisperx",
        "pyannote",
        "llama_cpp",
        "soundfile",
        "scipy",
        "numpy",
    ):
        try:
            module = __import__(name)
            libs[name] = getattr(module, "__version__", "unknown")
        except Exception as exc:  # noqa: BLE001
            libs[name] = f"<import failed: {exc}>"

    return libs


def main() -> int:
    REPORT["offline_env"] = check_offline_env()
    REPORT["python"] = check_python()
    REPORT["hardware"] = check_hardware()
    REPORT["libraries"] = check_libraries()

    # Core readiness: torch imports AND offline vars forced.
    hw = REPORT["hardware"]
    core_ok = (
        REPORT["offline_env"]["HF_HUB_OFFLINE"]
        and REPORT["offline_env"]["TRANSFORMERS_OFFLINE"]
        and "torch_error" not in hw
    )
    REPORT["ok"] = bool(core_ok)

    json.dump(REPORT, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0 if REPORT["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())