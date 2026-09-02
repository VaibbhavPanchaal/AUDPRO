# Offline Speech Intelligence — Tauri v2 + Python Sidecar

Windows offline desktop app: record audio natively (Rust/cpal), then run
WhisperX → alignment → Pyannote diarization → in-process GGUF summarization
via an air-gapped Python sidecar. Frontend is React/TypeScript + Tailwind.

## Layout
- `src-tauri/` — Rust shell (audio capture, session store, sidecar IPC bridge)
- `src/` — React UI (hooks/useTauriBridge, components)
- `python-sidecar/` — Python 3.11 worker (pipeline_stages, sidecar_entry)
- `python-sidecar/models/` — local model weights (air-gapped)

## Protocol
stdin commands: `{"command": "PROCESS", "audioPath": ..., ...}` | PING | EXIT
stdout events: READY | STAGE | PROGRESS | ERROR | COMPLETE | BYE | PONG
(stderr is diagnostics only — never parsed)

## Verify
- Rust: `cargo test` (3 tests pass), `cargo build` clean
- Frontend: `npm run build` (TypeScript + Vite) green
- Python: `.venv/bin/python test_mock_run.py` (pipeline + artifacts), `verify_env.py` (offline + CUDA probe)
