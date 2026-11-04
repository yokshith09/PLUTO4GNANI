// Mock handlers for the voice and meeting-AI commands, composed into `createMock` (mock.ts).
// Data comes from the shared fixtures in test-data/fixtures (also checked by a Rust test), so the
// UI and the Rust side agree. Every state is reachable, including the errors.
import { emitNamed } from "./namedEvents";
import type {
  ApplyExtraction, Commitment, ExtractionResult, PrivacyLogEntry, Proposal, ProposalResponse, RecallAnswer, RecordingSession, SpeakerMapping, ToolResult,
  TranscriptionJob, TranscriptSegment, UsageCounters, VoiceError, VoiceErrorCode, VoiceSettings, VoiceSettingsPatch, VoiceStatus,
} from "./generated";
import type { Meeting } from "./types";
import settingsDefault from "../../test-data/fixtures/voice/voice_settings.default.json";
import statusIdle from "../../test-data/fixtures/voice/voice_status.idle.json";
import usageSample from "../../test-data/fixtures/voice/usage_counters.sample.json";
import toolOk from "../../test-data/fixtures/voice/tool_result.ok.json";
import segmentsAs1 from "../../test-data/fixtures/meetings/transcript_segments.as1.json";
import extractionAs1 from "../../test-data/fixtures/meetings/extraction_result.as1.json";
import recallOk from "../../test-data/fixtures/meetings/recall_answer.ok.json";

type Args = Record<string, unknown>;
const PROPOSAL_WINDOW_MS = 30_000;
const KEY_FLAGS = { gnani: "has_gnani_key", gemini: "has_gemini_key", groq: "has_groq_key" } as const;

export interface VoiceMockDeps {
  now: () => number;
  /** Create the meeting row an import or recording makes; returns its id. */
  createMeeting: (title: string, participants: string[]) => string;
  /** Add decisions and action items to a base meeting, and make tasks for the ones asked; returns the meeting. */
  applyItems: (meetingId: string, decisions: string[], actions: { body: string; assignee: string | null; createTask: boolean }[]) => Meeting;
  /** Delay before simulated pipeline steps; tests pass 0. */
  delayMs?: number;
}

const voiceError = (code: VoiceErrorCode, message: string, retryable = false): never => {
  throw { code, message, retryable } satisfies VoiceError;
};

// Module-level so ids stay unique across mock instances (window events are shared by all of them).
let seq = 0;

export function createVoiceMock({ now, createMeeting, applyItems, delayMs = 250 }: VoiceMockDeps) {
  const id = (p: string) => `${p}-${++seq}`;
  const settings: VoiceSettings = structuredClone(settingsDefault.data) as VoiceSettings;
  let status: VoiceStatus = structuredClone(statusIdle.data) as VoiceStatus;
  const pending = new Map<string, Proposal>();
  const sessions = new Map<string, RecordingSession>();
  const jobs = new Map<string, TranscriptionJob>();
  const mappings = new Map<string, SpeakerMapping[]>();
  const privacy: PrivacyLogEntry[] = [
    { id: 1, at: now() - 120_000, destination: "gnani_realtime", purpose: "assistant command", bytes: 51_200, ref_id: null },
    { id: 2, at: now() - 90_000, destination: "groq", purpose: "assistant intent", bytes: 640, ref_id: null },
    { id: 3, at: now() - 60_000, destination: "gnani_batch", purpose: "meeting transcript", bytes: 2_400_000, ref_id: "m-as1" },
  ];
  const later = (f: () => void) => void setTimeout(f, delayMs);
  const setStatus = (patch: Partial<VoiceStatus>) => {
    status = { ...status, ...patch };
    emitNamed("voice://status", status);
  };
  const requireCloud = () => {
    if (!settings.cloud_voice_enabled) voiceError("cloud_voice_disabled", "Cloud voice is off. Turn it on in Settings to use the assistant.");
    if (!settings.has_gnani_key) voiceError("missing_api_key", "Add your Gnani key in Settings first.");
  };
  const busy = () => {
    if (status.state === "recording") voiceError("busy", "Busy: recording a meeting.", true);
  };
  const log = (destination: PrivacyLogEntry["destination"], purpose: string, bytes: number, ref: string | null = null) =>
    privacy.unshift({ id: (privacy[0]?.id ?? 0) + 1, at: now(), destination, purpose, bytes, ref_id: ref });
  const items = () => (extractionAs1.data as unknown as ExtractionResult).items;

  const proposeTask = (): Proposal => ({
    id: id("prop"), interaction_id: id("int"), tool: "task_create",
    args: { title: "Send the revised budget", due_date: "2026-11-06", due_phrase: "Friday" },
    summary_text: "Add “Send the revised budget”, due Friday?", language: "en-IN", expires_at: now() + PROPOSAL_WINDOW_MS,
  });

  const runJob = (job: TranscriptionJob) => {
    jobs.set(job.meeting_id, job);
    const step = (status: TranscriptionJob["status"], wait: number) =>
      void setTimeout(() => {
        const next = { ...(jobs.get(job.meeting_id) ?? job), status, updated_at: now() };
        jobs.set(job.meeting_id, next);
        emitNamed("meeting://job", next);
      }, wait);
    step("in_progress", delayMs);
    step("completed", delayMs * 2);
  };

  return {
    voice_settings_get: () => settings,
    voice_settings_set: (a: Args) => {
      Object.assign(settings, a.patch as VoiceSettingsPatch);
      setStatus({ state: settings.cloud_voice_enabled ? "idle" : "disabled", detail: null });
      return settings;
    },
    voice_key_set: (a: Args) => {
      const flag = KEY_FLAGS[a.account as keyof typeof KEY_FLAGS] ?? voiceError("core", "Unknown key account.");
      if (!String(a.secret).trim()) voiceError("invalid_api_key", "That key is empty.");
      settings[flag] = true;
      return null;
    },
    voice_key_clear: (a: Args) => {
      settings[KEY_FLAGS[a.account as keyof typeof KEY_FLAGS] ?? voiceError("core", "Unknown key account.")] = false;
      return null;
    },
    voice_status: () => status,
    voice_ptt_start: () => {
      requireCloud();
      busy();
      const session = id("sess");
      setStatus({ state: "listening", mode: "assistant", detail: null, session_id: session });
      return session;
    },
    voice_ptt_stop: () => {
      if (status.state !== "listening") return null;
      setStatus({ state: "thinking" });
      log("gnani_realtime", "assistant command", 41_000);
      later(() => {
        const p = proposeTask();
        pending.set(p.id, p);
        emitNamed("voice://proposal", p);
        setStatus({ state: "awaiting_approval", detail: null });
      });
      return null;
    },
    voice_cancel: () => {
      pending.clear();
      setStatus({ state: settings.cloud_voice_enabled ? "idle" : "disabled", mode: null, detail: null, session_id: null });
      return null;
    },
    audio_push: () => null,
    voice_proposal_respond: (a: Args): ToolResult => {
      const p = pending.get(String(a.proposalId)) ?? voiceError("proposal_expired", "That request is no longer waiting.");
      pending.delete(p.id);
      const idle: Partial<VoiceStatus> = { state: "idle", mode: null, detail: null, session_id: null };
      if (now() > p.expires_at) {
        setStatus(idle);
        return voiceError("proposal_expired", "That took too long, so nothing was changed.");
      }
      const response = a.response as ProposalResponse;
      setStatus(idle);
      if (response.kind === "reject") return { tool: p.tool, ok: false, message: "Okay, I didn't change anything.", record_id: null };
      return structuredClone(toolOk.data) as ToolResult;
    },
    voice_speak: () => null,
    voice_stop_speaking: () => null,
    voice_usage: () => structuredClone(usageSample.data) as UsageCounters,
    privacy_log_list: (a: Args) => privacy.slice(0, Number(a.limit ?? 100)),
    privacy_log_clear: () => { privacy.length = 0; return null; },

    meeting_rec_start: (a: Args): RecordingSession => {
      if (!a.consent) voiceError("consent_required", "Confirm everyone knows the meeting is being recorded.");
      busy();
      const s: RecordingSession = { id: id("rec"), meeting_id: createMeeting(String(a.title), (a.participants as string[]) ?? []), started_at: now(), live_captions: Boolean(a.liveCaptions), chunk_count: 0 };
      sessions.set(s.id, s);
      setStatus({ state: "recording", mode: "meeting", detail: "Recording", session_id: s.id });
      return s;
    },
    meeting_rec_pause: (a: Args) => sessions.get(String(a.sessionId)) ?? voiceError("core", "That recording isn't active."),
    meeting_rec_resume: (a: Args) => sessions.get(String(a.sessionId)) ?? voiceError("core", "That recording isn't active."),
    meeting_rec_stop: (a: Args) => {
      const s = sessions.get(String(a.sessionId)) ?? voiceError("core", "That recording isn't active.");
      sessions.delete(s.id);
      setStatus({ state: settings.cloud_voice_enabled ? "idle" : "disabled", mode: null, detail: null, session_id: null });
      log("gnani_batch", "meeting transcript", 2_400_000, s.meeting_id);
      runJob({ id: id("job"), meeting_id: s.meeting_id, gnani_job_id: null, status: "queued", error: null, updated_at: now() });
      return { ...s, chunk_count: s.chunk_count + 1 };
    },
    meeting_audio_pick: () => "C:\\Users\\you\\Recordings\\launch-sync.wav",
    meeting_import: (a: Args): TranscriptionJob => {
      const path = String(a.path);
      if (!/\.(wav|mp3|m4a|flac|ogg)$/i.test(path)) voiceError("unsupported_audio", "Choose a WAV, MP3, M4A, FLAC or OGG file.");
      const job: TranscriptionJob = { id: id("job"), meeting_id: createMeeting(String(a.title), (a.participants as string[]) ?? []), gnani_job_id: null, status: "queued", error: null, updated_at: now() };
      log("gnani_batch", "meeting transcript", 2_400_000, job.meeting_id);
      runJob(job);
      return job;
    },
    meeting_job_get: (a: Args) => jobs.get(String(a.meetingId)) ?? null,
    meeting_job_retry: (a: Args): TranscriptionJob => {
      const old = jobs.get(String(a.meetingId)) ?? voiceError("core", "There is no transcription to retry.");
      const job = { ...old, status: "queued" as const, error: null, updated_at: now() };
      runJob(job);
      return job;
    },
    // Speakers start unnamed (as straight from the transcription) until the user maps them.
    meeting_transcript_get: (a: Args) => {
      const map = mappings.get(String(a.meetingId));
      return (structuredClone(segmentsAs1.data) as TranscriptSegment[]).map((s) => ({
        ...s, participant: map?.find((m) => m.speaker_label === s.speaker_label)?.participant ?? null,
      }));
    },
    meeting_speakers_set: (a: Args) => { mappings.set(String(a.meetingId), a.mapping as SpeakerMapping[]); return null; },
    meeting_extract: (a: Args) => ({ ...(structuredClone(extractionAs1.data) as ExtractionResult), meeting_id: String(a.meetingId) }),
    meeting_extraction_apply: (a: Args) => {
      const input = a.input as ApplyExtraction;
      const byId = new Map(items().map((i) => [i.id, i]));
      const picked = input.accepted.flatMap((e) => { const o = byId.get(e.item_id); return o ? [{ e, o }] : []; });
      if (!picked.length && input.accepted.length) voiceError("core", "Those items are no longer in this review.");
      return applyItems(
        input.meeting_id,
        picked.filter((p) => p.o.kind === "decision").map((p) => p.e.body),
        picked.filter((p) => p.o.kind === "action_item").map((p) => ({ body: p.e.body, assignee: p.e.owner, createTask: p.e.create_task })),
      );
    },
    meeting_items_get: () => structuredClone(items()),
    meeting_recall: (a: Args): RecallAnswer => {
      if (!String(a.question).trim()) voiceError("core", "Ask a question first.");
      return structuredClone(recallOk.data) as RecallAnswer;
    },
    meeting_recap_text: (a: Args) => {
      const lang = String(a.language);
      if (!lang) voiceError("core", "Choose a language.");
      return lang.startsWith("hi")
        ? "लॉन्च 20 नवंबर को होगा। रवि गुरुवार तक प्राइसिंग पेज की कॉपी पूरी करेंगे और मीना सोमवार तक कानूनी टीम से अनुबंध की पुष्टि करेंगी।"
        : "Launch is on 20 November, starting with the annual plan. Ravi will finish the pricing page copy by Thursday, and Meena will confirm the vendor contract with legal by Monday.";
    },
    meeting_recap: (a: Args) => (a.export ? "C:\\Users\\you\\Documents\\Pluto\\recap-launch-sync.mp3" : null),
    commitments_list: (a: Args): Commitment[] =>
      items().filter((i) => i.kind === "action_item" && (!a.owner || i.owner === a.owner)).map((i) => ({
        action_item_id: i.id, meeting_id: "m-as1", meeting_title: "Launch sync", owner: i.owner, body: i.body,
        due_date: i.due_date, task_id: null, status: "no_task" as const, days_overdue: null,
      })).filter((c) => !a.status || c.status === a.status),
  };
}
