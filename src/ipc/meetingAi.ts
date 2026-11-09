// Typed client for the meeting-AI commands (API_CONTRACTS §4.3). Errors are `VoiceError`
// (see `voiceErrorMessage`). Glossary commands arrive with the P1 glossary task.
import { call } from "./transport";
import type {
  ApplyExtraction, Commitment, CommitmentStatus, ExtractedItem, ExtractionResult, RecallAnswer, RecordingSession, SpeakerMapping,
  TranscriptionJob, TranscriptSegment,
} from "./generated";
import type { Meeting } from "./types";

/** One line of a transcript that matches a search, with the meeting it is in. */
export interface TranscriptHit {
  meeting_id: string;
  meeting_title: string;
  meeting_starts_at: number;
  segment_id: string;
  start_ms: number;
  speaker: string | null;
  text: string;
}

export const meetingAi = {
  recStart: (title: string, participants: string[], liveCaptions: boolean, consent: boolean, systemAudio = false) =>
    call<RecordingSession>("meeting_rec_start", { title, participants, liveCaptions, consent, systemAudio }),
  recPause: (sessionId: string) => call<RecordingSession>("meeting_rec_pause", { sessionId }),
  recResume: (sessionId: string) => call<RecordingSession>("meeting_rec_resume", { sessionId }),
  /** Also queues the batch transcription job. */
  recStop: (sessionId: string) => call<RecordingSession>("meeting_rec_stop", { sessionId }),
  /** Opens the system file dialog for an audio file; `null` if cancelled. */
  pickAudio: () => call<string | null>("meeting_audio_pick"),
  import: (title: string, participants: string[], path: string) =>
    call<TranscriptionJob>("meeting_import", { title, participants, path }),
  jobGet: (meetingId: string) => call<TranscriptionJob | null>("meeting_job_get", { meetingId }),
  jobRetry: (meetingId: string) => call<TranscriptionJob>("meeting_job_retry", { meetingId }),
  transcriptGet: (meetingId: string) => call<TranscriptSegment[]>("meeting_transcript_get", { meetingId }),
  /** Search as you type across every transcript ("budg" finds "budget"). Best matches first. */
  transcriptSearch: (query: string) => call<TranscriptHit[]>("meeting_transcript_search", { query }),
  speakersSet: (meetingId: string, mapping: SpeakerMapping[]) =>
    call<null>("meeting_speakers_set", { meetingId, mapping }),
  extract: (meetingId: string) => call<ExtractionResult>("meeting_extract", { meetingId }),
  extractionApply: (input: ApplyExtraction) => call<Meeting>("meeting_extraction_apply", { input }),
  itemsGet: (meetingId: string) => call<ExtractedItem[]>("meeting_items_get", { meetingId }),
  recall: (question: string, language: string | null = null) => call<RecallAnswer>("meeting_recall", { question, language }),
  /** A short written recap in `language` (for example `hi-IN`), from the applied items only. */
  recapText: (meetingId: string, language: string) => call<string>("meeting_recap_text", { meetingId, language }),
  /** Returns the mp3 path when `export` is true. */
  recap: (meetingId: string, language: string, speak: boolean, exportMp3: boolean) =>
    call<string | null>("meeting_recap", { meetingId, language, speak, export: exportMp3 }),
  commitmentsList: (owner: string | null = null, status: CommitmentStatus | null = null) =>
    call<Commitment[]>("commitments_list", { owner, status }),
} as const;
