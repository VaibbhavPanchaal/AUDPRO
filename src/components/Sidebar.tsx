// src/components/Sidebar.tsx — device picker, record controls, session history.

import { CalendarDays, FolderOpen, Languages, Mic, Square } from "lucide-react";
import type { ProcessingStatus, SessionSummary } from "../hooks/useTauriBridge";

function formatClock(totalSeconds: number): string {
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function formatDuration(seconds: number): string {
  if (seconds >= 3600) {
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return `${h}h ${m}m`;
  }
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

export default function Sidebar({
  devices,
  device,
  setDevice,
  status,
  elapsed,
  sessions,
  selectedId,
  onOpenSession,
  onOpenFolder,
  onToggleRecording,
}: {
  devices: string[];
  device: string;
  setDevice: (value: string) => void;
  status: ProcessingStatus;
  elapsed: number;
  sessions: SessionSummary[];
  selectedId: string | null;
  onOpenSession: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onToggleRecording: () => void;
}) {
  const isRecording = status === "recording";
  const busy =
    isRecording ||
    status === "queued" ||
    status === "transcribing" ||
    status === "aligning" ||
    status === "diarizing" ||
    status === "summarizing";

  return (
    <aside className="flex min-w-0 flex-col gap-6 rounded-3xl border border-slate-800 bg-slate-900 p-6 shadow-2xl">
      <header>
        <p className="text-sm font-medium text-sky-400">LOCAL AI WORKSPACE</p>
        <h1 className="mt-2 text-2xl font-semibold tracking-tight text-slate-100">
          Offline Speech Intelligence
        </h1>
      </header>

      <section>
        <label
          htmlFor="device-select"
          className="mb-2 block text-sm text-slate-400"
        >
          Microphone input
        </label>
        <select
          id="device-select"
          value={device}
          onChange={(event) => setDevice(event.target.value)}
          disabled={busy}
          className="w-full rounded-xl border border-slate-700 bg-slate-950 px-4 py-3 text-slate-100 outline-none focus:border-sky-400 disabled:opacity-50"
        >
          {devices.length === 0 && <option>No input devices found</option>}
          {devices.map((item) => (
            <option key={item} value={item}>
              {item}
            </option>
          ))}
        </select>
      </section>

      <div className="flex items-center justify-between text-sm">
        <span className="text-slate-400">Status</span>
        <span className="flex items-center gap-2 text-slate-100">
          <span
            className={`h-2 w-2 rounded-full ${
              isRecording
                ? "animate-pulse bg-red-400"
                : busy
                  ? "bg-amber-400"
                  : status === "failed"
                    ? "bg-red-500"
                    : "bg-emerald-400"
            }`}
          />
          {isRecording ? `Recording ${formatClock(elapsed)}` : status}
        </span>
      </div>

      <button
        onClick={onToggleRecording}
        disabled={busy && !isRecording}
        className={`flex w-full items-center justify-center gap-3 rounded-xl px-4 py-4 font-medium text-white transition ${
          isRecording
            ? "bg-red-500 hover:bg-red-400"
            : "bg-sky-500 hover:bg-sky-400"
        } disabled:cursor-not-allowed disabled:opacity-50`}
      >
        {isRecording ? <Square size={18} /> : <Mic size={18} />}
        {isRecording ? "Stop recording" : "Start recording"}
      </button>

      <section className="min-h-0">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="font-semibold text-slate-100">Session history</h2>
          <span className="text-xs text-slate-500">
            {sessions.length} local
          </span>
        </div>

        <div className="max-h-72 space-y-2 overflow-auto pr-1">
          {sessions.length === 0 && (
            <p className="rounded-xl bg-slate-950/60 p-4 text-sm text-slate-500">
              No sessions yet. Record something to get started.
            </p>
          )}

          {sessions.map((session) => (
            <div
              key={session.id}
              className={`group flex items-center justify-between rounded-2xl p-3 transition ${
                selectedId === session.id
                  ? "bg-sky-500/15 ring-1 ring-sky-400/50"
                  : "hover:bg-slate-800"
              }`}
            >
              <button
                onClick={() => onOpenSession(session.id)}
                className="min-w-0 flex-1 text-left"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate font-medium text-slate-100">
                    {session.id}
                  </span>
                  <span className="text-xs text-slate-500">
                    {formatDuration(session.duration_seconds)}
                  </span>
                </div>
                <div className="mt-1 flex gap-3 text-xs text-slate-400">
                  <span className="flex items-center gap-1">
                    <CalendarDays size={12} />
                    {new Date(session.created_at).toLocaleDateString()}
                  </span>
                  {session.language && (
                    <span className="flex items-center gap-1">
                      <Languages size={12} />
                      {session.language}
                    </span>
                  )}
                  <span
                    className={
                      session.status === "completed"
                        ? "text-emerald-400"
                        : "text-amber-400"
                    }
                  >
                    {session.status}
                  </span>
                </div>
              </button>
              <button
                onClick={() => onOpenFolder(session.id)}
                title="Open folder"
                className="ml-2 rounded-lg p-2 text-slate-500 opacity-0 transition hover:bg-slate-700 hover:text-slate-200 group-hover:opacity-100"
              >
                <FolderOpen size={15} />
              </button>
            </div>
          ))}
        </div>
      </section>
    </aside>
  );
}