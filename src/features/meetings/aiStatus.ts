import type { AiStatus, TranscriptionJob, TranscriptSegment } from "@/ipc/generated";

/** What the status chip shows. `null`: this meeting has no audio pipeline (a hand-written one). */
export function aiStatusFor(job: TranscriptionJob | null | undefined, segments: readonly TranscriptSegment[] | undefined): AiStatus | null {
  if (!job) return segments?.length ? "needs_review" : null;
  switch (job.status) {
    case "failed":
    case "cancelled":
      return "failed";
    case "completed":
    case "partial_failure": {
      if (!segments?.length) return "transcribing";
      return segments.some((s) => s.participant) ? "needs_review" : "needs_speakers";
    }
    default:
      return "transcribing";
  }
}

export const AI_STATUS_LABEL: Record<AiStatus, string> = {
  recording: "Recording", transcribing: "Transcribing", needs_speakers: "Needs speakers", needs_review: "Ready to review", done: "Done", failed: "Failed",
};

/** Four steps for the progress bar; `null` once finished. */
export function jobStep(job: TranscriptionJob): { step: number; of: number; label: string } | null {
  const steps: Partial<Record<TranscriptionJob["status"], [number, string]>> = {
    queued: [1, "Waiting to start"], uploading: [2, "Uploading the audio"], created: [3, "Preparing the transcript"], in_progress: [3, "Transcribing"],
  };
  const s = steps[job.status];
  return s ? { step: s[0], of: 4, label: s[1] } : null;
}

/** Speakers in order of first appearance, each with two sample quotes. */
export function speakersOf(segments: readonly TranscriptSegment[]) {
  const map = new Map<number, { label: number; participant: string | null; quotes: string[] }>();
  for (const s of segments) {
    if (s.speaker_label === null) continue;
    const e = map.get(s.speaker_label) ?? { label: s.speaker_label, participant: s.participant, quotes: [] };
    if (e.quotes.length < 2 && s.text.trim()) e.quotes.push(s.text);
    if (s.participant) e.participant = s.participant;
    map.set(s.speaker_label, e);
  }
  return [...map.values()];
}

export function timeLabel(ms: number | null): string {
  if (ms === null) return "";
  const t = Math.floor(ms / 1000);
  return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
}
