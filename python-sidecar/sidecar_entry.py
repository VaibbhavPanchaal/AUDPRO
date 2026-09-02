#!/usr/bin/env python3
"""python-sidecar/sidecar_entry.py

Offline speech-intelligence worker.

Protocol (STRICT):
  stdin : line-delimited JSON commands
  stdout: line-delimited JSON events {"event": NAME, "payload": {...}}
  stderr: human diagnostics (never parsed by the host)

Commands: PROCESS | PING | EXIT
Events:   READY | STAGE | PROGRESS | ERROR | COMPLETE | BYE | PONG
"""

from __future__ import annotations

import os
import sys

# Air-gap enforcement — MUST run before ANY transformers/hf/whisperx import.
os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_DATASETS_OFFLINE"] = "1"
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")

# Frozen-executable support (PyInstaller on Windows).
if sys.platform == "win32":
    try:
        os.add_dll_directory(os.path.dirname(sys.executable))
    except (AttributeError, OSError):
        pass

import json  # noqa: E402
import time  # noqa: E402
import uuid  # noqa: E402
import wave  # noqa: E402
from pathlib import Path  # noqa: E402

import pipeline_stages as stages  # noqa: E402

_COMPLETE = 100


def emit(event: str, payload: dict | None = None) -> None:
    sys.stdout.write(
        json.dumps({"event": event, "payload": payload or {}}, ensure_ascii=False) + "\n"
    )
    sys.stdout.flush()


def emit_progress(progress: int, stage: str, message: str = "") -> None:
    emit("PROGRESS", {"progress": int(progress), "stage": stage, "message": message})


def emit_stage(stage: str, message: str = "") -> None:
    emit("STAGE", {"stage": stage, "message": message})


def emit_error(message: str, stage: str | None = None) -> None:
    emit("ERROR", {"message": message, "stage": stage})


def wav_duration_seconds(path: Path) -> float:
    try:
        with wave.open(str(path), "rb") as handle:
            return handle.getnframes() / float(handle.getframerate() or 1)
    except Exception:
        return 0.0


def write_metadata(output_dir: Path, session_id: str, duration: float,
                   speaker_count: int, status: str, language: str | None) -> None:
    metadata = {
        "uuid": session_id or str(uuid.uuid4()),
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime()) + "Z",
        "duration_seconds": round(duration, 3),
        "speaker_count": speaker_count,
        "status": status,
        "language": language,
    }
    (output_dir / "metadata.json").write_text(
        json.dumps(metadata, indent=2) + "\n", encoding="utf-8"
    )


def write_transcript(output_dir: Path, segments: list[dict]) -> None:
    cleaned = [
        {
            "start": float(seg.get("start", 0.0)),
            "end": float(seg.get("end", 0.0)),
            "text": str(seg.get("text", "")).strip(),
            "speaker": seg.get("speaker", "SPEAKER_00"),
            "words": seg.get("words", []),
        }
        for seg in segments
    ]
    (output_dir / "transcript.json").write_text(
        json.dumps({"segments": cleaned}, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def write_empty_summary(out: Path) -> None:
    out.write_text(
        "# Overview\n\nNo speech detected in the recording.\n"
        "\n# Key Discussion Topics\n\n- (none)\n"
        "\n# Decisions Made\n\n- (none)\n"
        "\n# Action Items\n\n- (none)\n",
        encoding="utf-8",
    )


# ===========================================================================
# Pipeline
# ===========================================================================

def process_audio_session(
    audio_path: str,
    output_dir: str,
    session_id: str,
    models_dir: str | None = None,
    language: str | None = None,
    device_pref: str | None = None,
    mock: bool = False,
) -> dict:
    """Run the full offline pipeline. Raises only on fatal setup errors."""
    audio = Path(audio_path)
    out = Path(output_dir)
    out.mkdir(parents=True, exist_ok=True)
    duration = wav_duration_seconds(audio)

    if mock:
        return run_mock(out, session_id, duration)

    import torch

    # Hardware fallback: CUDA when present; CPU + INT8 otherwise, no raise.
    cuda_ok = torch.cuda.is_available()
    device = device_pref if device_pref in ("cuda", "cpu") else ("cuda" if cuda_ok else "cpu")
    compute_type = "float16" if device == "cuda" else "int8"
    stages.log(f"device={device} compute_type={compute_type} cuda={cuda_ok}")

    emit_stage("probe", f"device={device}, compute={compute_type}")
    write_metadata(out, session_id, duration, 0, "processing", language)

    models = stages.resolve_models_dir(models_dir)
    speaker_count = 1
    detected_language = language
    segments: list[dict] = []

    # Stage 3.1 — transcribe (WhisperX / CTranslate2)
    emit_stage("transcribe", "Loading WhisperX CTranslate2 model")
    emit_progress(10, "transcribe", "Transcribing audio")
    segments, info = stages.stage_transcribe(
        audio, models, language, device, compute_type,
        on_progress=lambda p: emit_progress(p, "transcribe", "Transcription complete"),
    )
    detected_language = info.get("language", language)

    if not segments:
        write_transcript(out, [])
        write_metadata(out, session_id, duration, 0, "completed", detected_language)
        write_empty_summary(out / "summary.md")
        emit_progress(_COMPLETE, "complete", "No speech detected")
        return {"status": "completed", "segments": 0, "speakers": 0}

    # Stage 3.2 — align (Wav2Vec2); non-fatal on failure.
    emit_stage("align", "Loading Wav2Vec2 alignment model")
    emit_progress(45, "align", "Forced alignment")
    try:
        segments = stages.stage_align(
            segments, audio, models, "cuda" if cuda_ok else "cpu", detected_language,
            on_progress=lambda p: emit_progress(p, "align", "Alignment complete"),
        )
    except Exception as exc:  # noqa: BLE001
        stages.log(f"Alignment skipped: {exc}")
        emit_progress(stages.PROGRESS["align_end"], "align", "Alignment skipped")

    # Stage 3.3 — diarize (Pyannote 3.1); non-fatal on failure.
    emit_stage("diarize", "Loading Pyannote 3.1 pipeline")
    emit_progress(70, "diarize", "Diarizing speakers")
    try:
        segments, speaker_count = stages.stage_diarize(
            segments, audio, models, "cuda" if cuda_ok else "cpu",
            on_progress=lambda p: emit_progress(p, "diarize", "Diarization complete"),
        )
    except Exception as exc:  # noqa: BLE001
        stages.log(f"Diarization skipped: {exc}")
        emit_progress(stages.PROGRESS["diarize_end"], "diarize", "Diarization skipped")

    # Persist transcript before summarization so a crash never loses it.
    write_transcript(out, segments)
    write_metadata(out, session_id, duration, speaker_count, "summarizing", detected_language)

    # Stage 3.4 — summarize (embedded GGUF; extractive fallback).
    emit_stage("summarize", "Loading embedded GGUF model")
    emit_progress(92, "summarize", "Generating summary")
    try:
        stages.stage_summarize(
            segments, models, out / "summary.md", speaker_count, duration,
            on_progress=lambda p: emit_progress(p, "summarize", "Summary complete"),
        )
    except Exception as exc:  # noqa: BLE001
        stages.log(f"Summarization fell back: {exc}")
        stages.write_fallback_summary(
            out / "summary.md", segments, speaker_count, duration
        )
        emit_progress(stages.PROGRESS["summarize_end"], "summarize", "Summary fallback used")

    write_metadata(out, session_id, duration, speaker_count, "completed", detected_language)
    emit_progress(_COMPLETE, "complete", "Pipeline finished")

    return {
        "status": "completed",
        "segments": len(segments),
        "speakers": speaker_count,
        "language": detected_language,
        "transcript_path": str(out / "transcript.json"),
        "summary_path": str(out / "summary.md"),
        "metadata_path": str(out / "metadata.json"),
    }


def run_mock(out: Path, session_id: str, duration: float) -> dict:
    """Skeleton run for test_mock_run.py: validates event sequencing,
    file layout, and metadata without heavy model dependencies."""
    for stage, end_pct in (
        ("transcribe", 40), ("align", 65), ("diarize", 90), ("summarize", 99),
    ):
        emit_stage(stage, f"mock {stage}")
        emit_progress(end_pct, stage, f"mock {stage} complete")

    segments = [
        {
            "start": 0.0, "end": 2.5,
            "text": "This is a mock transcript segment used for testing.",
            "speaker": "SPEAKER_00",
            "words": [
                {"word": "This", "start": 0.0, "end": 0.5, "score": 0.99},
                {"word": "test", "start": 2.0, "end": 2.5, "score": 0.97},
            ],
        },
        {
            "start": 3.0, "end": 5.0,
            "text": "Second speaker replies with mock content.",
            "speaker": "SPEAKER_01", "words": [],
        },
        {
            "start": 5.5, "end": 8.0,
            "text": "We agreed to ship the offline pipeline.",
            "speaker": "SPEAKER_00", "words": [],
        },
    ]

    write_transcript(out, segments)
    (out / "summary.md").write_text(
        "# Overview\n\nMock summary for offline pipeline verification.\n"
        "\n# Key Discussion Topics\n\n- Offline pipeline testing\n"
        "\n# Decisions Made\n\n- Ship the offline pipeline\n"
        "\n# Action Items\n\n- Run test_mock_run.py after every change\n",
        encoding="utf-8",
    )
    write_metadata(out, session_id, duration, 2, "completed", "EN")
    emit_progress(_COMPLETE, "complete", "Mock pipeline finished")

    return {
        "status": "completed", "mock": True, "segments": len(segments),
        "speakers": 2,
        "transcript_path": str(out / "transcript.json"),
        "summary_path": str(out / "summary.md"),
        "metadata_path": str(out / "metadata.json"),
    }


# ===========================================================================
# stdin command loop
# ===========================================================================

def handle_command(message: dict) -> bool:
    """Handle one command. Returns False to exit the loop."""
    command = str(message.get("command", "")).upper()

    if command in ("EXIT", "QUIT", "SHUTDOWN"):
        emit("BYE", {"reason": "shutdown requested"})
        return False

    if command == "PING":
        emit("PONG", {"time": time.time()})
        return True

    if command == "PROCESS":
        try:
            result = process_audio_session(
                audio_path=str(message.get("audioPath", "")),
                output_dir=str(message.get("outputDir", "")),
                session_id=str(message.get("sessionId", "")),
                models_dir=message.get("modelsDir"),
                language=message.get("language"),
                device_pref=message.get("device"),
                mock=bool(message.get("mock", False)),
            )
            emit("COMPLETE", result)
        except Exception as exc:  # noqa: BLE001 — the loop must survive errors
            emit_error(str(exc))
            stages.log(f"PROCESS failed: {exc!r}")
        return True

    emit_error(f"Unknown command: {command or '<empty>'}")
    return True


def serve_stdin() -> None:
    emit("READY", {"pid": os.getpid()})

    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError as exc:
            emit_error(f"Malformed JSON command: {exc}")
            continue
        if not isinstance(message, dict):
            emit_error("Command must be a JSON object")
            continue
        if not handle_command(message):
            break

    stages.log("stdin closed; exiting")


def main(argv: list[str]) -> int:
    # No args: stdin command loop. Args: one-shot CLI mode.
    if len(argv) <= 1:
        serve_stdin()
        return 0

    args: dict = {}
    i = 1
    while i < len(argv):
        arg = argv[i]
        if arg.startswith("--"):
            key = arg[2:]
            if i + 1 < len(argv) and not argv[i + 1].startswith("--"):
                args[key] = argv[i + 1]
                i += 2
            else:
                args[key] = True
                i += 1
        else:
            i += 1

    if "verify" in args:
        import subprocess
        script = Path(__file__).resolve().parent / "verify_env.py"
        return subprocess.call([sys.executable, str(script)])

    try:
        result = process_audio_session(
            audio_path=str(args.get("audio", "")),
            output_dir=str(args.get("output-dir", ".")),
            session_id=str(args.get("session-id", "")),
            models_dir=str(args["models-dir"]) if "models-dir" in args else None,
            language=str(args["language"]) if "language" in args else None,
            device_pref=str(args["device"]) if "device" in args else None,
            mock=bool(args.get("mock", False)),
        )
        emit("COMPLETE", result)
        return 0
    except Exception as exc:  # noqa: BLE001
        emit_error(str(exc))
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
