import { useEffect, useRef, useState } from "react";
import { ipc, onAutoRecording, onPetAsk, type AutoRec, type PetAsk } from "@/ipc";
import { cn } from "@/lib/utils";

/**
 * Google Meet auto mode on the pet: the question ("Record this meeting?") and, while recording,
 * how long it has run with a Stop button. The shell decides when to ask; the pet only shows it
 * and sends the answer back.
 */
export function useAutoMeet(onAsk: () => void) {
  const [asks, setAsks] = useState<PetAsk[]>([]);
  const [rec, setRec] = useState<AutoRec | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const wake = useRef(onAsk);
  wake.current = onAsk;

  useEffect(
    () => onPetAsk(
      (a) => { wake.current(); setAsks((q) => [...q.filter((x) => x.id !== a.id), a]); },
      (id) => setAsks((q) => q.filter((x) => x.id !== id)),
    ),
    [],
  );
  // After a reload the recording may already be running.
  useEffect(() => {
    void ipc.autoMeetingState().then((r) => setRec(r), () => undefined);
    return onAutoRecording(setRec);
  }, []);
  useEffect(() => {
    if (!rec) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [rec]);

  const ask = asks[0];
  return {
    ask,
    answer: (a: PetAsk, choice: string) => {
      setAsks((q) => q.filter((x) => x.id !== a.id));
      void (a.kind === "proposal" ? ipc.petProposalAnswer(a.id, choice) : ipc.autoMeetingAnswer(a.id, choice));
    },
    rec,
    elapsedMs: rec ? Math.max(0, now - rec.started_at) : 0,
    stop: () => void ipc.autoMeetingStop(),
  };
}

/** "12:03", or "1:02:03" after an hour. */
export function elapsedText(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}

/** A question with its buttons; the first is the main one. */
export function AskCard({ ask, width, onAnswer }: { ask: PetAsk; width: number; onAnswer: (choice: string) => void }) {
  return (
    <section role="alertdialog" aria-label={ask.title} className="pet-bubble-in flex shrink-0 flex-col gap-3 rounded-2xl border bg-popover p-4 text-popover-foreground shadow-lg" style={{ width }}>
      <strong className="text-[14px] leading-snug">{ask.title}</strong>
      <p className="text-[13px] leading-relaxed text-muted-foreground">{ask.detail}</p>
      <div className="flex gap-2">
        {ask.options.map((o, i) => (
          <button
            key={o} type="button" onClick={() => onAnswer(o)}
            className={cn("min-h-9 flex-1 rounded-lg px-2 text-[13px] font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring", i === 0 ? "bg-primary text-primary-foreground hover:bg-primary/90" : "border hover:bg-accent")}
          >
            {o}
          </button>
        ))}
      </div>
    </section>
  );
}

/** Shown under the pet's card while a call is being recorded. */
export function RecordChip({ width, elapsed, onStop }: { width: number; elapsed: string; onStop: () => void }) {
  return (
    <div className="flex shrink-0 items-center justify-between gap-2 rounded-xl border bg-popover px-2.5 py-1.5 text-popover-foreground shadow-md" style={{ width }} role="status" aria-label={`Recording ${elapsed}`}>
      <span className="flex items-center gap-1.5 text-[12px] font-semibold"><span className="size-2 animate-pulse rounded-full bg-destructive" />Recording {elapsed}</span>
      <button type="button" onClick={onStop} className="rounded-md border px-2 py-0.5 text-[12px] font-medium outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring">Stop</button>
    </div>
  );
}
