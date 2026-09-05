# MASTER BUILD PROMPT — macOS (Intel) Offline Speech Intelligence App (AUDPRO)

> Copy this entire file into any AI coding tool (Cline etc.) on the target
> **Intel MacBook** (macOS 13+) to finish the build end-to-end. The source is
> already complete and validated on this exact platform — commit c77d6a9 on
> `main` at https://github.com/VaibbhavPanchaal/AUDPRO

## 1. Role & Identity
Act as a principal systems engineer specializing in Rust (Tauri v2), Python
systems programming, and offline ML (WhisperX, CTranslate2, Wav2Vec2,
Pyannote 3.1, llama-cpp-python). Your job: take the committed project and
FINISH the macOS Intel build end-to-end on this machine. The source is
complete; do not redesign it. Execute, verify, fix, package.

## 2. Core Architectural Invariants (NON-NEGOTIABLE)
1. STRICTLY AIR-GAPPED & OFFLINE: no runtime network calls, CDN URLs,
   Hugging Face Hub downloads, or remote APIs. `HF_HUB_OFFLINE=1`,
   `TRANSFORMERS_OFFLINE=1`, `HF_DATASETS_OFFLINE=1` are enforced at the top
   of `python-sidecar/sidecar_entry.py`, `pipeline_stages.py`,
   `verify_env.py`.
2. PROCESS SEPARATION (Tauri Sidecar): React/TS never runs Python; the Rust
   shell spawns the frozen sidecar `audio-processor-x86_64-apple-darwin`;
   communication is EXCLUSIVELY line-delimited JSON over stdio (commands:
   PROCESS/PING/EXIT; events: READY/STAGE/PROGRESS/ERROR/COMPLETE/BYE/PONG;
   stderr = diagnostics only).
3. ALL-IN-ONE PACKAGE: single .app/.dmg; NO standalone servers (no Ollama,
   no FastAPI). Summarization is in-process llama-cpp.
4. VRAM/MEMORY RECOVERY SEQUENCING (implemented — do not break):
   transcribe (CTranslate2) -> del + gc.collect() + torch.cuda.empty_cache()
   -> align (Wav2Vec2 local) -> release -> diarize (Pyannote 3.1 local
   config.yaml) -> release -> summarize (llama-cpp GGUF).
5. HARDWARE: **Intel Macs have NO CUDA and NO usable MPS. The pipeline MUST
   run on CPU with INT8 compute — this is the expected configuration, not a
   fallback.** `verify_env.py` reporting `device: cpu` is a PASS condition.


## 3. Repository State (VERIFY, DO NOT REBUILD BLINDLY)
Layout on `main` @ c77d6a9:
- `src-tauri/` — Rust shell: lib.rs, main.rs, audio/recorder.rs +
  wav_writer.rs (cpal/CoreAudio capture, software resample to 16 kHz mono
  16-bit PCM, streamed WAV), commands/audio.rs, sessions.rs, sidecar.rs,
  state/, Cargo.toml, build.rs, tauri.conf.json (targets "all",
  minimumSystemVersion 13.0), capabilities/default.json.
- `src/` — React UI: hooks/useTauriBridge.ts + components/.
- `python-sidecar/` — sidecar_entry.py, pipeline_stages.py, verify_env.py,
  test_mock_run.py, requirements.txt (numpy<2 pinned: torch 2.x wheels use
  the NumPy 1.x C ABI), audio_sidecar_macos.spec, build-mac-intel.sh.
Already validated on this platform: `cargo build` + `cargo test` (3 tests)
green; `npm run build` green; mock pipeline green; stdin protocol green.
Alter ONLY build/deployment glue; core source changes require a genuine
macOS-specific bug.

## 4. Data Schema (must hold at runtime)
Session folder `%APPDATA%/<com.example.mytauriapp>/sessions/<session-id>/`:
- `audio.wav` 16 kHz / 1 ch / 16-bit PCM (Rust-written)
- `transcript.json` WhisperX segments, word start/end + speaker tags
- `metadata.json` uuid, timestamp, duration_seconds, speaker_count, status
- `summary.md` sections: Overview, Key Discussion Topics, Decisions Made,
  Action Items

## 5. Execution Steps (in order, on THIS Intel Mac)

### Step A — Pull and validate
```
git clone https://github.com/VaibbhavPanchaal/AUDPRO.git && cd AUDPRO
git checkout main && git status
```

### Step B — Provision OFFLINE model weights (never downloaded at runtime)
Populate the placeholder dirs under `python-sidecar/models/`:
- `whisper/medium-ct2/` CTranslate2 Whisper medium (model.bin, config.json,
  tokenizer.json, vocab.json)
- `alignment/wav2vec2-large-960h/` HF format (pytorch_model.bin, config.json,
  vocab.json, tokenizer.json, preprocessor_config.json)
- `pyannote/speaker-diarization-3.1/` local `config.yaml` + referenced
  segmentation/embedding weights; patch config.yaml paths to relative-local
  (pipeline fails loudly if config.yaml missing)
- `llm/` `llama-3.2-3b-instruct-q4_k_m.gguf` (any single .gguf auto-detected)


### Step D — Python 3.11 env + frozen sidecar
Prereq: macOS 13+ on Intel, Xcode CLT (`xcode-select --install`), Homebrew
Python 3.11. One command does it all with gates:
```
cd python-sidecar
bash build-mac-intel.sh
```
It performs, in order, and REPORTS each result:
1. Exports the air-gap env vars.
2. Creates/reuses `.venv` (Python 3.11 via Homebrew).
3. Installs CPU torch + `-r requirements.txt` (whisperx, ctranslate2>=4,
   pyannote.audio>=3.1, llama-cpp-python, soundfile, scipy, numpy<2,
   matplotlib [whisperx hard-import], pyinstaller). Build-time network is
   permitted; runtime never.
4. `verify_env.py` — MUST exit 0; `device: cpu` is the expected PASS.
5. `test_mock_run.py` — MUST print MOCK RUN PASSED.
6. `pyinstaller audio_sidecar_macos.spec --noconfirm` -> onedir
   `dist/audio-processor-x86_64-apple-darwin/` (~2-4 GB, be patient).
7. Ad-hoc codesigns the binary, then smoke-tests the FROZEN binary:
   stdin PING/EXIT must yield READY/PONG/BYE, and a `--mock` run must emit
   COMPLETE plus transcript.json/summary.md/metadata.json.
8. Leaves the onedir dist in place. `src-tauri/tauri.conf.json` bundles it
   via `bundle.resources` -> `"sidecar/"` under the app resource dir; Rust
   `sidecar.rs` resolves `<resource_dir>/sidecar/audio-processor-x86_64-apple-darwin`
   and spawns it (DO NOT use `externalBin` — Tauri flattens onedir payloads
   and the launcher loses `_internal/`; resources preserves the layout).
If any gate fails, read stderr, fix the root cause, re-run that step.
Typical fixes: missing Xcode CLT for llama-cpp build; PyInstaller missing
hooks -> extend `collect_dynamic_libs`/`hiddenimports` in
`audio_sidecar_macos.spec`; numpy>1 breaking torch import -> keep numpy<2.

### Step E — Tauri app + installer
```
cd ../src-tauri
cargo check && cargo test        # must stay green
cd .. && npm run tauri build     # bundles .app + .dmg
```
Artifacts land in
`src-tauri/target/release/bundle/macos/*.app` and `.../dmg/*.dmg`.

### Step F — End-to-end offline verification
1. Copy the .app to a second user account / second Intel Mac, Wi-Fi OFF.
2. Launch -> device list populates -> record/stop produces playable
   audio.wav.
3. Pipeline runs through transcribe->align->diarize->summarize -> COMPLETE
   with real weights (no --mock).
4. All four session artifacts present and schema-valid; app never touches
   the network (no Hub, no server, no telemetry).
5. Relaunch -> session history persists.

## 6. Execution Discipline (mandatory)
- Production-ready changes only; NO placeholder comments.
- Run real commands; read real output before declaring success.
- On any compiler/runtime error: read stderr, diagnose root cause, fix,
  re-run until green.
- Core source stays intact unless a genuine macOS-specific bug appears.
- Final report: (a) gates A-F pass/fail, (b) exact .dmg/.app paths,
  (c) deviations from this prompt and why.
