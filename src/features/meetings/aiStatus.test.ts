import { describe, expect, it } from "vitest";
import type { TranscriptionJob, TranscriptSegment } from "@/ipc/generated";
import { aiStatusFor, jobStep, speakersOf, timeLabel } from "./aiStatus";

const job = (status: TranscriptionJob["status"]): TranscriptionJob => ({ id: "j", meeting_id: "m", gnani_job_id: null, status, error: null, updated_at: 0 });
const seg = (i: number, label: number | null, participant: string | null, text = `line ${i}`): TranscriptSegment => ({
  id: `s${i}`, meeting_id: "m", source: "batch", index: i, start_ms: i * 1000, end_ms: i * 1000 + 900, speaker_label: label, participant, text, language: null, relevance: null,
});

describe("aiStatusFor", () => {
  it("follows the job and the speaker mapping", () => {
    expect(aiStatusFor(null, undefined)).toBeNull();
    for (const s of ["queued", "uploading", "created", "in_progress"] as const) expect(aiStatusFor(job(s), undefined)).toBe("transcribing");
    expect(aiStatusFor(job("failed"), undefined)).toBe("failed");
    expect(aiStatusFor(job("cancelled"), undefined)).toBe("failed");
    expect(aiStatusFor(job("completed"), [seg(0, 1, null)])).toBe("needs_speakers");
    expect(aiStatusFor(job("completed"), [seg(0, 1, "Asha")])).toBe("needs_review");
    expect(aiStatusFor(job("completed"), [])).toBe("transcribing");
    expect(aiStatusFor(null, [seg(0, 1, "Asha")])).toBe("needs_review");
  });
});

describe("helpers", () => {
  it("steps run 1 to 4 and end", () => {
    expect(jobStep(job("queued"))).toMatchObject({ step: 1, of: 4 });
    expect(jobStep(job("in_progress"))?.label).toBe("Transcribing");
    expect(jobStep(job("completed"))).toBeNull();
  });
  it("lists speakers once, in order, with at most two quotes", () => {
    const s = speakersOf([seg(0, 2, null), seg(1, 1, "Asha"), seg(2, 2, null), seg(3, 2, null), seg(4, null, null)]);
    expect(s.map((x) => x.label)).toEqual([2, 1]);
    expect(s[0]?.quotes).toEqual(["line 0", "line 2"]);
    expect(s[1]?.participant).toBe("Asha");
  });
  it("formats times", () => {
    expect(timeLabel(65_000)).toBe("1:05");
    expect(timeLabel(null)).toBe("");
  });
});
