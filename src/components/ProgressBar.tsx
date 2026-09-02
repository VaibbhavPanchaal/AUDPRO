// src/components/ProgressBar.tsx — animated stage-aware progress bar.

import type { PipelineStage } from "../hooks/useTauriBridge";

const STAGE_LABELS: Record<string, string> = {
  probe: "Initializing",
  transcribe: "Transcribing",
  align: "Aligning phonemes",
  diarize: "Diarizing speakers",
  summarize: "Summarizing",
  complete: "Complete",
};

export default function ProgressBar({
  stage,
  progress,
  message,
  visible,
}: {
  stage: PipelineStage | "idle";
  progress: number;
  message: string;
  visible: boolean;
}) {
  if (!visible) return null;

  const label = STAGE_LABELS[stage] ?? "Working";

  return (
    <div className="mt-5">
      <div className="mb-2 flex justify-between text-xs text-slate-400">
        <span>{message || label}</span>
        <span>{Math.round(progress)}%</span>
      </div>
      <div
        className="h-2 overflow-hidden rounded-full bg-slate-800"
        role="progressbar"
        aria-valuenow={Math.round(progress)}
        aria-valuemin={0}
        aria-valuemax={100}
      >
        <div
          className="h-full rounded-full bg-sky-400 transition-all duration-300"
          style={{ width: `${Math.max(2, progress)}%` }}
        />
      </div>
    </div>
  );
}
