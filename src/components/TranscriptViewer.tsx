// src/components/TranscriptViewer.tsx
// Groups contiguous utterances by speaker tag and renders individual words
// with timestamp tooltips.

import { useMemo } from "react";
import type { TranscriptSegment } from "../hooks/useTauriBridge";

const SPEAKER_COLORS = [
  "text-sky-300",
  "text-emerald-300",
  "text-amber-300",
  "text-rose-300",
  "text-violet-300",
];

function speakerColor(speaker: string): string {
  const index = Math.abs(
    [...speaker].reduce((acc, c) => acc + c.charCodeAt(0), 0),
  ) % SPEAKER_COLORS.length;
  return SPEAKER_COLORS[index];
}

type Utterance = {
  speaker: string;
  start: number;
  end: number;
  text: string;
  words: TranscriptSegment["words"];
};

function groupBySpeaker(segments: TranscriptSegment[]): Utterance[] {
  const grouped: Utterance[] = [];

  for (const seg of segments) {
    const last = grouped[grouped.length - 1];

    if (last && last.speaker === seg.speaker && seg.start - last.end < 0.8) {
      last.text = `${last.text} ${seg.text}`.trim();
      last.end = seg.end;
      last.words = [...(last.words ?? []), ...(seg.words ?? [])];
    } else {
      grouped.push({
        speaker: seg.speaker || "UNKNOWN",
        start: seg.start,
        end: seg.end,
        text: seg.text,
        words: seg.words ?? [],
      });
    }
  }

  return grouped;
}

export default function TranscriptViewer({
  segments,
}: {
  segments: TranscriptSegment[];
}) {
  const utterances = useMemo(() => groupBySpeaker(segments), [segments]);

  if (segments.length === 0) {
    return (
      <p className="text-sm text-slate-500">
        No transcript available for this session yet.
      </p>
    );
  }

  return (
    <div className="space-y-4 text-sm">
      {utterances.map((utterance, index) => (
        <div key={`${utterance.start}-${index}`}>
          <div className="mb-1 flex items-baseline gap-2">
            <span
              className={`font-mono text-xs ${speakerColor(utterance.speaker)}`}
            >
              {utterance.start.toFixed(1)}s
            </span>
            <span
              className={`font-medium ${speakerColor(utterance.speaker)}`}
            >
              {utterance.speaker}
            </span>
          </div>

          <p className="leading-relaxed text-slate-300">
            {utterance.words && utterance.words.length > 0
              ? utterance.words.map((word, wi) => (
                  <span
                    key={`${word.start}-${wi}`}
                    title={`${word.start.toFixed(2)}s – ${word.end.toFixed(2)}s`}
                    className="cursor-default border-b border-dotted border-slate-700"
                  >
                    {word.word}{" "}
                  </span>
                ))
              : utterance.text}
          </p>
        </div>
      ))}
    </div>
  );
}