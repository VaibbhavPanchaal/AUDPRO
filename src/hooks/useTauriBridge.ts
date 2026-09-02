// src/hooks/useTauriBridge.ts — React bridge over Tauri IPC.
// Types, session helpers, recording actions, and the sidecar event stream.

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---------------------------------------------------------------------------
// Section 1: types (mirrored from Rust session store / sidecar protocol)
// ---------------------------------------------------------------------------

export type TranscriptSegment = {
  start: number;
  end: number;
  text: string;
  speaker: string;
  words?: { word: string; start: number; end: number; score?: number }[];
};

export type SessionSummary = {
  id: string;
  created_at: string;
  duration_seconds: number;
  speaker_count: number;
  status: string;
  language: string | null;
  has_summary: boolean;
  has_transcript: boolean;
};

export type SessionDetail = {
  id: string;
  metadata: {
    uuid: string;
    timestamp: string;
    duration_seconds: number;
    speaker_count: number;
    status: string;
    language: string | null;
  };
  audio_path: string;
  summary: string | null;
  transcript: { segments: TranscriptSegment[] } | null;
};

export type PipelineStage =
  | "probe"
  | "transcribe"
  | "align"
  | "diarize"
  | "summarize"
  | "complete";

export type SidecarEvent =
  | { type: "stage"; stage: string; message: string }
  | { type: "progress"; stage: string; progress: number; message: string }
  | {
      type: "done";
      session_id: string;
      duration_seconds: number;
      speaker_count: number;
      language: string | null;
    }
  | { type: "error"; stage: string | null; message: string }
  | { type: "raw"; message: string };

export type ProcessingStatus =
  | "idle"
  | "recording"
  | "queued"
  | "transcribing"
  | "aligning"
  | "diarizing"
  | "summarizing"
  | "completed"
  | "failed";

const STAGE_TO_STATUS: Record<string, ProcessingStatus> = {
  probe: "queued",
  transcribe: "transcribing",
  align: "aligning",
  diarize: "diarizing",
  summarize: "summarizing",
  complete: "completed",
};

// ---------------------------------------------------------------------------
// Section 2: hook
// ---------------------------------------------------------------------------

export function useTauriBridge() {
  const [devices, setDevices] = useState<string[]>([]);
  const [device, setDevice] = useState("");
  const [status, setStatus] = useState<ProcessingStatus>("idle");
  const [stage, setStage] = useState<PipelineStage | "idle">("idle");
  const [progress, setProgress] = useState(0);
  const [progressMessage, setProgressMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selected, setSelected] = useState<SessionDetail | null>(null);
  const [elapsed, setElapsed] = useState(0);

  const recordingTimer = useRef<ReturnType<typeof setInterval> | null>(null);
  const activeSessionRef = useRef<string | null>(null);

  // ---------------- sessions ----------------

  const refreshSessions = useCallback(async () => {
    try {
      setSessions(await invoke<SessionSummary[]>("list_sessions"));
    } catch (err) {
      console.error("list_sessions failed:", err);
      setSessions([]);
    }
  }, []);

  const openSession = useCallback(async (id: string) => {
    try {
      setSelected(await invoke<SessionDetail>("get_session", { sessionId: id }));
    } catch (err) {
      console.error("get_session failed:", err);
    }
  }, []);

  const openSessionFolder = useCallback(async (id: string) => {
    try {
      await invoke("open_session_folder", { sessionId: id });
    } catch (err) {
      console.error("open_session_folder failed:", err);
    }
  }, []);

  // ---------------- recording ----------------

  const startRecording = useCallback(async () => {
    setError(null);
    const id = `session-${Date.now()}`;
    await invoke("start_recording", { sessionId: id, device: device || null });
    activeSessionRef.current = id;
    setStatus("recording");
    setStage("idle");
    setProgress(0);
    setProgressMessage("");
    setElapsed(0);
    recordingTimer.current = setInterval(() => setElapsed((e) => e + 1), 1000);
  }, [device]);

  const stopRecording = useCallback(async () => {
    if (recordingTimer.current) {
      clearInterval(recordingTimer.current);
      recordingTimer.current = null;
    }

    try {
      const id = await invoke<string>("stop_recording");
      setStatus("queued");
      setStage("probe");
      setProgressMessage("Waiting for sidecar…");

      await invoke("process_audio", {
        sessionId: id,
        modelsDir: null,
        language: null,
        device: null,
      });
    } catch (err) {
      setStatus("failed");
      setError(String(err));
      void refreshSessions();
    }
  }, [refreshSessions]);

  const cancelProcessing = useCallback(async () => {
    try {
      await invoke("cancel_processing");
    } catch (err) {
      console.error("cancel_processing failed:", err);
    }
  }, []);

  // ---------------- sidecar event stream ----------------

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    void listen<SidecarEvent>("processing-progress", (event) => {
      const e = event.payload;

      switch (e.type) {
        case "stage":
          setStage(e.stage as PipelineStage);
          setStatus(STAGE_TO_STATUS[e.stage] ?? "queued");
          setProgressMessage(e.message || e.stage);
          break;

        case "progress":
          setStage(e.stage as PipelineStage);
          setProgress(e.progress);
          setStatus(STAGE_TO_STATUS[e.stage] ?? "queued");
          setProgressMessage(e.message);
          break;

        case "done": {
          setProgress(100);
          setStatus("completed");
          setStage("complete");
          const finished = e.session_id || activeSessionRef.current;
          activeSessionRef.current = null;
          if (finished) {
            void refreshSessions().then(() => openSession(finished));
          }
          break;
        }

        case "error":
          setStatus("failed");
          setError(e.message);
          void refreshSessions();
          break;

        default:
          break;
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshSessions, openSession]);

  // ---------------- initial load ----------------

  useEffect(() => {
    void invoke<string[]>("list_input_devices")
      .then((items) => {
        setDevices(items);
        setDevice(items[0] ?? "");
      })
      .catch((err) => console.error("list_input_devices failed:", err));

    void refreshSessions();

    return () => {
      if (recordingTimer.current) clearInterval(recordingTimer.current);
    };
  }, [refreshSessions]);

  const isBusy =
    status === "recording" ||
    status === "queued" ||
    status === "transcribing" ||
    status === "aligning" ||
    status === "diarizing" ||
    status === "summarizing";

  return {
    devices,
    device,
    setDevice,
    status,
    isBusy,
    stage,
    progress,
    progressMessage,
    error,
    elapsed,
    sessions,
    selected,
    openSession,
    openSessionFolder,
    refreshSessions,
    startRecording,
    stopRecording,
    cancelProcessing,
    setError,
  };
}
