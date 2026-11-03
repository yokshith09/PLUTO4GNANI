import { useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useBusSync } from "@/hooks/usePluto";
import { useTheme } from "@/hooks/useTheme";
import { isTauri } from "@tauri-apps/api/core";
import { NoteEditor } from "./NoteEditor";

/** The id of the note this window shows, if it is a note window (set by the shell, or `?note=` in a browser). */
export function noteWindowId(): string | null {
  const fromShell = (window as unknown as { __PLUTO_NOTE__?: unknown }).__PLUTO_NOTE__;
  if (typeof fromShell === "string" && fromShell) return fromShell;
  return new URLSearchParams(window.location.search).get("note");
}

async function closeWindow() {
  if (isTauri()) {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  } else {
    window.close();
  }
}

function Body({ id }: { id: string }) {
  useBusSync();
  useTheme();
  return <div className="h-screen bg-background text-foreground"><NoteEditor noteId={id} onClose={() => void closeWindow()} fullWindow /></div>;
}

/**
 * A note in a window of its own (owner 2026-10-25). Only the note: none of the dashboard's daily
 * jobs (greeting, reminders, auto-delete) run here, so nothing happens twice.
 */
export function NoteWindow({ id }: { id: string }) {
  const [client] = useState(() => new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: false } } }));
  return <QueryClientProvider client={client}><Body id={id} /></QueryClientProvider>;
}
