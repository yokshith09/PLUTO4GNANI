import { useId, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { AudioLines, Loader2, RotateCcw, Users } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { aiKeys, useMeetingJob, useTranscript } from "@/hooks/useMeetingAi";
import { meetingAi } from "@/ipc/meetingAi";
import { voiceErrorMessage } from "@/ipc/voiceClient";
import type { AiStatus, SpeakerMapping, TranscriptSegment } from "@/ipc/generated";
import { notify } from "@/lib/notify";
import { cn } from "@/lib/utils";
import { ReviewPanel } from "./ReviewPanel";
import { AI_STATUS_LABEL, aiStatusFor, jobStep, speakersOf, timeLabel } from "./aiStatus";

type Tab = "transcript" | "speakers" | "review";

export function StatusChip({ status }: { status: AiStatus }) {
  const variant = status === "failed" ? "destructive" : status === "done" ? "success" : status === "needs_speakers" || status === "needs_review" ? "primary" : "default";
  return <Badge variant={variant}>{status === "transcribing" && <Loader2 className="size-3 animate-spin" />}{AI_STATUS_LABEL[status]}</Badge>;
}

function Transcript({ segments, speakerName, highlight }: { segments: TranscriptSegment[]; speakerName: (s: TranscriptSegment) => string; highlight: string | null }) {
  return (
    <ol aria-label="Transcript" className="max-h-96 divide-y overflow-y-auto rounded-lg border bg-card">
      {segments.map((s) => (
        <li key={s.id} id={`seg-${s.id}`} aria-current={s.id === highlight ? "true" : undefined} className={cn("flex gap-3 px-3 py-2 text-[13px]", s.id === highlight && "bg-primary/10")}>
          <span className="w-10 shrink-0 pt-0.5 text-right text-[11px] tabular-nums text-muted-foreground">{timeLabel(s.start_ms)}</span>
          <span className="min-w-0 flex-1">
            <span className="mr-2 font-medium">{speakerName(s)}</span>
            <span>{s.text}</span>
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

/**
 * Where a meeting's audio stands (T-202): status chip, progress while transcribing, Retry on failure,
 * then the Transcript and Speakers tabs. Nothing shows for a meeting that was typed by hand.
 */
export function AiPanel({ meetingId, participants }: { meetingId: string; participants: string[] }) {
  const qc = useQueryClient();
  const { data: job } = useMeetingJob(meetingId);
  const ready = job?.status === "completed" || job?.status === "partial_failure";
  const { data: segments } = useTranscript(meetingId, ready);
  const [tab, setTab] = useState<Tab>("transcript");
  const [highlight, setHighlight] = useState<string | null>(null);
  const status = aiStatusFor(job, segments);
  if (!status || !job) return null;
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

  return (
    <section aria-label="Audio and transcript" className="mt-6 space-y-3 rounded-xl border bg-card/50 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <AudioLines className="size-4 text-muted-foreground" />
        <h2 className="text-sm font-semibold">Audio and transcript</h2>
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
          <div role="tablist" aria-label="Transcript views" className="flex gap-1">
            {(["transcript", "speakers", "review"] as const).map((t) => (
              <button
                key={t} type="button" role="tab" aria-selected={tab === t} onClick={() => setTab(t)}
                className={cn("flex items-center gap-1.5 rounded-md px-2.5 py-1 text-[13px] font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring", tab === t ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/60")}
              >
                {t === "speakers" && <Users className="size-3.5" />}{t === "transcript" ? "Transcript" : t === "speakers" ? "Speakers" : "Review"}
              </button>
            ))}
          </div>
          <div role="tabpanel">
            {tab === "transcript" && <Transcript segments={segments} speakerName={name} highlight={highlight} />}
            {tab === "speakers" && <Speakers meetingId={meetingId} segments={segments} participants={participants} />}
            {tab === "review" && (
              <ReviewPanel
                meetingId={meetingId} participants={participants}
                onShowSegment={(id) => { setHighlight(id); setTab("transcript"); setTimeout(() => document.getElementById(`seg-${id}`)?.scrollIntoView?.({ block: "center" }), 0); }}
              />
            )}
          </div>
        </>
      )}
    </section>
  );
}
