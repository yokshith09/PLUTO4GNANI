//! The meeting-AI service the shell commands call (T-203/T-204 wiring): transcript with speaker
//! names, speaker mapping, extraction, apply, and the applied items. Thin on purpose: the logic is
//! in `extraction` and `apply`; this reads and writes the database around them.

use std::sync::Arc;

use pluto_contracts::error::{VoiceError, VoiceErrorCode, VoiceResult};
use pluto_contracts::llm::Llm;
use pluto_contracts::meeting::{
    ApplyExtraction, ExtractedItem, ExtractionResult, Relevance, SegmentSource, SpeakerMapping,
    TranscriptSegment,
};
use pluto_core::clock::Clock;
use pluto_core::db::Database;
use pluto_core::error::AppError;
use pluto_core::meeting_ai_store as store;
use pluto_core::meetings::{self, Meeting};
use pluto_core::tasks::{self, TaskFilters, TaskView};

use super::apply::{apply, ApplyOutcome};
use super::extraction::{extract, ExtractInput, OpenTask};
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
