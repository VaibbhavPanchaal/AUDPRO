# macOS Intel MacBook — Final Checklist & Run Guide

## Current State (What is Done)

| Item | Status |
|------|--------|
| Tauri v2 + React/TS frontend | Built |
| Rust audio capture (cpal + resampler) | x86_64 |
| Python sidecar (WhisperX -> align -> diarize -> summarize) | Frozen |
| PyInstaller sidecar binary | x86_64, 82 MB |
| Sidecar bundled into .app | Contents/Resources/sidecar/ |
| .app bundle | src-tauri/target/release/bundle/macos/Conversation Summarizer.app |
| .dmg installer | dist/Conversation Summarizer_0.1.0_x64.dmg (657 MB) |
| Info.plist microphone permission | NSMicrophoneUsageDescription |
| assetProtocol for audio playback | Scoped to APPDATA/RESOURCE/HOME |
| transformers<5 pin (Intel Mac critical) | Prevents torch 2.2 breakage |
| Git pushed to VaibbhavPanchaal/AUDPRO | main branch |

---

## What You Need on the Intel MacBook

### 1. Model Weights (the only missing piece)
The `python-sidecar/models/` directory has empty placeholder folders. Populate these:

```
python-sidecar/models/
├── whisper/medium-ct2/             ← WhisperX CTranslate2 model
├── alignment/wav2vec2-large-960h/  ← Wav2Vec2 phoneme alignment
├── pyannote/speaker-diarization-3.1/← config.yaml + .bin weights
└── llm/                            ← llama-3.2-3b-instruct-q4_k_m.gguf
```

**How to obtain (air-gap workflow):**
- On a machine with internet, run whisperx once on any audio so it caches HuggingFace models, then copy the cache into the matching paths above.
- For the LLM: download the .gguf from HuggingFace (bartowski/Llama-3.2-3B-Instruct-GGUF) into models/llm/.

### 2. Run the App

Option A — Open the DMG:
```bash
open "dist/Conversation Summarizer_0.1.0_x64.dmg"
```
Drag to /Applications, then launch.

Option B — Dev build:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo tauri dev
```

Option C — Open the .app directly:
```bash
open "src-tauri/target/release/bundle/macos/Conversation Summarizer.app"
```

Option D — If "can't be opened" warning appears:
```bash
xattr -cr "src-tauri/target/release/bundle/macos/Conversation Summarizer.app"
```

---

## Master Prompt — Use This on the Intel MacBook

Copy-paste the following into Cline on your Intel MacBook:

```
MASTER PROMPT — macOS Intel MacBook Setup

Repository: https://github.com/VaibbhavPanchaal/AUDPRO.git

You are helping set up and run AUDPRO — an offline speech intelligence
desktop app (Tauri v2 + Python sidecar) — on an Intel MacBook.

The macOS build already produced a working .app and DMG.

TASKS:
1. Clone the repo (if not already cloned)
2. Verify Rust toolchain (cargo) and Node.js are installed
3. Verify Python 3.11 venv exists at python-sidecar/.venv
4. Check that model weights are populated under python-sidecar/models/
5. If models are missing, give exact instructions (do NOT download)
6. Run `cargo tauri dev` to verify the app launches
7. If dev launch works, run `cargo tauri build` to produce the .app
8. Verify the .app bundle contains the sidecar binary
9. Report final status

DO NOT:
- Modify Rust source code unless there is a compile error
- Change the PyInstaller spec
- Add new dependencies
- Push to git

The sidecar binary (82 MB) is committed in src-tauri/binaries/.
```

---

## Architecture Recap

```
Tauri v2 Shell (Rust, x86_64)
  Frontend: React/TS (Vite) — device picker, record controls, progress, sessions
  Rust Audio Engine (cpal + hound) — enumerate devices, resample to 16kHz, stream WAV
  Sidecar Spawner (tauri-plugin-shell) — spawn on stop_record, NDJSON over stdout
          |
          | stdin/stdout NDJSON
          v
Python Sidecar (PyInstaller frozen, x86_64)
  1. transcribe  — WhisperX/CTranslate2 (local weights)
  2. align       — Wav2Vec2 (local weights)
  3. diarize     — Pyannote 3.1 (local config.yaml)
  4. summarize   — llama-cpp-python (local .gguf)
  VRAM: del model; gc.collect(); torch.cuda.empty_cache()
  Output: transcript.json, metadata.json, summary.md
```

---

## Troubleshooting

| Problem | Fix |
|---------|-----|
| "audio-processor" can't be opened | Right-click → Open, or `xattr -cr` the .app |
| No microphone permission | System Settings → Privacy & Security → Microphone |
| Sidecar crashes on launch | Check `python-sidecar/models/` is populated |
| cargo: command not found | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh -s -- -y` |
| npm: command not found | Install Node.js from nodejs.org |
| DMG won't mount | Rebuild: `cargo tauri build` |

