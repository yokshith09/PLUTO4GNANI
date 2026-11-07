//! The meeting-AI service the shell commands call (T-203/T-204 wiring): transcript with speaker
//! names, speaker mapping, extraction, apply, and the applied items. Thin on purpose: the logic is
//! in `extraction` and `apply`; this reads and writes the database around them.

use std::sync::Arc;

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::Llm;
use pluto_contracts::meeting::{
    ApplyExtraction, ExtractedItem, ExtractionResult, JobStatus, Relevance, SegmentSource,
    SpeakerMapping, TranscriptSegment, TranscriptionJob,
};
use pluto_core::clock::Clock;
use pluto_core::db::Database;
use pluto_core::error::AppError;
use pluto_core::ids::new_id;
use pluto_core::meeting_ai_store as store;
use pluto_core::meetings::{self, Meeting, MeetingInput};
use pluto_core::tasks::{self, TaskFilters, TaskView};

use super::apply::{apply, ApplyOutcome};
use super::commitments;
use super::extraction::{extract, ExtractInput, OpenTask};
use super::recall::{self, Hit};
use super::recap::{self, RecapAction, RecapInput};
use super::ExtractionCache;

fn core(e: AppError) -> VoiceError {
    VoiceError::new(VoiceErrorCode::Core, e.message)
}

fn relevance_of(s: &str) -> Option<Relevance> {
    match s {
        "decision" => Some(Relevance::Decision),
        "action_item" => Some(Relevance::ActionItem),
        "question" => Some(Relevance::Question),
        "risk" => Some(Relevance::Risk),
        "chitchat" => Some(Relevance::Chitchat),
        _ => None,
    }
}

fn job_status_of(s: &str) -> JobStatus {
    match s {
        "uploading" => JobStatus::Uploading,
        "created" => JobStatus::Created,
        "in_progress" => JobStatus::InProgress,
        "completed" => JobStatus::Completed,
        "partial_failure" => JobStatus::PartialFailure,
        "failed" => JobStatus::Failed,
        "cancelled" => JobStatus::Cancelled,
        _ => JobStatus::Queued,
    }
}

pub struct MeetingAi {
    db: Arc<Database>,
    clock: Arc<dyn Clock>,
    llm: Arc<dyn Llm>,
    cache: ExtractionCache,
}

impl MeetingAi {
    pub fn new(db: Arc<Database>, clock: Arc<dyn Clock>, llm: Arc<dyn Llm>) -> Self {
        Self {
            db,
            clock,
            llm,
            cache: ExtractionCache::new(),
        }
    }

    async fn meeting(&self, id: &str) -> VoiceResult<Meeting> {
        meetings::get(&self.db, id).await.map_err(core)
    }

    /// The newest transcription job of a meeting; `None` for a meeting that was typed by hand.
    pub async fn job(&self, meeting_id: &str) -> VoiceResult<Option<TranscriptionJob>> {
        let id = meeting_id.to_owned();
        let row = self
            .db
            .read(move |c| store::latest_job(c, &id))
            .await
            .map_err(core)?;
        Ok(row.map(|r| TranscriptionJob {
            id: r.id,
            meeting_id: r.meeting_id,
            gnani_job_id: r.gnani_job_id,
            status: job_status_of(&r.status),
            error: r.error,
            updated_at: r.updated_at,
        }))
    }

    /// **Stand-in until Gnani Batch (T-201) is connected.** Makes the meeting and gives it the sample
    /// transcript instead of sending the audio anywhere, so the rest of the journey (speakers,
    /// extraction, review, tasks, recap, recall) can run for real. The job it records is "completed".
    pub async fn import_with_sample(
        &self,
        title: &str,
        participants: Vec<String>,
        sample: Vec<TranscriptSegment>,
    ) -> VoiceResult<TranscriptionJob> {
        let now = self.clock.now_ms();
        let meeting = meetings::create(
            &self.db,
            &*self.clock,
            MeetingInput {
                title: if title.trim().is_empty() {
                    "Imported meeting".into()
                } else {
                    title.trim().to_owned()
                },
                participants,
                ..Default::default()
            },
        )
        .await
        .map_err(core)?;
        let (mid, job_id) = (meeting.id.clone(), new_id());
        let rows: Vec<store::SegmentRow> = sample
            .into_iter()
            .enumerate()
            .map(|(i, s)| store::SegmentRow {
                id: new_id(),
                meeting_id: mid.clone(),
                index: u32::try_from(i).unwrap_or(u32::MAX),
                start_ms: s.start_ms.unwrap_or(0),
                end_ms: s.end_ms.unwrap_or(0),
                speaker_label: s.speaker_label,
                participant_position: None,
                text: s.text,
                language: s.language,
                relevance: None,
            })
            .collect();
        let jid = job_id.clone();
        let m2 = mid.clone();
        self.db
            .write(move |tx| {
                store::replace_segments(tx, &m2, &rows)?;
                store::insert_job(tx, &jid, &m2, "{\"source\":\"sample\"}", now)?;
                store::update_job(tx, &jid, "completed", None, None, now)?;
                store::set_status(tx, &m2, "needs_speakers", None, now)?;
                Ok(((), vec![]))
            })
            .await
            .map_err(core)?;
        Ok(TranscriptionJob {
            id: job_id,
            meeting_id: mid,
            gnani_job_id: None,
            status: JobStatus::Completed,
            error: None,
            updated_at: now,
        })
    }

    /// The transcript in order, each line with its speaker's name once mapped.
    pub async fn transcript(&self, meeting_id: &str) -> VoiceResult<Vec<TranscriptSegment>> {
        let meeting = self.meeting(meeting_id).await?;
        let id = meeting_id.to_owned();
        let rows = self
            .db
            .read(move |c| store::list_segments(c, &id))
            .await
            .map_err(core)?;
        Ok(rows
            .into_iter()
            .map(|r| TranscriptSegment {
                participant: r
                    .participant_position
                    .and_then(|p| meeting.participants.get(p as usize).cloned()),
                id: r.id,
                meeting_id: Some(r.meeting_id),
                source: SegmentSource::Batch,
                index: r.index,
                start_ms: Some(r.start_ms),
                end_ms: Some(r.end_ms),
                speaker_label: r.speaker_label,
                text: r.text,
                language: r.language,
                relevance: r.relevance.as_deref().and_then(relevance_of),
            })
            .collect())
    }

    /// Name speakers. A name must be one of the meeting's participants; `None` clears it.
    pub async fn set_speakers(
        &self,
        meeting_id: &str,
        mapping: &[SpeakerMapping],
    ) -> VoiceResult<()> {
        let meeting = self.meeting(meeting_id).await?;
        let mut positions = Vec::new();
        for m in mapping {
            let pos = match &m.participant {
                None => None,
                Some(name) => Some(
                    meeting
                        .participants
                        .iter()
                        .position(|p| p.eq_ignore_ascii_case(name))
                        .ok_or_else(|| {
                            VoiceError::new(
                                VoiceErrorCode::Core,
                                "Choose one of the meeting's participants.",
                            )
                        })? as u32,
                ),
            };
            positions.push((m.speaker_label, pos));
        }
        let (id, now) = (meeting_id.to_owned(), self.clock.now_ms());
        self.db
            .write(move |tx| {
                store::set_speaker_positions(tx, &id, &positions)?;
                store::set_status(tx, &id, "needs_review", None, now)?;
                Ok(((), vec![]))
            })
            .await
            .map_err(core)
    }

    /// Find decisions, action items, questions and risks; remember them for `apply`.
    pub async fn extract(&self, meeting_id: &str) -> VoiceResult<ExtractionResult> {
        let meeting = self.meeting(meeting_id).await?;
        let segments = self.transcript(meeting_id).await?;
        if segments.is_empty() {
            return Err(VoiceError::new(
                VoiceErrorCode::Core,
                "This meeting has no transcript yet.",
            ));
        }
        let open_tasks = tasks::query(
            &self.db,
            &*self.clock,
            TaskView::All,
            TaskFilters::default(),
        )
        .await
        .map_err(core)?
        .into_iter()
        .filter(|r| {
            !matches!(
                r.task.status,
                tasks::TaskStatus::Completed | tasks::TaskStatus::Cancelled
            )
        })
        .map(|r| OpenTask {
            id: r.task.id,
            title: r.task.title,
        })
        .collect();
        let result = extract(
            &*self.llm,
            &ExtractInput {
                meeting_id: meeting_id.to_owned(),
                meeting_date: self.clock.local_date(meeting.starts_at),
                participants: meeting.participants,
                segments,
                open_tasks,
            },
        )
        .await?;
        self.cache.put(result.clone());
        Ok(result)
    }

    pub async fn apply(&self, input: &ApplyExtraction) -> VoiceResult<ApplyOutcome> {
        let extraction = self.cache.get(&input.meeting_id).ok_or_else(|| {
            VoiceError::new(
                VoiceErrorCode::Core,
                "That review has expired. Find the items again.",
            )
        })?;
        apply(&self.db, &*self.clock, &extraction, input).await
    }

    /// Answer a question from the user's own transcripts, with citations; never from outside them.
    pub async fn recall(
        &self,
        question: &str,
        language: Option<&str>,
    ) -> VoiceResult<pluto_contracts::meeting::RecallAnswer> {
        let language = language.unwrap_or("en-IN");
        let question = question.trim();
        if question.is_empty() {
            return Err(VoiceError::new(
                VoiceErrorCode::Core,
                "Ask a question first.",
            ));
        }
        let terms = recall::search_terms(question);
        let rows = self
            .db
            .read(move |c| store::search_segments_any(c, &terms, recall::TOP_K))
            .await
            .map_err(core)?;
        let mut hits = Vec::new();
        let mut cache: std::collections::HashMap<String, Meeting> =
            std::collections::HashMap::new();
        for r in rows {
            if !cache.contains_key(&r.meeting_id) {
                // A meeting deleted since the index was written has no row; skip its lines.
                let Ok(m) = meetings::get(&self.db, &r.meeting_id).await else {
                    continue;
                };
                cache.insert(r.meeting_id.clone(), m);
            }
            let m = &cache[&r.meeting_id];
            hits.push(Hit {
                said_at: m.starts_at + r.start_ms,
                meeting_title: m.title.clone(),
                segment: TranscriptSegment {
                    participant: r
                        .participant_position
                        .and_then(|p| m.participants.get(p as usize).cloned()),
                    id: r.id,
                    meeting_id: Some(r.meeting_id),
                    source: SegmentSource::Batch,
                    index: r.index,
                    start_ms: Some(r.start_ms),
                    end_ms: Some(r.end_ms),
                    speaker_label: r.speaker_label,
                    text: r.text,
                    language: r.language,
                    relevance: r.relevance.as_deref().and_then(relevance_of),
                },
            });
        }
        recall::answer(&*self.llm, question, language, &hits).await
    }

    /// A short recap in `language`, written only from the decisions and action items on the meeting.
    pub async fn recap_text(&self, meeting_id: &str, language: &str) -> VoiceResult<String> {
        let meeting = self.meeting(meeting_id).await?;
        let input = RecapInput {
            date: self.clock.local_date(meeting.starts_at),
            title: meeting.title,
            participants: meeting.participants,
            decisions: meeting.decisions.into_iter().map(|d| d.body).collect(),
            actions: meeting
                .action_items
                .into_iter()
                .map(|a| RecapAction {
                    body: a.body,
                    owner: a.assignee,
                    due_date: None,
                })
                .collect(),
        };
        let input = self.with_due_dates(meeting_id, input).await?;
        recap::generate(&*self.llm, &input, language).await
    }

    /// Fill in each action item's due date from the provenance saved when it was applied.
    async fn with_due_dates(
        &self,
        meeting_id: &str,
        mut input: RecapInput,
    ) -> VoiceResult<RecapInput> {
        let id = meeting_id.to_owned();
        let applied = self
            .db
            .read(move |c| store::applied_actions(c, &id))
            .await
            .map_err(core)?;
        for a in &mut input.actions {
            a.due_date = applied
                .iter()
                .find(|x| x.body == a.body)
                .and_then(|x| x.meta.as_ref())
                .and_then(|m| m.due_date.clone());
        }
        Ok(input)
    }

    /// Commitments across all meetings (who promised what, and whether it is late).
    pub async fn commitments(
        &self,
        owner: Option<&str>,
        status: Option<pluto_contracts::meeting::CommitmentStatus>,
    ) -> VoiceResult<Vec<pluto_contracts::meeting::Commitment>> {
        let rows = self.db.read(store::commitment_rows).await.map_err(core)?;
        Ok(commitments::list(rows, self.clock.today(), owner, status))
    }

    /// What was applied to the meeting, with the quote and segment each came from.
    pub async fn items(&self, meeting_id: &str) -> VoiceResult<Vec<ExtractedItem>> {
        let id = meeting_id.to_owned();
        let (decisions, actions, insights) = self
            .db
            .read(move |c| {
                Ok((
                    store::applied_decisions(c, &id)?,
                    store::applied_actions(c, &id)?,
                    store::list_insights(c, &id)?,
                ))
            })
            .await
            .map_err(core)?;
        let base = |id: String, kind, body: String, segment: Option<String>, quote: String| {
            ExtractedItem {
                id,
                kind,
                body,
                owner: None,
                due_phrase: None,
                due_date: None,
                priority: None,
                source_segment_id: segment.unwrap_or_default(),
                quote,
                duplicate_of_task_id: None,
            }
        };
        let mut out = Vec::new();
        for d in decisions {
            let (seg, quote) = d.decision_source.unwrap_or((None, String::new()));
            out.push(base(d.id, Relevance::Decision, d.body, seg, quote));
        }
        for a in actions {
            let m = a.meta;
            let mut item = base(
                a.id,
                Relevance::ActionItem,
                a.body,
                m.as_ref().and_then(|m| m.source_segment_id.clone()),
                m.as_ref().map(|m| m.quote.clone()).unwrap_or_default(),
            );
            item.owner = a.assignee;
            if let Some(m) = m {
                item.due_phrase = m.due_phrase;
                item.due_date = m.due_date;
                item.priority = match m.priority.as_deref() {
                    Some("LOW") => Some(tasks::Priority::Low),
                    Some("MEDIUM") => Some(tasks::Priority::Medium),
                    Some("HIGH") => Some(tasks::Priority::High),
                    _ => None,
                };
            }
            out.push(item);
        }
        for i in insights {
            let kind = if i.kind == "risk" {
                Relevance::Risk
            } else {
                Relevance::Question
            };
            out.push(base(i.id, kind, i.body, i.source_segment_id, String::new()));
        }
        Ok(out)
    }
}

#[async_trait::async_trait]
impl pluto_contracts::meeting::MeetingKnowledge for MeetingAi {
    async fn recall(
        &self,
        question: &str,
        language: Option<&str>,
    ) -> VoiceResult<pluto_contracts::meeting::RecallAnswer> {
        MeetingAi::recall(self, question, language).await
    }

    async fn open_commitments(&self) -> VoiceResult<Vec<pluto_contracts::meeting::Commitment>> {
        let all = MeetingAi::commitments(self, None, None).await?;
        Ok(all
            .into_iter()
            .filter(|c| {
                matches!(
                    c.status,
                    pluto_contracts::meeting::CommitmentStatus::Open
                        | pluto_contracts::meeting::CommitmentStatus::Overdue
                        | pluto_contracts::meeting::CommitmentStatus::NoTask
                )
            })
            .collect())
    }
}
