# Windows Offline Build Script (Step 6)
# Run from PowerShell on a Windows x64 machine with Python 3.11 + NVIDIA GPU.
#
#   powershell -ExecutionPolicy Bypass -File build-windows.ps1

$ErrorActionPreference = "Stop"

Set-Location $PSScriptRoot

Write-Host "==> Setting air-gap build environment" -ForegroundColor Cyan
$env:HF_HUB_OFFLINE = "1"
$env:TRANSFORMERS_OFFLINE = "1"
$env:HF_DATASETS_OFFLINE = "1"

# --- 1. Create venv with Python 3.11 ----------------------------------------
if (-not (Test-Path ".venv")) {
    Write-Host "==> Creating Python 3.11 venv"
    py -3.11 -m venv .venv
}

Write-Host "==> Activating venv"
& ".venv\Scripts\Activate.ps1"

# --- 2. Install requirements (CUDA 12.4 torch wheels first) ------------------
Write-Host "==> Installing torch stack (CUDA 12.4)"
& .venv\Scripts\pip.exe install torch torchaudio --index-url https://download.pytorch.org/whl/cu124

Write-Host "==> Installing remaining requirements"
& .venv\Scripts\pip.exe install -r requirements.txt

# --- 3. Verify environment ----------------------------------------------------
Write-Host "==> Verifying offline environment"
& .venv\Scripts\python.exe verify_env.py

# --- 4. Run mock pipeline test ------------------------------------------------
Write-Host "==> Running mock pipeline test"
& .venv\Scripts\python.exe test_mock_run.py

# --- 5. PyInstaller freeze ----------------------------------------------------
Write-Host "==> Building sidecar with PyInstaller (onedir)"
& .venv\Scripts\python.exe -m PyInstaller audio_sidecar.spec --noconfirm

# --- 6. Copy CUDA/cuDNN DLLs --------------------------------------------------
Write-Host "==> Copying GPU runtime DLLs into dist"
& .venv\Scripts\python.exe copy_windows_dlls.py --dist-dir dist\audio-processor-x86_64-pc-windows-msvc

# --- 7. Done: the onedir dist stays in python-sidecar/dist/ -------------------
# tauri.conf.json bundles it via `bundle.resources` ("sidecar/" under the
# app's resource dir). Nothing is copied into src-tauri/binaries anymore.
Write-Host ""
Write-Host "SIDECAR READY: $PSScriptRoot\dist\audio-processor-x86_64-pc-windows-msvc"
Write-Host "Next: cd ..\src-tauri && npm run tauri build" -ForegroundColor Green