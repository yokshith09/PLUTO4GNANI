import { afterEach, describe, expect, it } from "vitest";
import { createMock } from "./mock";
import { onNamed } from "./namedEvents";
import { isVoiceError } from "./voiceClient";

// Every new command is callable against the mock, and the error states are reachable (T-006).
const setup = () => createMock();
type M = ReturnType<typeof createMock>;
const enable = async (m: M) => {
  await m.call("voice_settings_set", { patch: { cloud_voice_enabled: true } });
  await m.call("voice_key_set", { account: "gnani", secret: "k" });
};
const code = async (p: Promise<unknown>) => p.then(() => "ok", (e) => (isVoiceError(e) ? e.code : "other"));

afterEach(() => undefined);

describe("voice commands on the mock", () => {
  it("refuses push-to-talk until cloud voice and a key are set", async () => {
    const m = setup();
    expect(await code(m.call("voice_ptt_start", {}))).toBe("cloud_voice_disabled");
    await m.call("voice_settings_set", { patch: { cloud_voice_enabled: true } });
    expect(await code(m.call("voice_ptt_start", {}))).toBe("missing_api_key");
    await m.call("voice_key_set", { account: "gnani", secret: "k" });
    expect(await m.call("voice_ptt_start", {})).toMatch(/^sess-/);
  });

  it("shows a proposal after push-to-talk and runs it only when approved", async () => {
    const m = setup();
    await enable(m);
    const seen = new Promise<{ id: string }>((res) => { const off = onNamed("voice://proposal", (p) => { off(); res(p); }); });
    await m.call("voice_ptt_start", {});
    await m.call("voice_ptt_stop", {});
    const p = await seen;
    const done = (await m.call("voice_proposal_respond", { proposalId: p.id, response: { kind: "approve" } })) as { ok: boolean };
    expect(done.ok).toBe(true);
    expect(await code(m.call("voice_proposal_respond", { proposalId: p.id, response: { kind: "approve" } }))).toBe("proposal_expired");
  });

  it("answers rejection without changing anything", async () => {
    const m = setup();
    await enable(m);
    const seen = new Promise<{ id: string }>((res) => { const off = onNamed("voice://proposal", (p) => { off(); res(p); }); });
    await m.call("voice_ptt_start", {});
    await m.call("voice_ptt_stop", {});
    const r = (await m.call("voice_proposal_respond", { proposalId: (await seen).id, response: { kind: "reject" } })) as { ok: boolean };
    expect(r.ok).toBe(false);
  });

  it("keeps secrets out of the settings it returns", async () => {
    const m = setup();
    await m.call("voice_key_set", { account: "groq", secret: "sekret" });
    const s = (await m.call("voice_settings_get", {})) as Record<string, unknown>;
    expect(s.has_groq_key).toBe(true);
    expect(JSON.stringify(s)).not.toContain("sekret");
  });

  it("lists and clears the privacy log", async () => {
    const m = setup();
    expect(((await m.call("privacy_log_list", { limit: 10 })) as unknown[]).length).toBeGreaterThan(0);
    await m.call("privacy_log_clear", {});
    expect(await m.call("privacy_log_list", { limit: 10 })).toEqual([]);
  });
});

describe("meeting commands on the mock", () => {
  it("needs consent to record, and blocks the assistant while recording", async () => {
    const m = setup();
    await enable(m);
    expect(await code(m.call("meeting_rec_start", { title: "t", participants: [], liveCaptions: false, consent: false }))).toBe("consent_required");
    const s = (await m.call("meeting_rec_start", { title: "t", participants: [], liveCaptions: false, consent: true })) as { id: string };
    expect(await code(m.call("voice_ptt_start", {}))).toBe("busy");
    await m.call("meeting_rec_stop", { sessionId: s.id });
    expect(((await m.call("voice_status", {})) as { state: string }).state).toBe("idle");
  });

  it("imports audio, reports the job and rejects unsupported files", async () => {
    const m = setup();
    expect(await code(m.call("meeting_import", { title: "t", participants: [], path: "notes.txt" }))).toBe("unsupported_audio");
    const job = (await m.call("meeting_import", { title: "t", participants: [], path: "C:\\a\\launch.wav" })) as { meeting_id: string };
    // Window events are shared by every mock in the process, so wait for this meeting's job.
    await new Promise<void>((res) => {
      const off = onNamed("meeting://job", (j) => { if (j.meeting_id === job.meeting_id && j.status === "completed") { off(); res(); } });
    });
    expect(((await m.call("meeting_job_get", { meetingId: job.meeting_id })) as { status: string }).status).toBe("completed");
  });

  it("returns the transcript, extraction with quotes, commitments and a recall answer", async () => {
    const m = setup();
    expect(((await m.call("meeting_transcript_get", { meetingId: "m-as1" })) as unknown[]).length).toBeGreaterThan(3);
    const ex = (await m.call("meeting_extract", { meetingId: "m-as1" })) as { items: { quote: string }[] };
    expect(ex.items.every((i) => i.quote.length > 0)).toBe(true);
    expect(((await m.call("commitments_list", { owner: "Ravi", status: null })) as unknown[]).length).toBe(1);
    const r = (await m.call("meeting_recall", { question: "Who owns the copy?", language: null })) as { citations: unknown[] };
    expect(r.citations.length).toBe(1);
    expect(await code(m.call("meeting_recall", { question: " ", language: null }))).toBe("core");
  });
});
