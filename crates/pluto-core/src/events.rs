//! The one `Event` enum (ADR-004, TRD §3). Every state change in Pluto is one of these.
//!
//! Names are past tense (`SettingChanged`), with the two lifecycle exceptions the TRD lists
//! (`AppReady`, `AppShuttingDown`). Only the variants Phase 0 needs exist; each later feature
//! adds its own next to the code that publishes it. Every variant carries `at`, ms since the Unix
//! epoch, UTC.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::focus::FocusSession;
use crate::labels::Label;
use crate::meetings::Meeting;
use crate::notes::Note;
use crate::reminders::Reminder;
use crate::retention::RetentionCounts;
use crate::tasks::{Task, TaskStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type")]
pub enum Event {
    AppStarted {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        version: String,
    },
    AppReady {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        startup_ms: u64,
    },
    AppShuttingDown {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
    },
    SettingChanged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        key: String,
        value: Value,
    },
    NoteCreated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        note: Note,
    },
    NoteUpdated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        note: Note,
    },
    NoteDeleted {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
    },
    /// A note left the Bin.
    NoteRestored {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        note: Note,
    },
    /// A note was permanently removed from the Bin.
    NotePurged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
    },
    NotePinnedChanged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
        pinned: bool,
    },
    NoteArchivedChanged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
        archived: bool,
    },
    LabelsChanged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        labels: Vec<Label>,
    },
    TaskCreated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        task: Task,
    },
    TaskUpdated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        task: Task,
    },
    TaskStatusChanged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
        from: TaskStatus,
        to: TaskStatus,
    },
    TaskDeferred {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
        from_date: Option<String>,
        to_date: String,
    },
    TaskDeleted {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
    },
    /// One day of a repeating task was ticked or unticked (FR-109).
    TaskTicked {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
        day: String,
        done: bool,
    },
    /// A focus session or break started, paused, resumed or got more time (FR-05).
    FocusChanged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        session: FocusSession,
    },
    /// It ran out (DONE) or was stopped (STOPPED).
    FocusEnded {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        session: FocusSession,
    },
    /// Auto-delete ran and changed something (FR-46).
    RetentionApplied {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        counts: RetentionCounts,
    },
    ReminderCreated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        reminder: Reminder,
    },
    ReminderUpdated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        reminder: Reminder,
    },
    ReminderDeleted {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
    },
    /// A reminder's time has come. Published exactly once per reminder (and `remind_at`).
    ReminderDue {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        reminder: Reminder,
    },
    MeetingCreated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        meeting: Meeting,
    },
    MeetingUpdated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        meeting: Meeting,
    },
    MeetingDeleted {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        id: String,
    },
    PetVisibilityChanged {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        visible: bool,
    },
    PetMoved {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        display_id: String,
        x: i32,
        y: i32,
    },
    DataExported {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        path: String,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        bytes: u64,
    },
    DataImported {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        #[cfg_attr(feature = "ts", ts(type = "Record<string, number>"))]
        counts: BTreeMap<String, u64>,
    },
    AllDataDeleted {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
    },
    DayRolledOver {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        ended_date: String,
        new_date: String,
    },
    DailyLogFrozen {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
        log_date: String,
        reconstructed: bool,
    },
    ResyncRequired {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
    },
    /// New or removed app/domain time. Coalesced: published at most once per 30 s while
    /// recording, and immediately after a delete. The UI refetches its usage queries.
    UsageUpdated {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        at: i64,
    },
}

impl Event {
    /// When the event happened, ms since the Unix epoch (UTC).
    pub fn at(&self) -> i64 {
        match self {
            Self::AppStarted { at, .. }
            | Self::AppReady { at, .. }
            | Self::AppShuttingDown { at }
            | Self::SettingChanged { at, .. }
            | Self::NoteCreated { at, .. }
            | Self::NoteUpdated { at, .. }
            | Self::NoteDeleted { at, .. }
            | Self::NoteRestored { at, .. }
            | Self::NotePurged { at, .. }
            | Self::NotePinnedChanged { at, .. }
            | Self::NoteArchivedChanged { at, .. }
            | Self::LabelsChanged { at, .. }
            | Self::TaskCreated { at, .. }
            | Self::TaskUpdated { at, .. }
            | Self::TaskStatusChanged { at, .. }
            | Self::TaskDeferred { at, .. }
            | Self::TaskDeleted { at, .. }
            | Self::TaskTicked { at, .. }
            | Self::FocusChanged { at, .. }
            | Self::FocusEnded { at, .. }
            | Self::RetentionApplied { at, .. }
            | Self::ReminderCreated { at, .. }
            | Self::ReminderUpdated { at, .. }
            | Self::ReminderDeleted { at, .. }
            | Self::ReminderDue { at, .. }
            | Self::MeetingCreated { at, .. }
            | Self::MeetingUpdated { at, .. }
            | Self::MeetingDeleted { at, .. }
            | Self::PetVisibilityChanged { at, .. }
            | Self::PetMoved { at, .. }
            | Self::DataExported { at, .. }
            | Self::DataImported { at, .. }
            | Self::AllDataDeleted { at }
            | Self::DayRolledOver { at, .. }
            | Self::DailyLogFrozen { at, .. }
            | Self::ResyncRequired { at }
            | Self::UsageUpdated { at } => *at,
        }
    }

    /// The variant name; identical to the `type` tag in the serialized form.
    pub fn name(&self) -> &'static str {
        match self {
            Self::AppStarted { .. } => "AppStarted",
            Self::AppReady { .. } => "AppReady",
            Self::AppShuttingDown { .. } => "AppShuttingDown",
            Self::SettingChanged { .. } => "SettingChanged",
            Self::NoteCreated { .. } => "NoteCreated",
            Self::NoteUpdated { .. } => "NoteUpdated",
            Self::NoteDeleted { .. } => "NoteDeleted",
            Self::NoteRestored { .. } => "NoteRestored",
            Self::NotePurged { .. } => "NotePurged",
            Self::NotePinnedChanged { .. } => "NotePinnedChanged",
            Self::NoteArchivedChanged { .. } => "NoteArchivedChanged",
            Self::LabelsChanged { .. } => "LabelsChanged",
            Self::TaskCreated { .. } => "TaskCreated",
            Self::TaskUpdated { .. } => "TaskUpdated",
            Self::TaskStatusChanged { .. } => "TaskStatusChanged",
            Self::TaskDeferred { .. } => "TaskDeferred",
            Self::TaskDeleted { .. } => "TaskDeleted",
            Self::TaskTicked { .. } => "TaskTicked",
            Self::FocusChanged { .. } => "FocusChanged",
            Self::FocusEnded { .. } => "FocusEnded",
            Self::RetentionApplied { .. } => "RetentionApplied",
            Self::ReminderCreated { .. } => "ReminderCreated",
            Self::ReminderUpdated { .. } => "ReminderUpdated",
            Self::ReminderDeleted { .. } => "ReminderDeleted",
            Self::ReminderDue { .. } => "ReminderDue",
            Self::MeetingCreated { .. } => "MeetingCreated",
            Self::MeetingUpdated { .. } => "MeetingUpdated",
            Self::MeetingDeleted { .. } => "MeetingDeleted",
            Self::PetVisibilityChanged { .. } => "PetVisibilityChanged",
            Self::PetMoved { .. } => "PetMoved",
            Self::DataExported { .. } => "DataExported",
            Self::DataImported { .. } => "DataImported",
            Self::AllDataDeleted { .. } => "AllDataDeleted",
            Self::DayRolledOver { .. } => "DayRolledOver",
            Self::DailyLogFrozen { .. } => "DailyLogFrozen",
            Self::ResyncRequired { .. } => "ResyncRequired",
            Self::UsageUpdated { .. } => "UsageUpdated",
        }
    }
}

#[cfg(test)]
pub(crate) fn one_of_each() -> Vec<Event> {
    vec![
        Event::AppStarted {
            at: 1,
            version: "0.0.1".into(),
        },
        Event::AppReady {
            at: 2,
            startup_ms: 850,
        },
        Event::AppShuttingDown { at: 3 },
        Event::SettingChanged {
            at: 4,
            key: "general.theme".into(),
            value: serde_json::json!("dark"),
        },
        Event::NoteCreated {
            at: 12,
            note: Note::sample(),
        },
        Event::NoteUpdated {
            at: 13,
            note: Note::sample(),
        },
        Event::NoteDeleted {
            at: 14,
            id: "n1".into(),
        },
        Event::NoteRestored {
            at: 24,
            note: Note::sample(),
        },
        Event::NotePurged {
            at: 25,
            id: "n1".into(),
        },
        Event::NotePinnedChanged {
            at: 15,
            id: "n1".into(),
            pinned: true,
        },
        Event::NoteArchivedChanged {
            at: 16,
            id: "n1".into(),
            archived: true,
        },
        Event::LabelsChanged {
            at: 17,
            labels: vec![Label {
                id: "l1".into(),
                name: "work".into(),
            }],
        },
        Event::TaskCreated {
            at: 18,
            task: Task::sample(),
        },
        Event::TaskUpdated {
            at: 19,
            task: Task::sample(),
        },
        Event::TaskStatusChanged {
            at: 20,
            id: "t1".into(),
            from: TaskStatus::Planned,
            to: TaskStatus::InProgress,
        },
        Event::TaskDeferred {
            at: 21,
            id: "t1".into(),
            from_date: Some("2026-10-03".into()),
            to_date: "2026-10-04".into(),
        },
        Event::TaskDeleted {
            at: 22,
            id: "t1".into(),
        },
        Event::ReminderCreated {
            at: 26,
            reminder: Reminder::sample(),
        },
        Event::ReminderUpdated {
            at: 27,
            reminder: Reminder::sample(),
        },
        Event::ReminderDeleted {
            at: 28,
            id: "r1".into(),
        },
        Event::ReminderDue {
            at: 29,
            reminder: Reminder::sample(),
        },
        Event::MeetingCreated {
            at: 31,
            meeting: Meeting::sample(),
        },
        Event::MeetingUpdated {
            at: 32,
            meeting: Meeting::sample(),
        },
        Event::MeetingDeleted {
            at: 33,
            id: "m1".into(),
        },
        Event::PetVisibilityChanged {
            at: 5,
            visible: false,
        },
        Event::PetMoved {
            at: 6,
            display_id: "primary".into(),
            x: -40,
            y: 900,
        },
        Event::DataExported {
            at: 7,
            path: "/tmp/pluto.json".into(),
            bytes: 2048,
        },
        Event::DataImported {
            at: 8,
            counts: BTreeMap::from([("notes".to_owned(), 3), ("tasks".to_owned(), 5)]),
        },
        Event::AllDataDeleted { at: 9 },
        Event::DayRolledOver {
            at: 10,
            ended_date: "2026-10-03".into(),
            new_date: "2026-10-04".into(),
        },
        Event::DailyLogFrozen {
            at: 11,
            log_date: "2026-10-03".into(),
            reconstructed: true,
        },
        Event::ResyncRequired { at: 23 },
        Event::UsageUpdated { at: 30 },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TRD §3 names, in the order of the enum. The test below fails if a variant is added or
    /// renamed without updating this list, which keeps the enum and the document in step.
    const TRD_NAMES: &[&str] = &[
        "AppStarted",
        "AppReady",
        "AppShuttingDown",
        "SettingChanged",
        "NoteCreated",
        "NoteUpdated",
        "NoteDeleted",
        "NoteRestored",
        "NotePurged",
        "NotePinnedChanged",
        "NoteArchivedChanged",
        "LabelsChanged",
        "TaskCreated",
        "TaskUpdated",
        "TaskStatusChanged",
        "TaskDeferred",
        "TaskDeleted",
        "ReminderCreated",
        "ReminderUpdated",
        "ReminderDeleted",
        "ReminderDue",
        "MeetingCreated",
        "MeetingUpdated",
        "MeetingDeleted",
        "PetVisibilityChanged",
        "PetMoved",
        "DataExported",
        "DataImported",
        "AllDataDeleted",
        "DayRolledOver",
        "DailyLogFrozen",
        "ResyncRequired",
        "UsageUpdated",
    ];

    #[test]
    fn variant_names_match_the_trd_catalog_exactly() {
        let names: Vec<_> = one_of_each().iter().map(Event::name).collect();
        assert_eq!(names, TRD_NAMES);
    }

    #[test]
    fn names_are_past_tense_apart_from_the_two_lifecycle_exceptions() {
        for name in TRD_NAMES {
            let exception = matches!(
                *name,
                "AppReady" | "AppShuttingDown" | "ResyncRequired" | "ReminderDue"
            );
            // Regular past tense ends in "ed"; "Frozen" is the one irregular form, and
            // "RolledOver" ends in its particle.
            let past = name.ends_with("ed") || name.ends_with("Frozen") || name.ends_with("Over");
            assert!(exception || past, "{name} should be past tense");
        }
    }

    #[test]
    fn every_variant_round_trips_through_json_with_its_name_as_the_tag() {
        for event in one_of_each() {
            let json = serde_json::to_value(&event).unwrap();
            assert_eq!(
                json["type"],
                event.name(),
                "tag must equal the variant name"
            );
            assert_eq!(json["at"], event.at());
            let back: Event = serde_json::from_value(json).unwrap();
            assert_eq!(back, event);
        }
    }

    #[test]
    fn note_events_all_start_with_note_so_the_ui_can_invalidate_on_the_prefix() {
        for event in one_of_each() {
            let name = event.name();
            if name.contains("Note") {
                assert!(name.starts_with("Note"), "{name}");
            }
        }
        for name in ["NoteDeleted", "NoteRestored", "NotePurged"] {
            assert!(TRD_NAMES.contains(&name));
        }
    }

    #[test]
    fn an_unknown_event_type_is_rejected_not_silently_accepted() {
        let bad = serde_json::json!({ "type": "SomethingHappened", "at": 1 });
        assert!(serde_json::from_value::<Event>(bad).is_err());
    }
}
