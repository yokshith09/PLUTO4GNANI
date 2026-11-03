import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckSquare, NotebookPen, Users } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Bear } from "@/features/shell/Bear";
import { useNotes, useTasksQuery } from "@/hooks/usePluto";
import { useSetting } from "@/hooks/useTheme";
import { ipc } from "@/ipc";
import { ROLES, ROLE_INFO, ROLE_PREF, parseRole, type Role } from "@/features/guide/roles";
import { TOUR_PREF, parseTour } from "@/features/guide/tourSteps";
import { usePref } from "@/features/guide/usePref";

const KEY = ["prefs", "ui.onboarded"] as const;

/**
 * Whether to show the welcome screen (S-00, F1-26): never finished it, and there is nothing in
 * Pluto yet. After Delete all the preferences are gone too, so it shows again. Someone who
 * already has notes or tasks (an upgrade) never sees it.
 */
export function useFirstRun() {
  const qc = useQueryClient();
  const onboarded = useQuery({ queryKey: KEY, queryFn: async () => (await ipc.prefsGet<boolean>("ui.onboarded")) === true });
  const notes = useNotes(false, null, "last_edited");
  const tasks = useTasksQuery("all");
  const loading = onboarded.isPending || notes.isPending || tasks.isPending;
  const show = !loading && !onboarded.data && (notes.data?.length ?? 0) === 0 && (tasks.data?.length ?? 0) === 0;
  const finish = async () => {
    qc.setQueryData(KEY, true);
    await ipc.prefsSet("ui.onboarded", true);
  };
  return { loading, show, finish };
}

export type WelcomeNext = "today" | "task" | "note" | "meeting";

function Choice({ label, hint, checked, onChange }: { label: string; hint: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex cursor-pointer items-start gap-3 py-3">
      <span className="min-w-0 flex-1">
        <span className="block text-[13px] font-medium">{label}</span>
        <span className="block text-xs text-muted-foreground">{hint}</span>
      </span>
      <Switch checked={checked} onCheckedChange={onChange} aria-label={label} />
    </label>
  );
}

/** One tap: who is using Pluto. It only decides the order of the tour and the quiz (FR-83). */
function RoleStep({ next, onDone }: { next: WelcomeNext; onDone: (next: WelcomeNext) => void }) {
  const role = usePref(ROLE_PREF, parseRole);
  const tour = usePref(TOUR_PREF, parseTour);
  const pick = async (r: Role) => {
    await role.set(r);
    // The tour starts by itself only when you land on Today; from a new task or note it waits in Settings.
    if (next === "today") await tour.set("pending");
    onDone(next);
  };
  return (
    <main className="grid min-h-screen place-items-center bg-background px-4 py-10 text-foreground">
      <div className="w-full max-w-xl">
        <div className="flex flex-col items-center text-center">
          <Bear pose="wave" animated className="size-24" />
          <h1 className="mt-3 text-2xl font-semibold tracking-tight">What describes you best?</h1>
          <p className="mt-1.5 text-[13.5px] text-muted-foreground">Pluto shows what matters to you first in the tour and the quiz. Nothing is hidden, and you can change this any time.</p>
        </div>
        <div role="group" aria-label="What describes you best" className="mt-6 grid gap-3 sm:grid-cols-2">
          {ROLES.map((r) => (
            <button
              key={r} type="button" onClick={() => void pick(r)}
              className="rounded-xl border bg-card p-4 text-left outline-none transition-colors hover:border-primary/60 focus-visible:ring-2 focus-visible:ring-ring"
            >
              <span className="block text-[13px] font-semibold">{ROLE_INFO[r].label}</span>
              <span className="block text-xs text-muted-foreground">{ROLE_INFO[r].blurb}</span>
            </button>
          ))}
        </div>
        <div className="mt-5 text-center">
          <Button variant="ghost" size="sm" onClick={() => void pick("general")}>Skip</Button>
        </div>
      </div>
    </main>
  );
}

/** The first thing a new person sees: hello, the core actions, and three choices. Once. */
export function Welcome({ onDone }: { onDone: (next: WelcomeNext) => void }) {
  const [next, setNext] = useState<WelcomeNext | null>(null);
  if (next) return <RoleStep next={next} onDone={onDone} />;
  return <WelcomeChoices onDone={setNext} />;
}

function WelcomeChoices({ onDone }: { onDone: (next: WelcomeNext) => void }) {
  const [tracking, setTracking] = useSetting("tracking.apps");
  const [autostart, setAutostart] = useSetting("general.autostart");
  const [pet, setPet] = useSetting("pet.visible");

  return (
    <main className="grid min-h-screen place-items-center bg-background px-4 py-10 text-foreground">
      <div className="w-full max-w-xl">
        <div className="flex flex-col items-center text-center">
          <Bear pose="wave" animated className="size-28" />
          <h1 className="mt-3 text-2xl font-semibold tracking-tight">Welcome to Pluto</h1>
          <p className="mt-1.5 text-[13.5px] text-muted-foreground">Your notes, tasks and time, kept on this computer. Start with one of these:</p>
        </div>

        <div className="mt-6 grid grid-cols-3 gap-3">
          <button
            type="button" onClick={() => onDone("task")}
            className="flex flex-col items-start gap-2 rounded-xl border bg-card p-4 text-left outline-none transition-colors hover:border-primary/60 focus-visible:ring-2 focus-visible:ring-ring"
          >
            <CheckSquare className="size-5 text-primary" />
            <span className="text-[13px] font-semibold">Add your first task</span>
            <span className="text-xs text-muted-foreground">What do you want to finish today?</span>
          </button>
          <button
            type="button" onClick={() => onDone("note")}
            className="flex flex-col items-start gap-2 rounded-xl border bg-card p-4 text-left outline-none transition-colors hover:border-primary/60 focus-visible:ring-2 focus-visible:ring-ring"
          >
            <NotebookPen className="size-5 text-primary" />
            <span className="text-[13px] font-semibold">Write a note</span>
            <span className="text-xs text-muted-foreground">Ideas, plans, anything worth keeping.</span>
          </button>
          <button
            type="button" onClick={() => onDone("meeting")}
            className="flex flex-col items-start gap-2 rounded-xl border bg-card p-4 text-left outline-none transition-colors hover:border-primary/60 focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Users className="size-5 text-primary" />
            <span className="text-[13px] font-semibold">Log a meeting</span>
            <span className="text-xs text-muted-foreground">Decisions and who does what next.</span>
          </button>
        </div>

        <section aria-label="Your choices" className="mt-6 divide-y rounded-xl border bg-card px-4">
          <Choice
            label="Show Pluto the bear on your desktop" hint="Hover for today at a glance; click to open Pluto."
            checked={pet ?? true} onChange={(v) => void setPet(v)}
          />
          <Choice
            label="Track app time" hint="Which app is in front, and for how long. Stays on this computer; window titles are never stored."
            checked={tracking ?? false} onChange={(v) => void setTracking(v)}
          />
          <Choice
            label="Open Pluto at login" hint="Starts quietly in the tray."
            checked={autostart ?? false} onChange={(v) => void setAutostart(v)}
          />
        </section>

        <div className="mt-6 flex items-center justify-between">
          <p className="text-xs text-muted-foreground">You can change these any time in Settings.</p>
          <Button onClick={() => onDone("today")}>Start</Button>
        </div>
      </div>
    </main>
  );
}
