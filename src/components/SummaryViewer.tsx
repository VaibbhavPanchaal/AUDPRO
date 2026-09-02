// src/components/SummaryViewer.tsx — Markdown summary with export actions.

import { Clipboard, Download } from "lucide-react";
import ReactMarkdown from "react-markdown";

export default function SummaryViewer({
  summary,
  sessionId,
}: {
  summary: string | null;
  sessionId: string;
}) {
  const exportMarkdown = () => {
    if (!summary) return;

    const blob = new Blob([summary], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");

    anchor.href = url;
    anchor.download = `${sessionId}.md`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  const copySummary = async () => {
    if (summary) await navigator.clipboard.writeText(summary);
  };

  if (!summary) {
    return (
      <p className="text-sm text-slate-500">
        No summary generated for this session yet.
      </p>
    );
  }

  return (
    <div>
      <div className="mb-4 flex gap-2">
        <button
          onClick={() => void copySummary()}
          className="flex items-center gap-2 rounded-lg border border-slate-700 px-3 py-1.5 text-xs text-slate-300 transition hover:border-sky-400 hover:text-sky-300"
        >
          <Clipboard size={14} /> Copy
        </button>
        <button
          onClick={exportMarkdown}
          className="flex items-center gap-2 rounded-lg border border-slate-700 px-3 py-1.5 text-xs text-slate-300 transition hover:border-sky-400 hover:text-sky-300"
        >
          <Download size={14} /> Export .md
        </button>
      </div>

      <article className="prose prose-invert max-w-none text-sm">
        <ReactMarkdown>{summary}</ReactMarkdown>
      </article>
    </div>
  );
}