import { useTauriBridge } from "./hooks/useTauriBridge";
import Sidebar from "./components/Sidebar";
import WorkspacePanel from "./components/WorkspacePanel";

export default function App() {
  const bridge = useTauriBridge();

  return (
    <main className="min-h-screen bg-slate-950 p-6 text-slate-100">
      <div className="mx-auto grid max-w-7xl grid-cols-[minmax(340px,0.9fr)_minmax(520px,1.4fr)] gap-6">
        <Sidebar
          devices={bridge.devices}
          device={bridge.device}
          setDevice={bridge.setDevice}
          status={bridge.status}
          elapsed={bridge.elapsed}
          sessions={bridge.sessions}
          selectedId={bridge.selected?.id ?? null}
          onOpenSession={(id) => void bridge.openSession(id)}
          onOpenFolder={(id) => void bridge.openSessionFolder(id)}
          onToggleRecording={() => {
            if (bridge.status === "recording") void bridge.stopRecording();
            else void bridge.startRecording();
          }}
        />



        <WorkspacePanel
          stage={bridge.stage}
          progress={bridge.progress}
          progressMessage={bridge.progressMessage}
          isBusy={bridge.isBusy}
          status={bridge.status}
          error={bridge.error}
          selected={bridge.selected}
        />
      </div>
    </main>
  );
}
