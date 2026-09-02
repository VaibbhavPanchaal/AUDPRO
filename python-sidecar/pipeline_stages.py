"""python-sidecar/pipeline_stages.py

Heavy model stages. Each stage: load -> run -> release memory -> progress.

Air-gap: host must set HF_HUB_OFFLINE=1 / TRANSFORMERS_OFFLINE=1 before use.
"""

from __future__ import annotations

import gc
import sys
from pathlib import Path
from typing import Any, Callable

PROGRESS = {"transcribe_end": 40, "align_end": 65, "diarize_end": 90, "summarize_end": 99}
DEFAULT_GGUF_NAME = "llama-3.2-3b-instruct-q4_k_m.gguf"


def log(message: str) -> None:
    sys.stderr.write(f"[sidecar] {message}\n")
    sys.stderr.flush()


def release_vram(*objects: Any) -> None:
    for obj in objects:
        del obj
    gc.collect()
    try:
        import torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except Exception:
        pass


def resolve_models_dir(explicit: str | None) -> Path:
    if explicit and Path(explicit).is_dir():
        return Path(explicit)
    candidates: list[Path] = []
    if hasattr(sys, "_MEIPASS"):
        candidates.append(Path(sys._MEIPASS) / "models")
    candidates += [
        Path(__file__).resolve().parent / "models",
        Path.cwd() / "models",
        Path.cwd().parent / "python-sidecar" / "models",
    ]
    for c in candidates:
        if c.is_dir():
            return c
    return Path("models")


# ===========================================================================
# Stage 3.1 — Batched transcription with WhisperX / CTranslate2
# ===========================================================================

def stage_transcribe(
    audio_path: Path,
    models_dir: Path,
    language: str | None,
    device: str,
    compute_type: str,
    on_progress: Callable[[int], None],
) -> tuple[list[dict], dict]:
    """Batched WhisperX transcription. Returns (segments, info)."""
    import whisperx

    model_dir = models_dir / "whisper" / "medium-ct2"
    kwargs: dict[str, Any] = {"device": device, "compute_type": compute_type}

    if model_dir.is_dir() and any(model_dir.iterdir()):
        model = whisperx.load_model(str(model_dir), **kwargs)
    else:
        # Named model from faster-whisper's LOCAL cache; offline env
        # vars guarantee no Hub fetch.
        model = whisperx.load_model("medium", **kwargs)

    audio = whisperx.load_audio(str(audio_path))
    batch_size = 16 if device == "cuda" else 8
    result = model.transcribe(audio, batch_size=batch_size, language=language)

    segments: list[dict] = result.get("segments", [])
    info = {"language": result.get("language", language or "unknown")}

    release_vram(model, audio)
    on_progress(PROGRESS["transcribe_end"])
    return segments, info


# ===========================================================================
# Stage 3.2 — Phoneme alignment with Wav2Vec2
# ===========================================================================

def stage_align(
    segments: list[dict],
    audio_path: Path,
    models_dir: Path,
    device: str,
    language: str | None,
    on_progress: Callable[[int], None],
) -> list[dict]:
    import whisperx

    align_dir = models_dir / "alignment" / "wav2vec2-large-960h"
    use_local = align_dir.is_dir() and any(align_dir.iterdir())
    is_english = (language or "en")[:2].lower() == "en"

    align_model, align_metadata = whisperx.load_align_model(
        "wav2vec2-large-960h" if is_english else None,
        device,
        model_dir=str(align_dir) if use_local else None,
    )

    audio = whisperx.load_audio(str(audio_path))
    aligned = whisperx.align(
        segments, align_model, align_metadata, audio, device,
        return_char_alignments=False,
    )

    release_vram(align_model, audio)
    on_progress(PROGRESS["align_end"])
    return aligned.get("segments", segments)


# ===========================================================================
# Stage 3.3 — Speaker diarization with Pyannote 3.1 (local config.yaml)
# ===========================================================================

def stage_diarize(
    segments: list[dict],
    audio_path: Path,
    models_dir: Path,
    device: str,
    on_progress: Callable[[int], None],
) -> tuple[list[dict], int]:
    import whisperx

    config_path = models_dir / "pyannote" / "speaker-diarization-3.1" / "config.yaml"

    if not config_path.is_file():
        raise FileNotFoundError(
            f"Pyannote local config not found: {config_path}. Place "
            "speaker-diarization-3.1 weights under models/pyannote/."
        )

    diarize_pipeline = whisperx.DiarizationPipeline(
        model_name=str(config_path), device=device
    )

    audio = whisperx.load_audio(str(audio_path))
    diarize_segments = diarize_pipeline(audio)
    labeled = whisperx.assign_word_speakers(diarize_segments, segments)

    release_vram(diarize_pipeline, audio)

    speakers = {seg.get("speaker", "SPEAKER_00") for seg in labeled}
    on_progress(PROGRESS["diarize_end"])
    return labeled, len(speakers)


# ===========================================================================
# Stage 3.4 — In-process GGUF summarization (llama-cpp-python)
# ===========================================================================

SUMMARY_PROMPT_TEMPLATE = """You are an expert meeting analyst. Summarize the
conversation transcript below. Respond in Markdown with EXACTLY these four
sections: Overview, Key Discussion Topics, Decisions Made, Action Items.
Use concise bullet points. Attribute statements to SPEAKER tags where relevant.
Do not invent content absent from the transcript.

Transcript:
{transcript}

Markdown summary:"""


def build_transcript_text(segments: list[dict], max_chars: int = 24_000) -> str:
    lines = []
    for seg in segments:
        speaker = seg.get("speaker", "UNKNOWN")
        text = " ".join(str(seg.get("text", "")).split())
        lines.append(f"[{seg.get('start', 0.0):07.2f}s] {speaker}: {text}")

    blob = "\n".join(lines)
    if len(blob) > max_chars:
        blob = blob[:max_chars] + "\n...[truncated]"
        log(f"Transcript truncated to {max_chars} chars for LLM context")
    return blob


def write_fallback_summary(
    out_path: Path, segments: list[dict], speaker_count: int, duration: float
) -> None:
    """Deterministic extractive summary when the LLM is unavailable.
    Guarantees summary.md always exists with the required four sections."""
    lines = ["# Overview", ""]
    lines.append(
        f"Recorded conversation of {duration:.1f} seconds across "
        f"{speaker_count} speaker(s), {len(segments)} segments."
    )
    lines += ["", "# Key Discussion Topics", ""]
    for seg in segments[:10]:
        text = " ".join(str(seg.get("text", "")).split())
        if text:
            lines.append(f"- ({seg.get('speaker', 'UNKNOWN')}) {text}")
    lines += ["", "# Decisions Made", "", "- (No explicit decisions detected.)"]
    lines += ["", "# Action Items", "", "- (No explicit action items detected.)"]
    out_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def stage_summarize(
    segments: list[dict],
    models_dir: Path,
    out_path: Path,
    speaker_count: int,
    duration: float,
    on_progress: Callable[[int], None],
) -> bool:
    """Generate summary.md via embedded llama.cpp. Returns True if the LLM
    produced it; False if the extractive fallback was used."""
    llm_dir = models_dir / "llm"
    gguf_path = llm_dir / DEFAULT_GGUF_NAME

    if not gguf_path.is_file():
        candidates = sorted(llm_dir.glob("*.gguf")) if llm_dir.is_dir() else []
        if not candidates:
            log(f"GGUF model not found at {gguf_path}; using extractive fallback.")
            write_fallback_summary(out_path, segments, speaker_count, duration)
            on_progress(PROGRESS["summarize_end"])
            return False
        gguf_path = candidates[0]

    from llama_cpp import Llama

    llm = Llama(
        model_path=str(gguf_path),
        n_ctx=4096,
        n_threads=max(1, (os_cpu_count()) - 1),
        verbose=False,
    )
    prompt = SUMMARY_PROMPT_TEMPLATE.format(transcript=build_transcript_text(segments))
    output = llm(
        prompt, max_tokens=1024, temperature=0.3, top_p=0.9,
        stop=["</s>", "Transcript:"], echo=False,
    )
    text = (output.get("choices") or [{}])[0].get("text", "").strip()
    release_vram(llm)

    if not text:
        write_fallback_summary(out_path, segments, speaker_count, duration)
        on_progress(PROGRESS["summarize_end"])
        return False

    normalized = text
    for required in ("Overview", "Key Discussion Topics", "Decisions Made", "Action Items"):
        if required.lower() not in normalized.lower():
            normalized += f"\n\n# {required}\n\n- (Not explicitly covered in the transcript.)"

    out_path.write_text(normalized + "\n", encoding="utf-8")
    on_progress(PROGRESS["summarize_end"])
    return True


def os_cpu_count() -> int:
    import os
    return os.cpu_count() or 4
