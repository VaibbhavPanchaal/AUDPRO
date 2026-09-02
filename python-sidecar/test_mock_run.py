#!/usr/bin/env python3
"""python-sidecar/test_mock_run.py

Standalone test runner for the sidecar pipeline. Generates a dummy WAV
file, runs the pipeline in mock mode via the one-shot CLI, and validates:

  1. Event ordering (READY/STAGE/PROGRESS lines parse as JSON).
  2. The COMPLETE event reports success.
  3. transcript.json, summary.md, metadata.json exist with the schema.

Run:  .venv/bin/python test_mock_run.py
"""

from __future__ import annotations

import json
import math
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

HERE = Path(__file__).resolve().parent


def make_dummy_wav(path: Path, seconds: float = 2.0, rate: int = 16_000) -> float:
    """16 kHz mono 16-bit sine sweep — a valid but meaningless WAV."""
    frames = []
    for i in range(int(rate * seconds)):
        sample = int(8000 * math.sin(2 * math.pi * 440 * i / rate))
        frames.append(max(-32768, min(32767, sample)))

    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(rate)
        handle.writeframes(b"".join(f.to_bytes(2, "little", signed=True) for f in frames))

    return seconds


def main() -> int:
    tmp = Path(tempfile.mkdtemp(prefix="sidecar-mock-"))
    audio = tmp / "audio.wav"
    out_dir = tmp / "session"
    out_dir.mkdir()

    make_dummy_wav(audio)
    print(f"dummy wav: {audio}")

    result = subprocess.run(
        [
            sys.executable,
            str(HERE / "sidecar_entry.py"),
            "--audio", str(audio),
            "--output-dir", str(out_dir),
            "--session-id", "mock-test-001",
            "--mock",
        ],
        capture_output=True,
        text=True,
        timeout=60,
    )

    events = []
    failures: list[str] = []

    for line in result.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            failures.append(f"non-JSON stdout line: {line!r}")

    names = [e.get("event") for e in events]

    if "COMPLETE" not in names:
        failures.append(f"no COMPLETE event; got {names}")
    if "ERROR" in names:
        failures.append("unexpected ERROR event")

    stages_seen = [e["payload"]["stage"] for e in events if e.get("event") == "STAGE"]
    for expected in ("transcribe", "align", "diarize", "summarize"):
        if expected not in stages_seen:
            failures.append(f"missing stage: {expected}")

    complete = next((e for e in events if e.get("event") == "COMPLETE"), None)
    if complete and not complete["payload"].get("mock"):
        failures.append("COMPLETE payload should flag mock=true")

    transcript = out_dir / "transcript.json"
    summary = out_dir / "summary.md"
    metadata = out_dir / "metadata.json"

    for artifact in (transcript, summary, metadata):
        if not artifact.is_file():
            failures.append(f"missing artifact: {artifact.name}")

    if transcript.is_file():
        data = json.loads(transcript.read_text(encoding="utf-8"))
        segs = data.get("segments", [])
        if len(segs) != 3:
            failures.append(f"expected 3 segments, got {len(segs)}")
        if not all("speaker" in s and "start" in s for s in segs):
            failures.append("segments missing speaker/start fields")

    if metadata.is_file():
        meta = json.loads(metadata.read_text(encoding="utf-8"))
        for key in ("uuid", "timestamp", "duration_seconds", "speaker_count", "status"):
            if key not in meta:
                failures.append(f"metadata missing key: {key}")
        if meta.get("status") != "completed":
            failures.append(f"metadata status={meta.get('status')!r}")

    if summary.is_file():
        text = summary.read_text(encoding="utf-8")
        for section in ("Overview", "Key Discussion Topics", "Decisions Made", "Action Items"):
            if f"# {section}" not in text:
                failures.append(f"summary.md missing section: {section}")

    if result.stderr:
        print("--- stderr ---")
        print(result.stderr.strip())

    if failures:
        print("\nFAILURES:")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print("\nMOCK RUN PASSED")
    print(f"  events:   {len(events)} lines, ordered stages {stages_seen}")
    print(f"  artifacts: transcript.json, summary.md, metadata.json in {out_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())