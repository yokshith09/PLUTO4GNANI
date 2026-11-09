import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Check, Quote, Sparkles, Square, Volume2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { realAudio, type AudioDeps } from "@/features/voice/VoiceProvider";
import { useSpeakText } from "@/features/voice/useSpeakText";
import { ipc } from "@/ipc";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceErrorMessage } from "@/ipc/voiceClient";
import type { EditedItem, ExtractedItem, ExtractionResult, QualityWarning, Relevance } from "@/ipc/generated";
import { notify } from "@/lib/notify";
import { cn } from "@/lib/utils";

const GROUPS: { kind: Relevance; title: string }[] = [
  { kind: "decision", title: "Decisions" },
  { kind: "action_item", title: "Action items" },
  { kind: "question", title: "Open questions" },
  { kind: "risk", title: "Risks" },
];

interface Row { accepted: boolean; body: string; owner: string; due: string; createTask: boolean }

function initial(i: ExtractedItem): Row {
  return { accepted: true, body: i.body, owner: i.owner ?? "", due: i.due_date ?? "", createTask: i.kind === "action_item" && i.duplicate_of_task_id === null };
}

export function warningText(w: QualityWarning, items: ExtractedItem[]): string | null {
  const name = (id: string) => items.find((i) => i.id === id)?.body;
  switch (w.kind) {
    case "no_decisions": return "No decisions were found in this meeting.";
    case "action_item_without_owner": { const n = name(w.item_id); return n ? `“${n}” has no owner.` : null; }
    case "unresolved_due_date": { const n = name(w.item_id); return n ? `“${n}” has a deadline Pluto couldn't turn into a date.` : null; }
  }
}

/** Build what `meeting_extraction_apply` receives: only accepted items, with the user's edits. */
export function toApply(meetingId: string, items: ExtractedItem[], rows: Record<string, Row>) {
  const accepted: EditedItem[] = items.flatMap((i) => {
    const r = rows[i.id];
    if (!r?.accepted || !r.body.trim()) return [];
    const action = i.kind === "action_item";
    return [{
      item_id: i.id, body: r.body.trim(), owner: action && r.owner.trim() ? r.owner.trim() : null,
      due_date: action && r.due ? r.due : null, priority: i.priority, create_task: action && r.createTask,
    }];
  });
  return { meeting_id: meetingId, accepted };
}

/**
 * Review and apply (T-204): find decisions and action items, edit or reject each (always showing the
 * words they came from), then apply. Nothing is written until Apply.
 */
export function ReviewPanel({ meetingId, participants, onShowSegment, audio = realAudio }: { meetingId: string; participants: string[]; onShowSegment: (segmentId: string) => void; audio?: AudioDeps }) {
  const qc = useQueryClient();
  const [result, setResult] = useState<ExtractionResult | null>(null);
  const [rows, setRows] = useState<Record<string, Row>>({});
  const [busy, setBusy] = useState<"extract" | "apply" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const voice = useSpeakText(audio);

  const find = async () => {
    setBusy("extract"); setError(null);
    try {
      const r = await meetingAi.extract(meetingId);
      setResult(r);
      setRows(Object.fromEntries(r.items.map((i) => [i.id, initial(i)])));
    } catch (e) { setError(voiceErrorMessage(e)); } finally { setBusy(null); }
  };

  const patch = (id: string, p: Partial<Row>) => setRows((all) => ({ ...all, [id]: { ...(all[id] as Row), ...p } }));
  const payload = useMemo(() => (result ? toApply(meetingId, result.items, rows) : null), [meetingId, result, rows]);

  const apply = async () => {
    if (!payload || !result) return;
    setBusy("apply"); setError(null);
    try {
      const before = (await ipc.meetingGet(meetingId)).action_items.filter((a) => a.task_id).length;
      const meeting = await meetingAi.extractionApply(payload);
      const made = meeting.action_items.filter((a) => a.task_id).length - before;
      await Promise.all([qc.invalidateQueries({ queryKey: ["meeting", meetingId] }), qc.invalidateQueries({ queryKey: ["meetings"] }), qc.invalidateQueries({ queryKey: ["tasks"] })]);
      notify.info("Applied", { description: made > 0 ? `${made} task${made === 1 ? "" : "s"} created.` : `${payload.accepted.length} items added to the meeting.` });
      setResult(null);
    } catch (e) { setError(voiceErrorMessage(e)); } finally { setBusy(null); }
  };

  if (!result) {
    return (
      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={() => void find()} disabled={busy !== null}><Sparkles />{busy === "extract" ? "Reading the transcript…" : "Find decisions and action items"}</Button>
        <span className="text-[12px] text-muted-foreground">The transcript text is sent to Gemini or Groq. Nothing is saved until you apply.</span>
        {error && <p role="alert" className="w-full text-[13px] text-destructive">{error}</p>}
      </div>
    );
  }

  const warnings = result.warnings.map((w) => warningText(w, result.items)).filter((t): t is string => t !== null);
  const accepted = payload?.accepted.length ?? 0;
  return (
    <section aria-label="Review" className="space-y-4">
      {warnings.length > 0 && (
        <div className="space-y-2 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-[13px]">
          <ul aria-label="Warnings" className="space-y-1">
            {warnings.map((t) => <li key={t} className="flex items-start gap-2"><AlertTriangle className="mt-0.5 size-3.5 shrink-0" />{t}</li>)}
          </ul>
          {voice.speaking
            ? <Button size="sm" variant="ghost" onClick={voice.stop}><Square />Stop</Button>
            : <Button size="sm" variant="ghost" onClick={() => void voice.speak(`Check before you apply. ${warnings.join(" ")}`, "en-IN")}><Volume2 />Read aloud</Button>}
          {voice.error && <p role="alert" className="text-destructive">{voice.error}</p>}
        </div>
      )}
      {GROUPS.map(({ kind, title }) => {
        const list = result.items.filter((i) => i.kind === kind);
        if (!list.length) return null;
        return (
          <div key={kind}>
            <h3 className="mb-1.5 text-[13px] font-semibold">{title}</h3>
            <ul aria-label={title} className="space-y-2">
              {list.map((i) => {
                const r = rows[i.id] as Row;
                return (
                  <li key={i.id} className={cn("rounded-lg border bg-card p-3", !r.accepted && "opacity-60")}>
                    <div className="flex flex-wrap items-center gap-2">
                      <input type="checkbox" checked={r.accepted} onChange={(e) => patch(i.id, { accepted: e.target.checked })} aria-label={`Keep: ${i.body}`} className="size-4 accent-primary" />
                      <Input value={r.body} maxLength={300} aria-label={`Text of ${i.body}`} disabled={!r.accepted} className="min-w-48 flex-1" onChange={(e) => patch(i.id, { body: e.target.value })} />
                    </div>
                    {kind === "action_item" && (
                      <div className="mt-2 flex flex-wrap items-center gap-2 pl-6">
                        <select aria-label={`Owner of ${i.body}`} disabled={!r.accepted} value={r.owner} onChange={(e) => patch(i.id, { owner: e.target.value })} className="h-8 rounded-md border bg-transparent px-2 text-[13px]">
                          <option value="">No owner</option>
                          {participants.map((p) => <option key={p} value={p}>{p}</option>)}
                        </select>
                        <Input type="date" aria-label={`Due date of ${i.body}`} disabled={!r.accepted} value={r.due} className="w-40" onChange={(e) => patch(i.id, { due: e.target.value })} />
                        {i.due_phrase && <span className="text-[12px] text-muted-foreground">said “{i.due_phrase}”</span>}
                        <label className="flex items-center gap-1.5 text-[13px]">
                          <input type="checkbox" checked={r.createTask} disabled={!r.accepted} onChange={(e) => patch(i.id, { createTask: e.target.checked })} className="size-4 accent-primary" />Create a task
                        </label>
                        {i.duplicate_of_task_id && <Badge variant="violet" title="An open task already looks like this one">Looks like an existing task</Badge>}
                      </div>
                    )}
                    <p className="mt-2 flex items-start gap-1.5 pl-6 text-[12px] text-muted-foreground">
                      <Quote className="mt-0.5 size-3 shrink-0" />
                      <span className="min-w-0 flex-1">“{i.quote}”</span>
                      <button type="button" className="shrink-0 underline-offset-2 hover:underline focus-visible:underline" onClick={() => onShowSegment(i.source_segment_id)}>Show in transcript</button>
                    </p>
                  </li>
                );
              })}
            </ul>
          </div>
        );
      })}
      {result.items.length === 0 && <p className="text-[13px] text-muted-foreground">Nothing was found to add. You can still write decisions and action items by hand above.</p>}
      {error && <p role="alert" className="text-[13px] text-destructive">{error}</p>}
      <div className="flex gap-2">
        <Button onClick={() => void apply()} disabled={busy !== null || accepted === 0}><Check />{busy === "apply" ? "Applying…" : `Apply ${accepted} item${accepted === 1 ? "" : "s"}`}</Button>
        <Button variant="ghost" onClick={() => setResult(null)} disabled={busy !== null}>Discard</Button>
      </div>
    </section>
  );
}
