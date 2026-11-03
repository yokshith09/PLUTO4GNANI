import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import type { Page } from "@/features/shell/Sidebar";
import type { TourStep } from "./tourSteps";

const PAD = 6;
const CARD_W = 340;

interface Box { left: number; top: number; width: number; height: number }

function measure(target: string | undefined): Box | null {
  if (!target) return null;
  const el = document.querySelector(`[data-tour="${target}"]`);
  if (!el) return null;
  let r = el.getBoundingClientRect();
  // The page we came from may have been scrolled: bring the thing into view before pointing at it.
  if (r.top < 0 || r.bottom > window.innerHeight) {
    el.scrollIntoView({ block: "center" });
    r = el.getBoundingClientRect();
  }
  if (r.width === 0 || r.height === 0) return null;
  return { left: r.left - PAD, top: r.top - PAD, width: r.width + PAD * 2, height: r.height + PAD * 2 };
}

/** Where the card goes: beside the spotlight (right of a sidebar item, below a button), kept on screen. */
function place(box: Box | null, cardH: number): { left: number; top: number } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  if (!box) return { left: Math.max(16, (vw - CARD_W) / 2), top: Math.max(16, (vh - cardH) / 2) };
  const keep = (top: number) => Math.min(Math.max(16, top), Math.max(16, vh - cardH - 16));
  const roomRight = vw - (box.left + box.width) - 16;
  if (roomRight >= CARD_W) return { left: box.left + box.width + 16, top: keep(box.top) };
  const left = Math.min(Math.max(16, box.left + box.width - CARD_W), vw - CARD_W - 16);
  const below = box.top + box.height + 16;
  return { left, top: keep(below + cardH <= vh - 16 ? below : box.top - cardH - 16) };
}

/**
 * A spotlight tour over the real screens (owner 2026-10-28). It dims the page, lights the one
 * thing the step is about and says what it is for. Next / Back / Skip, Escape skips, and focus
 * stays inside the card so the keyboard and a screen reader get the same tour.
 */
export function Tour({ steps, onNavigate, onClose }: { steps: TourStep[]; onNavigate: (p: Page) => void; onClose: (finished: boolean) => void }) {
  const [i, setI] = useState(0);
  const [box, setBox] = useState<Box | null>(null);
  const [cardH, setCardH] = useState(200);
  const card = useRef<HTMLDivElement>(null);
  const next = useRef<HTMLButtonElement>(null);
  const step = steps[i];
  const last = i === steps.length - 1;

  // Go to the page the step is about, then find its target once it has been drawn.
  useEffect(() => {
    if (!step) return;
    onNavigate(step.page);
  }, [step, onNavigate]);

  useLayoutEffect(() => {
    if (!step) return;
    const update = () => {
      setBox(measure(step.target));
      setCardH(card.current?.offsetHeight ?? 200);
    };
    update();
    const timer = window.setTimeout(update, 250); // the page it opened may still be drawing
    window.addEventListener("resize", update);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("resize", update);
    };
  }, [step, i]);

  useEffect(() => next.current?.focus(), [i]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") { e.stopPropagation(); onClose(false); }
    if (e.key === "Tab" && card.current) {
      const items = [...card.current.querySelectorAll<HTMLElement>("button")];
      const first = items[0];
      const final = items[items.length - 1];
      if (!first || !final) return;
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); final.focus(); }
      else if (!e.shiftKey && document.activeElement === final) { e.preventDefault(); first.focus(); }
    }
  };

  if (!step) return null;
  const pos = place(box, cardH);
  return (
    <div className="fixed inset-0 z-[70]" onKeyDown={onKey}>
      {/* The page behind is covered so a stray click does nothing; the lit hole is only a picture. */}
      <div className="absolute inset-0" aria-hidden />
      {box ? (
        <div aria-hidden className="pointer-events-none absolute rounded-lg ring-2 ring-primary transition-all duration-200 [box-shadow:0_0_0_9999px_rgba(0,0,0,0.55)]" style={{ left: box.left, top: box.top, width: box.width, height: box.height }} />
      ) : (
        <div aria-hidden className="absolute inset-0 bg-black/55" />
      )}
      <div
        ref={card} role="dialog" aria-modal="true" aria-label="Pluto tour" aria-describedby="tour-body"
        className="absolute w-[340px] max-w-[calc(100vw-32px)] rounded-xl border bg-popover p-4 text-popover-foreground shadow-xl"
        style={{ left: pos.left, top: pos.top }}
      >
        <p className="text-[11px] font-medium text-muted-foreground" aria-live="polite">Step {i + 1} of {steps.length}</p>
        <h2 className="mt-1 text-sm font-semibold">{step.title}</h2>
        <p id="tour-body" className="mt-1.5 text-[13px] leading-5 text-muted-foreground">{step.body}</p>
        <div className="mt-4 flex items-center justify-between">
          <Button variant="ghost" size="sm" onClick={() => onClose(false)}>Skip tour</Button>
          <div className="flex gap-1.5">
            {i > 0 && <Button variant="outline" size="sm" onClick={() => setI(i - 1)}>Back</Button>}
            <Button ref={next} size="sm" onClick={() => (last ? onClose(true) : setI(i + 1))}>{last ? "Done" : "Next"}</Button>
          </div>
        </div>
      </div>
    </div>
  );
}
