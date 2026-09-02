# -*- mode: python ; coding: utf-8 -*-
# PyInstaller --onedir build for the offline speech sidecar.
# Output name matches the Tauri v2 externalBin triple convention:
#   audio-processor-x86_64-pc-windows-msvc
#
# Build (on a Windows x64 machine with the venv active):
#   pyinstaller audio_sidecar.spec --noconfirm

import os
from PyInstaller.utils.hooks import collect_data_files, collect_submodules

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


a = Analysis(
    ["sidecar_entry.py"],
    pathex=[project_root],
    binaries=[],
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=["matplotlib", "tkinter", "notebook", "tensorboard",
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
    name="audio-processor-x86_64-pc-windows-msvc",
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
    name="audio-processor-x86_64-pc-windows-msvc",
)
