# -*- mode: python ; coding: utf-8 -*-
# PyInstaller --onedir build for the offline speech sidecar — macOS Intel.
# Output name matches the Tauri v2 externalBin triple convention:
#   audio-processor-x86_64-apple-darwin
#
# Build (on this Mac, venv active):
#   bash build-mac-intel.sh
# or directly:
#   .venv/bin/pyinstaller audio_sidecar_macos.spec --noconfirm
#
# Intel Macs have no CUDA: the pipeline runs CPU + INT8 (expected config).
# macOS PyInstaller collects .dylibs automatically, but ctranslate2 /
# llama_cpp / torch ship dylibs outside their packages, so we pull them in
# explicitly (the counterpart of copy_windows_dlls.py on Windows).

import os
from PyInstaller.utils.hooks import (
    collect_data_files,
    collect_dynamic_libs,
    collect_submodules,
)

project_root = os.path.dirname(os.path.abspath(SPEC))

hiddenimports = []
for package in ("whisperx", "pyannote.audio", "ctranslate2", "llama_cpp",
                "torch", "torchaudio", "scipy", "soundfile"):
    try:
        hiddenimports += collect_submodules(package)
    except Exception:
        pass

datas = []

# Bundle local model weights: python-sidecar/models/** -> models/**
models_dir = os.path.join(project_root, "models")
if os.path.isdir(models_dir):
    for root, _dirs, files in os.walk(models_dir):
        for name in files:
            source = os.path.join(root, name)
            relative = os.path.relpath(source, project_root)
            datas.append((source, os.path.dirname(relative)))

datas += collect_data_files("whisperx")
datas += collect_data_files("pyannote.audio")

binaries = []
for package in ("ctranslate2", "llama_cpp", "torch", "soundfile"):
    try:
        binaries += collect_dynamic_libs(package)
    except Exception:
        pass

a = Analysis(
    ["sidecar_entry.py"],
    pathex=[project_root],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=["tkinter", "notebook", "tensorboard",
              "IPython", "jupyter"],
    noarchive=False,
    optimize=0,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="audio-processor-x86_64-apple-darwin",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    name="audio-processor-x86_64-apple-darwin",
)
