import { useEffect, useId, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2, RotateCcw, Search, Users } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { aiKeys, useMeetingJob, useTranscript } from "@/hooks/useMeetingAi";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceErrorMessage } from "@/ipc/voiceClient";
import type { AiStatus, SpeakerMapping, TranscriptSegment } from "@/ipc/generated";
import { notify } from "@/lib/notify";
import { cn } from "@/lib/utils";
import { Marked } from "./Marked";
import { AI_STATUS_LABEL, aiStatusFor, jobStep, speakersOf, timeLabel } from "./aiStatus";

export function StatusChip({ status }: { status: AiStatus }) {
  const variant = status === "failed" ? "destructive" : status === "done" ? "success" : status === "needs_speakers" || status === "needs_review" ? "primary" : "default";
  return <Badge variant={variant}>{status === "transcribing" && <Loader2 className="size-3 animate-spin" />}{AI_STATUS_LABEL[status]}</Badge>;
}

function Transcript({ segments, speakerName, highlight, query }: { segments: TranscriptSegment[]; speakerName: (s: TranscriptSegment) => string; highlight: string | null; query: string }) {
  return (
    <ol aria-label="Transcript" className="max-h-[32rem] divide-y overflow-y-auto rounded-lg border bg-card">
      {segments.map((s) => (
        <li key={s.id} id={`seg-${s.id}`} aria-current={s.id === highlight ? "true" : undefined} className={cn("flex gap-3 px-3 py-2 text-[13px]", s.id === highlight && "bg-primary/10")}>
          <span className="w-10 shrink-0 pt-0.5 text-right text-[11px] tabular-nums text-muted-foreground">{timeLabel(s.start_ms)}</span>
          <span className="min-w-0 flex-1">
            <span className="mr-2 font-medium">{speakerName(s)}</span>
            <span><Marked text={s.text} query={query} /></span>
          </span>
          {s.language && <span className="shrink-0 pt-0.5 text-[11px] text-muted-foreground">{s.language}</span>}
        </li>
      ))}
    </ol>
  );
}

function Speakers({ meetingId, segments, participants }: { meetingId: string; segments: TranscriptSegment[]; participants: string[] }) {
  const qc = useQueryClient();
  const idBase = useId();
  const speakers = speakersOf(segments);
  const [busy, setBusy] = useState(false);
  const choose = async (label: number, participant: string) => {
    const mapping: SpeakerMapping[] = speakers.map((s) => ({
      speaker_label: s.label, participant: s.label === label ? participant || null : s.participant,
    }));
    setBusy(true);
    try {
      await meetingAi.speakersSet(meetingId, mapping);
      await qc.invalidateQueries({ queryKey: aiKeys.transcript(meetingId) });
    } catch (e) {
      notify.error(voiceErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  if (!speakers.length) return <p className="text-[13px] text-muted-foreground">No speakers were separated in this recording.</p>;
  return (
    <ul aria-label="Speakers" className="space-y-3">
      {speakers.map((s) => (
        <li key={s.label} className="rounded-lg border bg-card p-3">
          <div className="flex flex-wrap items-center gap-3">
            <label htmlFor={`${idBase}-${s.label}`} className="text-[13px] font-medium">Speaker {s.label}</label>
            <select
              id={`${idBase}-${s.label}`} disabled={busy} value={s.participant ?? ""}
              onChange={(e) => void choose(s.label, e.target.value)}
              className="h-8 rounded-md border bg-transparent px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <option value="">Not set</option>
              {participants.map((p) => <option key={p} value={p}>{p}</option>)}
            </select>
          </div>
          <ul className="mt-2 space-y-1 text-[12px] text-muted-foreground">
            {s.quotes.map((q, i) => <li key={i}>“{q}”</li>)}
          </ul>
        </li>
      ))}
    </ul>
  );
}

/** Whether a search word matches the start of a word in `text`. */
function matches(text: string, words: string[]) {
  const tokens = text.toLowerCase().split(/[^\p{L}\p{N}]+/u);
  return words.every((w) => tokens.some((t) => t.startsWith(w)));
}

/**
 * The Transcript tab (T-202): where the audio stands (status, progress, Retry on failure), then the
 * transcript with a search box that narrows it to the lines you want, and the speakers behind a
 * button. Nothing shows for a meeting that was typed by hand.
 */
export function AiPanel({ meetingId, participants, highlight = null }: { meetingId: string; participants: string[]; highlight?: string | null }) {
  const qc = useQueryClient();
  const { data: job } = useMeetingJob(meetingId);
  const ready = job?.status === "completed" || job?.status === "partial_failure";
  const { data: segments } = useTranscript(meetingId, ready);
  const [find, setFind] = useState("");
  const [showSpeakers, setShowSpeakers] = useState(false);
  const status = aiStatusFor(job, segments);
  // Coming from a search result: bring that line into view.
  useEffect(() => {
    if (highlight && segments) setTimeout(() => document.getElementById(`seg-${highlight}`)?.scrollIntoView?.({ block: "center" }), 0);
  }, [highlight, segments]);
  if (!status || !job) return <p className="rounded-xl border bg-card px-4 py-10 text-center text-[13px] text-muted-foreground">This meeting has no recording. Record it, or import audio, to get a transcript.</p>;
  const step = jobStep(job);

  const retry = async () => {
    try {
      await meetingAi.jobRetry(meetingId);
      await qc.invalidateQueries({ queryKey: aiKeys.job(meetingId) });
    } catch (e) {
      notify.error(voiceErrorMessage(e));
    }
  };
  const name = (s: TranscriptSegment) => s.participant ?? (s.speaker_label === null ? "Unknown" : `Speaker ${s.speaker_label}`);
  const words = find.toLowerCase().split(/\s+/).map((w) => w.replace(/[^\p{L}\p{N}]/gu, "")).filter(Boolean);
  const shown = segments && words.length ? segments.filter((s) => matches(s.text, words)) : segments;

  return (
    <section aria-label="Audio and transcript" className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <StatusChip status={status} />
        {job.status === "partial_failure" && <Badge variant="destructive">Some parts failed</Badge>}
        {(status === "failed" || job.status === "partial_failure") && (
          <Button size="sm" variant="outline" onClick={() => void retry()}><RotateCcw />Retry</Button>
        )}
      </div>
      {step && (
        <div role="progressbar" aria-label={step.label} aria-valuemin={0} aria-valuemax={step.of} aria-valuenow={step.step}>
          <div className="h-1.5 overflow-hidden rounded-full bg-muted"><div className="h-full bg-primary transition-all" style={{ width: `${(step.step / step.of) * 100}%` }} /></div>
          <p className="mt-1 text-[12px] text-muted-foreground">{step.label}…</p>
        </div>
      )}
      {job.error && <p role="alert" className="text-[13px] text-destructive">{job.error}</p>}
      {ready && segments && (
        <>
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input aria-label="Search this transcript" value={find} onChange={(e) => setFind(e.target.value)} placeholder="Search this transcript" className="pl-9" />
            </div>
            <Button variant={showSpeakers ? "secondary" : "outline"} aria-expanded={showSpeakers} onClick={() => setShowSpeakers((v) => !v)}><Users />Name speakers</Button>
          </div>
          {showSpeakers && <Speakers meetingId={meetingId} segments={segments} participants={participants} />}
          {words.length > 0 && <p className="text-[12px] text-muted-foreground" role="status">{shown?.length ?? 0} of {segments.length} lines</p>}
          {shown && shown.length > 0
            ? <Transcript segments={shown} speakerName={name} highlight={highlight} query={find} />
            : <p className="rounded-lg border bg-card px-4 py-8 text-center text-[13px] text-muted-foreground">No line has “{find}” in it.</p>}
        </>
      )}
    </section>
  );
}
