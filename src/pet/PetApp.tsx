import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { BEAR_POSES, Bear, type BearPose } from "@/features/shell/Bear";
import { ROW_FOR_POSE, SpritePet } from "@/features/character/SpritePet";
import { useReminders, useSettings, useTodayTasks } from "@/hooks/usePluto";
import { useFocus } from "@/hooks/useFocus";
import { clock } from "@/lib/focus";
import { ymd } from "@/lib/dates";
import { useTheme } from "@/hooks/useTheme";
import { ipc, onEvent, onPetCard, onPetMenu, onPetSay, petAction, petCardSend, type TaskRow } from "@/ipc";
import { cn } from "@/lib/utils";
import { PetCard } from "./PetCard";
import { glanceText } from "./glance";
import { WARDROBE_KEY, useWorn } from "@/features/wardrobe/useWardrobe";
import { enqueue, isAsleep, reactionForSay, type Reaction } from "./reactions";
import { parseReactionSettings, reactionWithSettings } from "./reactionSettings";
import { NO_COUNTS, advance, reactionForSpecial, type SpecialSignal } from "./special";
import { useTypingAlong } from "./typing";

/** Card and bubble sizes in logical pixels; the shell grows the window by this much. */
export const CARD_W = 300;
export const CARD_H = 404; // measured 2026-10-28 in a browser: 390 px of content, ~396 px with the focus controls row; the old 352 cut off the buttons
export const BUBBLE_W = 220;
export const BUBBLE_H = 52;
/** Roughly how many characters fit on a bubble line at 220 px. */
const BUBBLE_CHARS = 32;
const BUBBLE_MAX_LINES = 6;

/** What a bubble says, with the click hint when it has an action. */
export const bubbleText = (r: { bubble: string; action?: { label: string } | undefined }) => (r.action ? `${r.bubble} · click: ${r.action.label}` : r.bubble);

/** A bubble is two lines tall for a short line and grows (to six lines) for a long one, like the daily hello. */
export function bubbleHeight(text: string): number {
  const lines = Math.min(BUBBLE_MAX_LINES, Math.max(2, Math.ceil(text.length / BUBBLE_CHARS)));
  return lines === 2 ? BUBBLE_H : 20 + lines * 16;
}
/** The focus timer above or below the pet (FR-110). */
export const TIMER_H = 36;
const SIZES = { S: 96, M: 128, L: 160 } as const;
const DOUBLE_CLICK_MS = 260;
const DRAG_PX = 4;
/** How often the pet checks whether it has been quiet long enough to fall asleep. */
const SLEEP_CHECK_MS = 30_000;

type Anchor = { pet: "left" | "right"; card: "above" | "below" };
type Panel = "card" | "bubble" | "timer" | null;

/**
 * The desktop pet. Click opens the dashboard, double-click hides it, right-click opens the
 * menu, drag moves it, hover shows today at a glance. It reacts to tasks and reminders with a
 * short animation and a speech bubble (see reactions.ts).
 */
/** Poses that share a row of a picture pet's sheet get their own motion, so a ring, a toss and a catch don't look alike. */
/** All in place: a hover greeting must not move the pet (owner 2026-10-27: "it moves when my cursor comes near"). */
export const PACK_MOTION: Partial<Record<BearPose, string>> = { wave: "pet-pop", ring: "pet-shake", toss: "pet-toss", catch: "pet-pop" };

/** Longest the pet stays hidden for a window resize; it comes back even if no frame is ever painted. */
const RESIZE_HIDE_MAX_MS = 250;

/** Resolves after the browser has painted the current layout (or after `RESIZE_HIDE_MAX_MS`, whichever is first). */
function nextPaint(): Promise<void> {
  return new Promise((done) => {
    const late = setTimeout(done, RESIZE_HIDE_MAX_MS);
    const finish = () => { clearTimeout(late); done(); };
    if (typeof requestAnimationFrame === "function") requestAnimationFrame(() => requestAnimationFrame(finish));
    else setTimeout(finish, 0);
  });
}

/**
 * `cardWindow`: the hover card opens in its own small window beside the pet (the app), so the
 * pet's window never changes size on hover and the pet never has to be hidden for a moment
 * (owner 2026-11-01: "the pet disappears and appears"). Outside the app (browser preview, tests of
 * the rest) the card is drawn inside the pet window as before.
 */
export function PetApp({ cardWindow = isTauri() }: { cardWindow?: boolean } = {}) {
  useTheme();
  const qc = useQueryClient();
  const worn = useWorn();
  const { data: settings } = useSettings();
  const { data: openToday = [] } = useTodayTasks();
  const size = SIZES[settings?.["pet.size"] ?? "M"];
  // A pet pack from the library replaces the bear (FR-63); reactions map to its rows.
  const packId = settings?.["pet.pack"] ?? null;
  const { data: sheet } = useQuery({ queryKey: ["pets", "sheet", packId], queryFn: () => ipc.petsSheet(packId ?? ""), enabled: !!packId, staleTime: Infinity, retry: false });
  // The pet's own details (`pluto.json`): its poses for some moments and its one special.
  const { data: library = [] } = useQuery({ queryKey: ["pets", "library"], queryFn: ipc.petsList, staleTime: 60_000 });
  const pack = packId ? library.find((p) => p.id === packId) : undefined;
  const packRef = useRef(pack);
  packRef.current = pack;
  const opacity = (settings?.["pet.opacity"] ?? 100) / 100;
  const hoverCard = settings?.["pet.hover_card"] ?? true;
  const hoverCardRef = useRef(hoverCard);
  hoverCardRef.current = hoverCard;
  const focus = useFocus();
  const focusRef = useRef(focus.session);
  focusRef.current = focus.session;
  const { data: reminders = [] } = useReminders(false);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const [panel, setPanel] = useState<Panel>(null);
  const [hovering, setHovering] = useState(false);
  const [hop, setHop] = useState(false);
  // True while the OS resizes the window. A transparent window can show its previous frame (the pet
  // alone) at the top of the grown window for a frame or two, which looked like a second Pluto
  // blinking above the card (owner 2026-10-31). The pet is invisible, not hidden, for that moment,
  // so the pointer still counts as over it.
  const [resizing, setResizing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const draggingRef = useRef(false);
  draggingRef.current = dragging;
  const [playing, setPlaying] = useState<Reaction | null>(null);
  const timers = useRef<{ enter?: number | undefined; leave?: number | undefined; click?: number | undefined; play?: number | undefined }>({});
  const press = useRef<{ x: number; y: number } | null>(null);
  const queue = useRef<Reaction[]>([]);
  const tasks = useRef<TaskRow[]>(openToday);
  // Anything happening (hover, click, an event from Pluto, tracked app time) keeps the bear awake.
  const lastActive = useRef(Date.now());
  const [asleep, setAsleep] = useState(false);
  // "Sleep" on the card: a nap that lasts until Wake (or a click on the pet), not ended by hovering.
  const [napping, setNapping] = useState(false);
  const nappingRef = useRef(false);
  nappingRef.current = napping;
  // The card's own window (see `cardWindow`): whether it is showing, whether the pointer is on it,
  // and whether it should come back after a drag.
  const cardOpenRef = useRef(false);
  const cardHover = useRef(false);
  const reopenAfterDrag = useRef(false);
  // "Sleep or wake" in the right-click menu (owner 2026-10-27).
  useEffect(() => onPetMenu((what) => { if (what === "nap") setNapping((n) => !n); }), []);
  const wake = () => {
    lastActive.current = Date.now();
    setAsleep(false);
  };
  const hoverRef = useRef(false);
  const panelRef = useRef<Panel>(null);
  tasks.current = openToday;
  const typing = useTypingAlong(settings?.["pet.typing"] ?? false);
  useEffect(() => {
    if (typing !== "off") wake();
  }, [typing]);
  hoverRef.current = hovering;
  panelRef.current = panel;

  // A layout change (card, bubble, timer) while the OS is moving the window would resize it under
  // the drag, so the pet slips off the cursor. It waits for the end of the drag instead.
  const deferred = useRef<{ kind: "expand"; next: Exclude<Panel, null>; text: string } | { kind: "collapse" } | null>(null);
  const expand = async (next: Exclude<Panel, null>, text = "") => {
    if (draggingRef.current) {
      deferred.current = { kind: "expand", next, text };
      return;
    }
    const [w, h] = next === "card" ? [CARD_W, CARD_H] : next === "timer" ? [size, TIMER_H] : [BUBBLE_W, bubbleHeight(text)];
    // The timer sits under the pet, in its own pixel area; the card and bubble keep above.
    const below = next === "timer";
    // Lay out for the grown window first (the pet in its corner, the panel clipped), then grow it:
    // the pet is already where it will stay, so it does not jump when the cursor comes near.
    const planned = await ipc.petPlan(w, h, below);
    try {
      if (planned) {
        setResizing(true);
        setAnchor(planned);
        setPanel(next);
        await nextPaint(); // the blank frame is on screen before the window changes size
        // A drag that began meanwhile gets the window as it is; the panel follows at its end.
        if (draggingRef.current) {
          deferred.current = { kind: "expand", next, text };
          return;
        }
      }
      const a = await ipc.petHover(true, w, h, below);
      if (a) {
        setAnchor(a);
        setPanel(next);
      }
      if (planned) await nextPaint(); // the grown window has painted its own layout
    } finally {
      setResizing(false);
    }
    placeCard();
  };
  const collapse = async () => {
    if (draggingRef.current) {
      deferred.current = { kind: "collapse" };
      return;
    }
    // Shrink first, then drop the panel: the other order re-centred the pet in the big window
    // for a moment, so it jumped every time the cursor left it.
    try {
      if (panelRef.current !== null) {
        setResizing(true);
        await nextPaint();
      }
      await ipc.petHover(false, 0, 0);
      setAnchor(null);
      setPanel(null);
      if (panelRef.current !== null) await nextPaint();
    } finally {
      setResizing(false);
    }
    placeCard();
  };
  /** The card window follows the pet window when it grows or shrinks (a focus timer starting). */
  const placeCard = () => {
    if (cardWindow && cardOpenRef.current) void ipc.petCard(true, CARD_W, CARD_H);
  };
  const showCard = async (open: boolean) => {
    cardOpenRef.current = open;
    await ipc.petCard(open, CARD_W, CARD_H);
    if (open) void petCardSend({ kind: "napping", value: nappingRef.current });
  };
  /** Back to resting: the focus timer while a session runs, else just the pet. */
  const settle = () => (focusRef.current ? expand("timer") : collapse());

  // Play the next waiting reaction: pose for its duration, bubble unless the card is open.
  const playingRef = useRef<Reaction | null>(null);
  const playNext = () => {
    const [next, ...rest] = queue.current;
    queue.current = rest;
    playingRef.current = next ?? null;
    setPlaying(next ?? null);
    if (!next) {
      if (panelRef.current === "bubble") void settle();
      return;
    }
    // While hovering, the card has the space, unless the card is turned off (FR-110).
    if ((!hoverRef.current && !cardOpenRef.current) || !hoverCardRef.current) void expand("bubble", bubbleText(next));
    timers.current.play = window.setTimeout(playNext, next.ms);
  };
  // The character's one special move, when its count comes round (see special.ts).
  const counts = useRef(NO_COUNTS);
  const special = (signal: SpecialSignal) => {
    const sp = packRef.current?.special;
    const next = advance(counts.current, signal, sp);
    counts.current = next.counts;
    if (next.fire && sp) reactRef.current(reactionForSpecial(sp));
  };
  const specialRef = useRef(special);
  specialRef.current = special;
  // A daily special plays the first time the pet is seen each day.
  const specialPackId = pack?.special?.trigger === "daily" ? pack.id : null;
  useEffect(() => {
    if (!specialPackId) return;
    const today = ymd(new Date());
    void ipc.prefsGet<string>("ui.special_day").then((last) => {
      if (last === today) return;
      void ipc.prefsSet("ui.special_day", today);
      specialRef.current("day-start");
    }, () => undefined);
  }, [specialPackId]);
  const reactRef = useRef((r: Reaction | null) => {
    if (!r) return;
    queue.current = enqueue(queue.current, r);
    if (!playingRef.current) playNext();
  });

  // Keep the card fresh, and react to what happens in Pluto.
  // Messages from the dashboard, e.g. the daily routine reminder (FR-85), shown above the pet.
  useEffect(() => onPetSay((say) => {
    // A say can follow a change of look made in the dashboard (a separate window): look again.
    void qc.invalidateQueries({ queryKey: WARDROBE_KEY });
    const r = reactionForSay(say, BEAR_POSES);
    if (r) reactRef.current(r);
  }), [qc]);
  useEffect(() => onEvent((e) => {
    wake();
    // The user's changes to what the pet does (Character page) are read fresh for each event.
    const ctx = { openToday: tasks.current };
    void ipc.prefsGet("ui.reactions").then(
      (raw) => reactRef.current(reactionWithSettings(e, ctx, parseReactionSettings(raw), packRef.current?.reactions)),
      () => reactRef.current(reactionWithSettings(e, ctx, {}, packRef.current?.reactions)),
    );
    if (e.type === "TaskStatusChanged" && (e as unknown as { to?: string }).to === "COMPLETED") special("task-done");
    void qc.invalidateQueries();
  }), [qc]);

  useEffect(() => {
    const id = window.setInterval(() => setAsleep(isAsleep(lastActive.current)), SLEEP_CHECK_MS);
    return () => clearInterval(id);
  }, []);

  useEffect(() => () => Object.values(timers.current).forEach((t) => clearTimeout(t)), []);

  const openCard = async () => {
    clearTimeout(timers.current.leave);
    if (cardWindow) {
      // Shown again even if it was open: the shell may have put it away with the pet (full screen).
      if (panelRef.current === "bubble") await settle(); // the card takes the bubble's place
      await showCard(true);
      return;
    }
    if (panelRef.current === "card") return;
    await expand("card");
  };
  const closeCard = async () => {
    clearTimeout(timers.current.enter);
    if (cardWindow) {
      if (!cardOpenRef.current) return;
      await showCard(false);
      if (playingRef.current) await expand("bubble", bubbleText(playingRef.current));
      return;
    }
    if (panelRef.current !== "card") return;
    // A reaction still playing gets its bubble back.
    if (playingRef.current) await expand("bubble", bubbleText(playingRef.current));
    else await settle();
  };

  // The timer comes and goes with the session, unless a bubble or the card is showing.
  const running = !!focus.session;
  useEffect(() => {
    if (running && panelRef.current === null) void expand("timer");
    if (!running && panelRef.current === "timer") void collapse();
  }, [running]);

  /** With the hover card off, the same facts come as one short bubble (FR-110). */
  const glance = () => {
    if (playingRef.current) return;
    const next = reminders.filter((x) => x.remind_at >= Date.now()).sort((a, b) => a.remind_at - b.remind_at)[0];
    reactRef.current({ pose: "wave", bubble: glanceText({ session: focusRef.current, remaining: focus.remaining, tasks: tasks.current, next, now: Date.now() }), ms: 6000, priority: 1 });
  };

  const onEnter = () => {
    wake();
    setHovering(true);
    clearTimeout(timers.current.leave);
    timers.current.enter = window.setTimeout(() => (hoverCard ? void openCard() : glance()), 350);
  };
  const onLeave = () => {
    if (draggingRef.current) return; // the window is moving under the pointer; not a real leave
    setHovering(false);
    clearTimeout(timers.current.enter);
    leaveSoon();
  };
  // The card closes a moment after the pointer has left both the pet and the card's window.
  const leaveSoon = () => {
    clearTimeout(timers.current.leave);
    timers.current.leave = window.setTimeout(() => {
      if (!hoverRef.current && !cardHover.current) void closeCardRef.current();
    }, 250);
  };
  const closeCardRef = useRef(closeCard);
  closeCardRef.current = closeCard;
  const leaveSoonRef = useRef(leaveSoon);
  leaveSoonRef.current = leaveSoon;
  // Messages from the card's window: the pointer on it keeps it open; Sleep and Wake there.
  useEffect(() => {
    if (!cardWindow) return;
    return onPetCard((m) => {
      if (m.kind === "hover") {
        cardHover.current = m.value;
        if (m.value) clearTimeout(timers.current.leave);
        else leaveSoonRef.current();
      } else if (m.kind === "nap") {
        setNapping(m.value);
      }
    });
  }, [cardWindow]);
  // The card shows Sleep or Wake to match.
  useEffect(() => {
    if (cardWindow && cardOpenRef.current) void petCardSend({ kind: "napping", value: napping });
  }, [cardWindow, napping]);

  const click = () => {
    setNapping(false);
    special("click");
    // A bubble with an action (Undo…): a click runs it instead of opening the dashboard.
    const action = playingRef.current?.action;
    if (action) {
      // A reminder's "Done" is handled right here; other actions go back to the dashboard.
      if (action.id.startsWith("reminder-done:")) void ipc.reminderSetDone(action.id.slice("reminder-done:".length), true);
      else if (action.id.startsWith("task-undo:")) void ipc.taskTransition(action.id.slice("task-undo:".length), "PLANNED");
      else if (action.id.startsWith("focus-break:")) void focus.start("BREAK", Number(action.id.slice("focus-break:".length)) || 5);
      else void petAction(action.id);
      const drop = (r: Reaction): Reaction => ({ pose: r.pose, bubble: r.bubble, ms: r.ms, priority: r.priority });
      if (playingRef.current) playingRef.current = drop(playingRef.current);
      setPlaying((p) => (p ? drop(p) : p));
      return;
    }
    if (timers.current.click) {
      clearTimeout(timers.current.click);
      timers.current.click = undefined;
      void ipc.petDoubleClick();
      return;
    }
    timers.current.click = window.setTimeout(() => {
      timers.current.click = undefined;
      setHop(true);
      void ipc.petClick();
    }, DOUBLE_CLICK_MS);
  };

  // Armed from anywhere in the pet window, not just the visible bear: with the card open the
  // window is bigger than the bear's own square, and the owner found that pressing on the bear
  // itself sometimes didn't start a drag there (0.1.3 report) — pressing anywhere in the window
  // always does, so there's no wrong spot to grab.
  const onPointerDown = (e: React.PointerEvent) => {
    // Only the primary button starts a click or a drag. (jsdom leaves `button` unset.)
    if (e.button !== undefined && e.button !== 0) return;
    wake();
    // A press on a button or link is a click, not a drag: once the OS takes over a drag the click
    // never arrives (the card's Sleep button did nothing; owner 2026-10-27). The bear is a button
    // too, and does drag.
    const control = e.target instanceof Element ? e.target.closest("button, a, input, select, textarea") : null;
    if (control && !control.hasAttribute("data-pet-bear")) {
      press.current = null;
      return;
    }
    press.current = { x: e.screenX ?? 0, y: e.screenY ?? 0 };
  };
  const startDrag = (e: React.PointerEvent) => {
    const p = press.current;
    if (!p || !(e.buttons & 1) || Math.hypot(e.screenX - p.x, e.screenY - p.y) < DRAG_PX) return;
    press.current = null;
    // Drag the window as it is: shrinking it first (to drop the card) loses the grab on Windows.
    // The card is hidden while dragging and the shell works out where the pet ended up.
    clearTimeout(timers.current.enter);
    draggingRef.current = true;
    setDragging(true);
    // The card's window does not move with the pet: put it away and bring it back at the new spot.
    if (cardWindow && cardOpenRef.current) {
      reopenAfterDrag.current = true;
      void showCard(false);
    }
    void (async () => {
      if (isTauri()) {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().startDragging();
      }
    })();
  };
  // The OS owns the pointer during a drag; the first move without a button pressed means it ended.
  const endDrag = () => {
    draggingRef.current = false;
    setDragging(false);
    const later = deferred.current;
    deferred.current = null;
    if (later?.kind === "collapse") void collapse();
    // A bubble that finished while the pet was being dragged is not worth opening any more.
    else if (later?.kind === "expand" && (later.next !== "bubble" || playingRef.current)) void expand(later.next, later.text);
    // Otherwise lay the card out again for the new spot (it may need to open the other way now).
    else if (panelRef.current) void expand(panelRef.current);
    if (reopenAfterDrag.current) {
      reopenAfterDrag.current = false;
      void showCard(true);
    }
  };
  // One pointermove handler for the whole window: arm a drag while pressed, or notice one ended.
  const onWindowPointerMove = (e: React.PointerEvent) => {
    if (draggingRef.current) {
      if (!(e.buttons & 1)) endDrag();
    } else {
      startDrag(e);
    }
  };
  // Clicking (as opposed to dragging) only means something on the bear itself; a press that
  // started elsewhere in the window (e.g. over the card) still arms a drag but opens nothing.
  const onBearPointerUp = () => {
    if (press.current) click();
    press.current = null;
  };
  const onWindowPointerUp = () => {
    press.current = null;
  };

  const working = openToday.some((r) => r.task.status === "IN_PROGRESS");
  // Typing along beats hover and the rest: you are busy, it keeps you company.
  const pose: BearPose = playing?.pose ?? (napping ? "sleep" : typing !== "off" ? typing : hovering ? "wave" : asleep ? "sleep" : working ? "work" : "idle");
  const bear = (
    <button
      type="button" data-pet-bear aria-label="Pluto. Click to open, double-click to hide, right-click for options"
      onPointerUp={onBearPointerUp}
      onContextMenu={(e) => { e.preventDefault(); void ipc.petMenu(); }}
      onAnimationEnd={(e) => e.target === e.currentTarget && setHop(false)}
      className={cn("shrink-0 cursor-pointer select-none rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring", hop && "pet-hop")}
      style={{ width: size, height: size, opacity }}
    >
      {/* key: a new reaction restarts its animation even when the pose repeats. One key for the
          whole typing sequence, so the laptop comes out once and isn't pulled out again. */}
      {packId && sheet ? (
        <SpritePet key={playing ? `${playing.pose}-${playing.bubble}` : undefined} sheet={sheet} row={ROW_FOR_POSE[pose]} rest={pose === "sleep"} size={size} label={`${packId} (${pose})`} wear={worn} className={cn("pointer-events-none mx-auto", PACK_MOTION[pose])} />
      ) : (
        <Bear key={playing ? `${playing.pose}-${playing.bubble}` : typing !== "off" ? "typing" : pose} pose={pose} wear={worn} animated className="pointer-events-none size-full" />
      )}
    </button>
  );

  const extra = panel === "card"
    ? <PetCard className="shrink-0" style={{ width: CARD_W, height: CARD_H }} napping={napping} onNap={setNapping} />
    : panel === "bubble" && playing
      ? <Bubble text={bubbleText(playing)} anchor={anchor} />
      : panel === "timer" && focus.session
        ? <FocusChip width={size} label={focus.session.kind === "BREAK" ? "Break" : focus.session.label || "Focus"} left={clock(focus.remaining)} paused={focus.session.paused_at !== null} onPause={(p) => void focus.pause(p)} />
        : null;

  return (
    <div
      className="flex h-screen w-screen select-none flex-col overflow-hidden"
      style={resizing ? { opacity: 0 } : undefined}
      onMouseEnter={onEnter} onMouseLeave={onLeave}
      onPointerDown={onPointerDown} onPointerMove={onWindowPointerMove} onPointerUp={onWindowPointerUp}
    >
      <p className="sr-only" role="status">{playing?.bubble ?? ""}</p>
      {anchor && extra ? (
        <div className={cn("flex h-full flex-col justify-end gap-2", anchor.card === "below" && "flex-col-reverse", anchor.pet === "right" ? "items-end" : "items-start")}>
          <div className={cn("contents", dragging && "[&>*]:invisible")}>{extra}</div>
          {bear}
        </div>
      ) : (
        <div className="grid h-full place-items-center">{bear}</div>
      )}
    </div>
  );
}

/** A short line from the pet, pointing at it. */
function Bubble({ text, anchor }: { text: string; anchor: Anchor | null }) {
  return (
    <div
      aria-hidden
      className={cn(
        "pet-bubble-in relative flex shrink-0 items-center rounded-2xl border bg-popover px-3.5 text-[12.5px] font-medium leading-tight text-popover-foreground shadow-lg",
        anchor?.pet === "right" ? "rounded-br-md" : "rounded-bl-md",
      )}
      style={{ width: BUBBLE_W, height: bubbleHeight(text) }}
    >
      <span className="line-clamp-6">{text}</span>
    </div>
  );
}

/** The focus timer under the pet (FR-110, H1): pixel style, time left, and pause/resume. (Music buttons were taken out 2026-10-27 to be planned again.) */
function FocusChip({ width, label, left, paused, onPause }: { width: number; label: string; left: string; paused: boolean; onPause: (paused: boolean) => void }) {
  const btn = "grid size-6 shrink-0 place-items-center border-2 border-foreground/70 bg-background font-mono text-[10px] leading-none outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring";
  // As wide as the pet (owner 2026-10-27): the time and pause only; the label is in the hover card.
  return (
    <div className="flex shrink-0 items-center justify-between gap-1 border-2 border-foreground/70 bg-popover px-1.5 font-mono text-popover-foreground shadow-[3px_3px_0_0_var(--border)]" style={{ width, height: TIMER_H, imageRendering: "pixelated" }} role="timer" title={label} aria-label={`${label}: ${left} left${paused ? ", paused" : ""}`}>
      <span className={cn("size-2 shrink-0", paused ? "bg-muted-foreground" : "bg-primary")} />
      <span className="text-[13px] font-bold tabular-nums">{left}</span>
      <button type="button" aria-label={paused ? "Resume focus" : "Pause focus"} onClick={() => onPause(!paused)} className={btn}>{paused ? "▶" : "❚❚"}</button>
    </div>
  );
}
