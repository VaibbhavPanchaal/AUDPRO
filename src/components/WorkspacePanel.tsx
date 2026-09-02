// src/components/WorkspacePanel.tsx — right pane: progress, tabs, views.

import { useState } from "react";
import { FileText, ScrollText } from "lucide-react";
import type {
  PipelineStage,
  ProcessingStatus,
  SessionDetail,
} from "../hooks/useTauriBridge";
import ProgressBar from "./ProgressBar";
import TranscriptViewer from "./TranscriptViewer";
import SummaryViewer from "./SummaryViewer";

type Tab = "transcript" | "summary";

function convertFileSrc(path: string): string {
  return `asset://localhost/${encodeURIComponent(path)}`;
}

export default function WorkspacePanel(props: {
  stage: PipelineStage | "idle";
  progress: number;
  progressMessage: string;
  isBusy: boolean;
  status: ProcessingStatus;
  error: string | null;
  selected: SessionDetail | null;
}) {
  const [tab, setTab] = useState<Tab>("summary");
  const segments = props.selected?.transcript?.segments ?? [];

  return (
    <section className="min-w-0 rounded-3xl border border-slate-800 bg-slate-900 p-6 shadow-2xl">
      <ProgressBar
        stage={props.stage}
        progress={props.progress}
        message={props.progressMessage}
        visible={props.isBusy || props.status === "completed" || props.status === "failed"}
      />

      {props.error && (
        <div className="mt-4 rounded-xl border border-red-500/40 bg-red-500/10 p-4 text-sm text-red-300">
          {props.error}
        </div>
      )}

      {props.selected ? (
        <div className="mt-6">
          <header className="mb-4">
            <h2 className="text-xl font-semibold">{props.selected.id}</h2>
            <p className="text-sm text-slate-400">
              {props.selected.metadata.speaker_count} speaker(s) ·{" "}
              {props.selected.metadata.duration_seconds.toFixed(1)}s ·{" "}
              {props.selected.metadata.status}
            </p>
          </header>

          {props.selected.audio_path && (
            <audio
              controls
              src={convertFileSrc(props.selected.audio_path)}
              className="mb-6 w-full"
            />
          )}

          <div className="mb-4 flex gap-2 border-b border-slate-800">
            <button
              onClick={() => setTab("summary")}
              className={`flex items-center gap-2 px-4 py-2 text-sm font-medium transition ${
                tab === "summary"
                  ? "border-b-2 border-sky-400 text-sky-300"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              <ScrollText size={15} /> Summary
            </button>
            <button
              onClick={() => setTab("transcript")}
              className={`flex items-center gap-2 px-4 py-2 text-sm font-medium transition ${
                tab === "transcript"
                  ? "border-b-2 border-sky-400 text-sky-300"
                  : "text-slate-400 hover:text-slate-200"
              }`}
            >
              <FileText size={15} /> Transcript
            </button>
          </div>

          <div className="max-h-[52vh] overflow-auto rounded-2xl bg-slate-950/60 p-5">
            {tab === "summary" ? (
              <SummaryViewer
                summary={props.selected.summary}
                sessionId={props.selected.id}
              />
            ) : (
              <TranscriptViewer segments={segments} />
            )}
          </div>
        </div>
      ) : (
        <div className="flex h-80 flex-col items-center justify-center text-center text-slate-500">
          <p>Select a session or record a new one to see results.</p>
        </div>
      )}
    </section>
  );
}
