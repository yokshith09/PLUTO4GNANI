import { useEffect, useId, useRef, useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { Presets } from "@/components/Presets";
import { dayPresets } from "@/lib/datePresets";
import { ArrowDown, ArrowLeft, ArrowUp, Check, Eye, ListChecks, Pencil, Plus, Trash2, X } from "lucide-react";
import { notify } from "@/lib/notify";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { MarkdownView } from "@/features/notes/MarkdownView";
import { useMeeting, useMeetingActions, useParticipantSuggestions } from "@/hooks/usePluto";
import { errorMessage, type ActionItemInput, type DecisionInput, type MeetingInput, type TaskStatus } from "@/ipc";
import { STATUS_LABEL } from "@/lib/taskStatus";
import { fromLocalInput, toLocalInput } from "@/lib/time";
import { cn } from "@/lib/utils";

/** How long the editor waits after the last change before saving (like notes, R1-02). */
export const SAVE_DELAY_MS = 600;

const newId = () => (globalThis.crypto?.randomUUID?.() ?? `i${Date.now()}${Math.random().toString(36).slice(2)}`);
const blank = (): MeetingInput => ({
  title: "", starts_at: Date.now(), notes: "", follow_up_date: null, participants: [], decisions: [], action_items: [],
});

type Status = "idle" | "saving" | "saved" | "error";

/** Move item `i` of a list up (-1) or down (+1). */
export function move<T>(list: T[], i: number, by: -1 | 1): T[] {
  const j = i + by;
  if (j < 0 || j >= list.length) return list;
  const next = [...list];
  [next[i], next[j]] = [next[j] as T, next[i] as T];
  return next;
}

function Field({ label, htmlFor, children, className }: { label: string; htmlFor?: string; children: React.ReactNode; className?: string }) {
  return (
    <div className={cn("space-y-1.5", className)}>
      <label htmlFor={htmlFor} className="text-xs font-medium text-muted-foreground">{label}</label>
      {children}
    </div>
  );
}

function RowTools({ label, i, count, onMove, onRemove }: { label: string; i: number; count: number; onMove: (by: -1 | 1) => void; onRemove: () => void }) {
  const btn = "grid size-7 shrink-0 place-items-center rounded text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-30";
  return (
    <div className="flex shrink-0 items-center opacity-60 group-hover:opacity-100 group-focus-within:opacity-100">
      <button type="button" className={btn} aria-label={`Move ${label} up`} disabled={i === 0} onClick={() => onMove(-1)}><ArrowUp className="size-3.5" /></button>
      <button type="button" className={btn} aria-label={`Move ${label} down`} disabled={i === count - 1} onClick={() => onMove(1)}><ArrowDown className="size-3.5" /></button>
      <ConfirmAction message={`Remove ${label}?`} verb="Remove" onConfirm={onRemove}>
        <button type="button" className={cn(btn, "hover:text-destructive")} aria-label={`Remove ${label}`}><X className="size-3.5" /></button>
      </ConfirmAction>
    </div>
  );
}

function Participants({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {
  const [text, setText] = useState("");
  const list = useId();
  const { data: suggestions = [] } = useParticipantSuggestions(text);
  const add = (raw: string) => {
    const name = raw.trim().replace(/,$/, "").trim();
    if (name && !value.some((v) => v.toLowerCase() === name.toLowerCase())) onChange([...value, name]);
    setText("");
  };
  return (
    <div className="flex min-h-9 flex-wrap items-center gap-1.5 rounded-md border bg-transparent px-2 py-1.5 focus-within:ring-2 focus-within:ring-ring">
      {value.map((p) => (
        <span key={p} className="flex items-center gap-1 rounded-md bg-muted px-2 py-0.5 text-[12px]">
          {p}
          <ConfirmAction message={`Remove ${p}?`} verb="Remove" onConfirm={() => onChange(value.filter((x) => x !== p))}>
            <button type="button" aria-label={`Remove ${p}`} className="text-muted-foreground hover:text-foreground"><X className="size-3" /></button>
          </ConfirmAction>
        </span>
      ))}
      <input
        id="meeting-participants" list={list} value={text} placeholder={value.length ? "" : "Names or emails, Enter to add"}
        aria-label="Add participant"
        className="min-w-32 flex-1 bg-transparent text-[13px] outline-none placeholder:text-muted-foreground"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === ",") { e.preventDefault(); add(text); }
          if (e.key === "Backspace" && !text && value.length) onChange(value.slice(0, -1));
        }}
        onBlur={() => text.trim() && add(text)}
      />
      <datalist id={list}>{suggestions.filter((s) => !value.includes(s)).map((s) => <option key={s} value={s} />)}</datalist>
    </div>
  );
}

function statusVariant(s: TaskStatus) {
  return s === "COMPLETED" ? "success" : s === "IN_PROGRESS" ? "primary" : s === "CANCELLED" ? "destructive" : "default";
}

/**
 * One meeting (R1-30…R1-34): saved by itself a moment after each change. A new meeting is
 * created the first time it has a title. Action items show their task's status live.
 */
export function MeetingEditor({ meetingId, onBack, onCreated }: { meetingId: string | null; onBack: () => void; onCreated: (id: string) => void }) {
  const actions = useMeetingActions();
  const [id, setId] = useState(meetingId);
  const { data: saved } = useMeeting(id);
  const [draft, setDraft] = useState<MeetingInput>(blank);
  const [loaded, setLoaded] = useState(meetingId === null);
  const [status, setStatus] = useState<Status>("idle");
  const [preview, setPreview] = useState(false);
  const dirty = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const inFlight = useRef<Promise<string | null> | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const idRef = useRef(id);
  idRef.current = id;

  // Fill the editor once from the stored meeting.
  useEffect(() => {
    if (loaded || !saved) return;
    setDraft({
      title: saved.title, starts_at: saved.starts_at, notes: saved.notes, follow_up_date: saved.follow_up_date,
      participants: saved.participants,
      decisions: saved.decisions.map((d) => ({ id: d.id, body: d.body })),
      action_items: saved.action_items.map((a) => ({ id: a.id, body: a.body, assignee: a.assignee })),
    });
    setLoaded(true);
  }, [saved, loaded]);

  /** Save now if anything changed. Resolves to the meeting's id (null while it has no title). */
  const save = async (): Promise<string | null> => {
    clearTimeout(timer.current);
    if (inFlight.current) await inFlight.current;
    if (!dirty.current) return idRef.current;
    const input = draftRef.current;
    if (!input.title.trim()) return idRef.current;
    dirty.current = false;
    setStatus("saving");
    const run = (async () => {
      try {
        if (idRef.current) {
          await actions.update(idRef.current, input);
          return idRef.current;
        }
        const m = await actions.create(input);
        idRef.current = m.id;
        setId(m.id);
        onCreated(m.id);
        return m.id;
      } catch (e) {
        dirty.current = true;
        setStatus("error");
        throw e;
      }
    })();
    inFlight.current = run;
    try {
      const out = await run;
      setStatus("saved");
      return out;
    } finally {
      inFlight.current = null;
    }
  };
  const saveRef = useRef(save);
  saveRef.current = save;

  const change = (patch: Partial<MeetingInput>) => {
    dirty.current = true;
    setDraft((d) => ({ ...d, ...patch }));
    clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void saveRef.current().catch(() => {}), SAVE_DELAY_MS);
  };
  // Leaving the page saves what is pending.
  useEffect(() => () => { clearTimeout(timer.current); if (dirty.current) void saveRef.current().catch(() => {}); }, []);

  const back = async () => {
    try { await save(); } catch { /* the error bubble already showed */ }
    onBack();
  };
  const setDecision = (i: number, d: Partial<DecisionInput>) => change({ decisions: draft.decisions.map((x, j) => (j === i ? { ...x, ...d } : x)) });
  const setAction = (i: number, a: Partial<ActionItemInput>) => change({ action_items: draft.action_items.map((x, j) => (j === i ? { ...x, ...a } : x)) });
  const statusOf = (itemId: string | null) => saved?.action_items.find((a) => a.id === itemId);

  const convert = async (itemId: string) => {
    try {
      await save();
      const task = await actions.convert(itemId);
      notify.info(`Task created: “${task.title}”`);
    } catch (e) {
      notify.error(errorMessage(e));
    }
  };
  const remove = async () => {
    clearTimeout(timer.current);
    dirty.current = false;
    if (id) await actions.remove(id);
    onBack();
  };

  if (!loaded) return <p className="py-10 text-center text-[13px] text-muted-foreground">Loading…</p>;

  return (
    <div className="mx-auto max-w-3xl space-y-5">
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="sm" aria-label="Back to meetings" onClick={() => void back()}><ArrowLeft />Meetings</Button>
        <span className="ml-auto text-xs text-muted-foreground" role="status">
          {!draft.title.trim() ? "Add a title to save" : status === "saving" ? "Saving…" : status === "error" ? "Not saved" : status === "saved" ? "Saved" : ""}
        </span>
        {id && (
          <ConfirmAction message="Delete this meeting?" onConfirm={() => void remove()}>
            <Button size="sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label="Delete meeting"><Trash2 /></Button>
          </ConfirmAction>
        )}
      </div>

      <section className="space-y-4 rounded-xl border bg-card p-5">
        <input
          autoFocus={meetingId === null} value={draft.title} maxLength={200} placeholder="Meeting title" aria-label="Meeting title"
          onChange={(e) => change({ title: e.target.value })}
          className="w-full bg-transparent text-xl font-semibold tracking-tight outline-none placeholder:text-muted-foreground/60"
        />
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="When" htmlFor="meeting-when">
            <Input id="meeting-when" type="datetime-local" value={toLocalInput(draft.starts_at ?? Date.now())} onChange={(e) => e.target.value && change({ starts_at: fromLocalInput(e.target.value) })} />
          </Field>
          <Field label="Follow-up date" htmlFor="meeting-follow-up">
            <Input id="meeting-follow-up" type="date" value={draft.follow_up_date ?? ""} onChange={(e) => change({ follow_up_date: e.target.value || null })} />
            <Presets label="Follow-up presets" options={dayPresets()} value={draft.follow_up_date ?? ""} onPick={(v) => change({ follow_up_date: v })} />
          </Field>
        </div>
        <Field label="Participants" htmlFor="meeting-participants">
          <Participants value={draft.participants} onChange={(participants) => change({ participants })} />
        </Field>
      </section>

      <section aria-label="Notes" className="space-y-2 rounded-xl border bg-card p-5">
        <div className="flex items-center">
          <h2 className="text-[13px] font-semibold">Notes</h2>
          <Button size="sm" variant="ghost" className="ml-auto" onClick={() => setPreview((p) => !p)}>{preview ? <><Pencil />Edit</> : <><Eye />Preview</>}</Button>
        </div>
        {preview ? (
          <MarkdownView source={draft.notes || "_No notes yet._"} className="min-h-24" />
        ) : (
          <textarea
            value={draft.notes} aria-label="Meeting notes" placeholder="What was discussed (Markdown works)"
            onChange={(e) => change({ notes: e.target.value })}
            className="min-h-32 w-full resize-y rounded-md border bg-transparent p-3 text-[13.5px] leading-relaxed outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
        )}
      </section>

      <section aria-label="Decisions" className="space-y-2 rounded-xl border bg-card p-5">
        <h2 className="flex items-center gap-1.5 text-[13px] font-semibold"><Check className="size-4 text-success" />Decisions</h2>
        {draft.decisions.length === 0 && <p className="text-xs text-muted-foreground">What did you agree on?</p>}
        <ol className="space-y-1.5">
          {draft.decisions.map((d, i) => (
            <li key={d.id ?? i} className="group flex items-center gap-2">
              <span className="w-5 shrink-0 text-right text-xs tabular-nums text-muted-foreground">{i + 1}.</span>
              <Input
                value={d.body} maxLength={2000} aria-label={`Decision ${i + 1}`}
                onChange={(e) => setDecision(i, { body: e.target.value })}
                onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); change({ decisions: [...draft.decisions.slice(0, i + 1), { id: newId(), body: "" }, ...draft.decisions.slice(i + 1)] }); } }}
              />
              <RowTools label={`decision ${i + 1}`} i={i} count={draft.decisions.length}
                onMove={(by) => change({ decisions: move(draft.decisions, i, by) })}
                onRemove={() => change({ decisions: draft.decisions.filter((_, j) => j !== i) })} />
            </li>
          ))}
        </ol>
        <Button size="sm" variant="ghost" onClick={() => change({ decisions: [...draft.decisions, { id: newId(), body: "" }] })}><Plus />Add decision</Button>
      </section>

      <section aria-label="Action items" className="space-y-2 rounded-xl border bg-card p-5">
        <h2 className="flex items-center gap-1.5 text-[13px] font-semibold"><ListChecks className="size-4 text-primary" />Action items</h2>
        {draft.action_items.length === 0 && <p className="text-xs text-muted-foreground">Who does what next? Each one can become a task.</p>}
        <datalist id="meeting-people">{draft.participants.map((p) => <option key={p} value={p} />)}</datalist>
        <ul className="space-y-1.5">
          {draft.action_items.map((a, i) => {
            const linked = statusOf(a.id);
            return (
              <li key={a.id ?? i} className="group flex flex-wrap items-center gap-2 sm:flex-nowrap">
                <Input
                  value={a.body} maxLength={300} aria-label={`Action item ${i + 1}`} className="min-w-48 flex-1"
                  onChange={(e) => setAction(i, { body: e.target.value })}
                  onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); change({ action_items: [...draft.action_items.slice(0, i + 1), { id: newId(), body: "", assignee: null }, ...draft.action_items.slice(i + 1)] }); } }}
                />
                <Input
                  value={a.assignee ?? ""} list="meeting-people" placeholder="Who" aria-label={`Assignee for action item ${i + 1}`} className="w-32"
                  onChange={(e) => setAction(i, { assignee: e.target.value || null })}
                />
                {linked?.task_status ? (
                  <Badge variant={statusVariant(linked.task_status)} className="shrink-0" title="The task made from this item">Task · {STATUS_LABEL[linked.task_status]}</Badge>
                ) : (
                  <Button size="sm" variant="secondary" className="shrink-0" disabled={!a.body.trim() || !draft.title.trim() || !a.id} onClick={() => a.id && void convert(a.id)}>
                    Convert to task
                  </Button>
                )}
                <RowTools label={`action item ${i + 1}`} i={i} count={draft.action_items.length}
                  onMove={(by) => change({ action_items: move(draft.action_items, i, by) })}
                  onRemove={() => change({ action_items: draft.action_items.filter((_, j) => j !== i) })} />
              </li>
            );
          })}
        </ul>
        <Button size="sm" variant="ghost" onClick={() => change({ action_items: [...draft.action_items, { id: newId(), body: "", assignee: null }] })}><Plus />Add action item</Button>
      </section>
    </div>
  );
}
