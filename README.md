# AUDPRO — Offline Speech Intelligence

Tauri v2 desktop app: native audio capture (Rust/cpal) -> frozen Python
sidecar (WhisperX -> Wav2Vec2 alignment -> Pyannote 3.1 diarization ->
in-process GGUF summarization) over a strict stdin/stdout NDJSON protocol.
Fully air-gapped at runtime.

## Platform tracks

| | Windows x64 | macOS Intel |
|---|---|---|
| Audio API | WASAPI (cpal) | CoreAudio (cpal) |
| Compute | CUDA 12.4 (CPU+INT8 fallback) | CPU + INT8 (no CUDA on Intel Macs) |
| Sidecar build kit | `python-sidecar/build-windows.ps1` | `python-sidecar/build-mac-intel.sh` |
| PyInstaller spec | `python-sidecar/audio_sidecar.spec` | `python-sidecar/audio_sidecar_macos.spec` |
| Installer | NSIS `.exe` | `.app` / `.dmg` |

## Quick start (macOS Intel)

```
npm install && npm run build
cd python-sidecar && bash build-mac-intel.sh   # gates + freeze + stage
cd ../src-tauri && npm run tauri build         # .app + .dmg
```

## Quick start (Windows x64)

```
npm install
cd python-sidecar
powershell -ExecutionPolicy Bypass -File build-windows.ps1
cd ..\src-tauri && npm run tauri build
```

## Model weights (provision locally, never downloaded at runtime)

`python-sidecar/models/{whisper/medium-ct2, alignment/wav2vec2-large-960h,
pyannote/speaker-diarization-3.1, llm}` — see `MASTER_PROMPT_MACOS.md`
§5 Step B for exact file lists.

## Protocol

stdin: `{"command":"PROCESS",...}` | PING | EXIT
stdout: READY | STAGE | PROGRESS | ERROR | COMPLETE | BYE | PONG
(stderr is diagnostics only — never parsed by the host)
