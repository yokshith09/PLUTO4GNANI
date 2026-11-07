//! T-303 / T-AS-05: no write without an approved proposal; "usse Friday kar do" resolves the
//! previous task; voice and click approval; expiry; ambiguity; one execution per proposal.

use std::sync::Mutex as StdMutex;

use chrono_tz::Tz;
use pluto_core::bus::EventBus;
use pluto_core::clock::FakeClock;

use super::*;

const START: i64 = 1_762_329_600_000; // 2025-11-05 13:30 IST

/// Answers each command from a script (rules for "haan"/"nahi" like D2's table).
struct Script(Vec<(&'static str, Intent, Option<&'static str>, Value)>);

#[async_trait]
impl Decider for Script {
    async fn decide(&self, text: &str, ctx: &DecideContext) -> VoiceResult<AssistantDecision> {
        let t = text.to_lowercase();
        if ["haan", "yes", "haan kar do"].contains(&t.as_str()) {
            return Ok(decision(
                Intent::Approval,
                None,
                json!({}),
                DecisionSource::Rules,
            ));
        }
        if ["nahi", "no", "cancel"].contains(&t.as_str()) {
            return Ok(decision(
                Intent::Reject,
                None,
                json!({}),
                DecisionSource::Rules,
            ));
        }
        let (_, intent, tool, args) = self
            .0
            .iter()
            .find(|(k, ..)| *k == text)
            .cloned()
            .unwrap_or(("", Intent::Noise, None, json!({})));
        // The context the LLM would see: the previous turns, so references can be resolved.
        CONTEXTS.lock().unwrap().push(ctx.recent_turns.clone());
        Ok(decision(intent, tool, args, DecisionSource::Llm))
    }
}

static CONTEXTS: StdMutex<Vec<Vec<String>>> = StdMutex::new(Vec::new());

fn decision(
    intent: Intent,
    tool: Option<&str>,
    args: Value,
    source: DecisionSource,
) -> AssistantDecision {
    AssistantDecision {
        intent,
        tool: tool.map(str::to_string),
        args,
        confidence: Some(0.9),
        source,
        latency_ms: 5,
    }
}

#[derive(Default)]
struct Seen {
    turns: StdMutex<Vec<AssistantTurn>>,
    proposals: StdMutex<Vec<Proposal>>,
}

impl AssistantEvents for Seen {
    fn turn(&self, t: &AssistantTurn) {
        self.turns.lock().unwrap().push(t.clone());
    }
    fn proposal(&self, p: &Proposal) {
        self.proposals.lock().unwrap().push(p.clone());
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    db: Arc<Database>,
    clock: Arc<FakeClock>,
    a: Arc<Assistant>,
    seen: Arc<Seen>,
}

fn rig(script: Script, window: Duration) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&dir.path().join("pluto.db"), EventBus::default()).unwrap());
    let clock = Arc::new(FakeClock::new(START, Tz::Asia__Kolkata));
    let seen = Arc::new(Seen::default());
    let a = Arc::new(
        Assistant::new(db.clone(), clock.clone(), Arc::new(script), seen.clone())
            .with_window(window),
    );
    Rig {
        _dir: dir,
        db,
        clock,
        a,
        seen,
    }
}

fn heard(text: &str, lang: &str) -> Heard {
    Heard {
        session_id: "s".into(),
        text: text.into(),
        language: Some(lang.into()),
    }
}

async fn open_tasks(r: &Rig) -> Vec<tasks::Task> {
    tasks::query(&r.db, &*r.clock, TaskView::All, TaskFilters::default())
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.task)
        .filter(|t| !matches!(t.status, TaskStatus::Completed | TaskStatus::Cancelled))
        .collect()
}

async fn add_task(r: &Rig, title: &str) -> tasks::Task {
    tasks::create(
        &r.db,
        &*r.clock,
        TaskInput {
            title: title.into(),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

fn budget_script() -> Script {
    Script(vec![
        (
            "Kal tak budget report bhejna hai",
            Intent::Task,
            Some("task_create"),
            json!({"title": "Send the budget report", "due_date": "2025-11-06", "due_phrase": "kal tak"}),
        ),
        (
            "usse Friday kar do",
            Intent::Task,
            Some("task_update"),
            json!({"match": "usse", "due_date": "2025-11-07", "due_phrase": "Friday"}),
        ),
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_written_until_the_proposal_is_approved() {
    let r = rig(budget_script(), PROPOSAL_WINDOW);
    let turn =
        r.a.turn(heard("Kal tak budget report bhejna hai", "hi-IN"))
            .await
            .unwrap();
    assert_eq!(turn, Turn::Proposal);
    assert!(open_tasks(&r).await.is_empty(), "a proposal writes nothing");

    let p = r.seen.proposals.lock().unwrap()[0].clone();
    assert_eq!(p.tool, "task_create");
    assert_eq!(p.language, "hi-IN");
    assert_eq!(p.expires_at, START + 30_000);
    assert!(p.summary_text.contains("Send the budget report"));
    let t = r.seen.turns.lock().unwrap()[0].clone();
    assert_eq!(t.reply_text.as_deref(), Some(p.summary_text.as_str()));
    assert_eq!(
        t.proposal.as_ref().map(|x| x.id.clone()),
        Some(p.id.clone())
    );

    let result = r.a.respond(&p.id, ProposalResponse::Approve).await.unwrap();
    assert!(result.ok);
    assert_eq!(result.message, "Ho gaya. Kaam add kar diya.");
    let tasks = open_tasks(&r).await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Send the budget report");
    assert_eq!(tasks[0].due_date, NaiveDate::from_ymd_opt(2025, 11, 6));
    assert_eq!(result.record_id.as_deref(), Some(tasks[0].id.as_str()));

    // The same proposal never runs twice.
    let again =
        r.a.respond(&p.id, ProposalResponse::Approve)
            .await
            .unwrap_err();
    assert_eq!(again.code, VoiceErrorCode::ProposalExpired);
    assert_eq!(open_tasks(&r).await.len(), 1);

    // The turn now carries its result, and the interaction log says executed.
    let last = r.seen.turns.lock().unwrap().last().cloned().unwrap();
    assert!(last.result.unwrap().ok);
    let row = {
        let id = p.interaction_id.clone();
        r.db.read(move |c| voice_store::get_interaction(c, &id))
            .await
            .unwrap()
            .unwrap()
    };
    assert_eq!(row.outcome.as_deref(), Some("executed"));
    assert_eq!(row.transcript, "Kal tak budget report bhejna hai");
}

#[tokio::test(flavor = "multi_thread")]
async fn usse_friday_kar_do_moves_the_previous_task() {
    let r = rig(budget_script(), PROPOSAL_WINDOW);
    add_task(&r, "Book the venue").await;
    r.a.turn(heard("Kal tak budget report bhejna hai", "hi-IN"))
        .await
        .unwrap();
    let p = r.seen.proposals.lock().unwrap()[0].clone();
    r.a.respond(&p.id, ProposalResponse::Approve).await.unwrap();

    assert_eq!(
        r.a.turn(heard("usse Friday kar do", "hi-IN"))
            .await
            .unwrap(),
        Turn::Proposal
    );
    // The decider was told what the previous turn was about.
    // (Other tests decide in parallel, so look for this test's context among all of them.)
    let told = CONTEXTS.lock().unwrap().iter().any(|ctx| {
        ctx.iter()
            .any(|l| l.contains("Send the budget report") && l.contains("task_create"))
    });
    assert!(told);

    let p2 = r.seen.proposals.lock().unwrap()[1].clone();
    assert_eq!(p2.tool, "task_update");
    assert_eq!(p2.args["task_title"], "Send the budget report");
    r.a.respond(&p2.id, ProposalResponse::Approve)
        .await
        .unwrap();
    let tasks = open_tasks(&r).await;
    let budget = tasks
        .iter()
        .find(|t| t.title == "Send the budget report")
        .unwrap();
    assert_eq!(budget.due_date, NaiveDate::from_ymd_opt(2025, 11, 7));
    let venue = tasks.iter().find(|t| t.title == "Book the venue").unwrap();
    assert_eq!(venue.due_date, None, "only the referenced task moves");
}

#[tokio::test(flavor = "multi_thread")]
async fn haan_approves_by_voice_and_nahi_rejects() {
    let r = rig(budget_script(), PROPOSAL_WINDOW);
    r.a.turn(heard("Kal tak budget report bhejna hai", "hi-IN"))
        .await
        .unwrap();
    assert_eq!(r.a.turn(heard("haan", "hi-IN")).await.unwrap(), Turn::Reply);
    assert_eq!(open_tasks(&r).await.len(), 1);
    let t = r.seen.turns.lock().unwrap().last().cloned().unwrap();
    assert_eq!(t.reply_text.as_deref(), Some("Ho gaya. Kaam add kar diya."));
    assert!(r.a.pending().is_none());

    r.a.turn(heard("Kal tak budget report bhejna hai", "hi-IN"))
        .await
        .unwrap();
    r.a.turn(heard("nahi", "hi-IN")).await.unwrap();
    assert_eq!(
        open_tasks(&r).await.len(),
        1,
        "a rejected proposal writes nothing"
    );
    let t = r.seen.turns.lock().unwrap().last().cloned().unwrap();
    assert_eq!(t.reply_text.as_deref(), Some("Theek hai, kuch nahi badla."));

    // "haan" with nothing waiting does nothing.
    r.a.turn(heard("haan", "hi-IN")).await.unwrap();
    assert_eq!(open_tasks(&r).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unanswered_proposal_expires_and_can_no_longer_run() {
    let r = rig(budget_script(), Duration::from_millis(150));
    r.a.turn(heard("Kal tak budget report bhejna hai", "en-IN"))
        .await
        .unwrap();
    let p = r.seen.proposals.lock().unwrap()[0].clone();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(r.a.pending().is_none());
    let err =
        r.a.respond(&p.id, ProposalResponse::Approve)
            .await
            .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::ProposalExpired);
    assert!(open_tasks(&r).await.is_empty());
    let row = {
        let id = p.id.clone();
        r.db.read(move |c| voice_store::get_proposal(c, &id))
            .await
            .unwrap()
            .unwrap()
    };
    assert_eq!(row.status, "expired");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_approvals_at_once_run_the_tool_once() {
    let r = rig(budget_script(), PROPOSAL_WINDOW);
    r.a.turn(heard("Kal tak budget report bhejna hai", "en-IN"))
        .await
        .unwrap();
    let p = r.seen.proposals.lock().unwrap()[0].clone();
    let (a, b) = tokio::join!(
        r.a.respond(&p.id, ProposalResponse::Approve),
        r.a.respond(&p.id, ProposalResponse::Approve)
    );
    assert_eq!([a.is_ok(), b.is_ok()].iter().filter(|ok| **ok).count(), 1);
    assert_eq!(open_tasks(&r).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn several_matches_are_offered_and_doosra_picks_the_second() {
    let r = rig(
        Script(vec![(
            "Call wala kaam ho gaya",
            Intent::Task,
            Some("task_complete"),
            json!({"match": "call"}),
        )]),
        PROPOSAL_WINDOW,
    );
    add_task(&r, "Call Ravi").await;
    add_task(&r, "Call Asha").await;
    add_task(&r, "Write the report").await;
    assert_eq!(
        r.a.turn(heard("Call wala kaam ho gaya", "hi-IN"))
            .await
            .unwrap(),
        Turn::Proposal
    );
    let p = r.seen.proposals.lock().unwrap()[0].clone();
    let offered: Vec<String> = p.args["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(offered.len(), 2);
    assert!(p.summary_text.starts_with("Kaunsa?"), "{}", p.summary_text);

    assert_eq!(
        r.a.turn(heard("doosra", "hi-IN")).await.unwrap(),
        Turn::Reply
    );
    let open: Vec<String> = open_tasks(&r).await.into_iter().map(|t| t.title).collect();
    assert_eq!(open.len(), 2);
    assert!(
        !open.contains(&offered[1]),
        "the second offered task is done"
    );
    assert!(open.contains(&offered[0]));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_cannot_point_the_write_at_a_task_that_was_not_offered() {
    let r = rig(
        Script(vec![(
            "Ravi ko call ho gaya",
            Intent::Task,
            Some("task_complete"),
            json!({"match": "Call Ravi"}),
        )]),
        PROPOSAL_WINDOW,
    );
    let ravi = add_task(&r, "Call Ravi").await;
    let other = add_task(&r, "Pay rent").await;
    r.a.turn(heard("Ravi ko call ho gaya", "en-IN"))
        .await
        .unwrap();
    let p = r.seen.proposals.lock().unwrap()[0].clone();
    // A forged edit swapping the target is ignored: the offered task is the one completed.
    r.a.respond(
        &p.id,
        ProposalResponse::Edit {
            args: json!({"task_id": other.id}),
        },
    )
    .await
    .unwrap();
    let open: Vec<String> = open_tasks(&r).await.into_iter().map(|t| t.id).collect();
    assert_eq!(open, std::slice::from_ref(&other.id));
    assert!(!open.contains(&ravi.id));
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_answer_at_once_with_at_most_five_items() {
    let r = rig(
        Script(vec![(
            "aaj kya karna hai",
            Intent::Task,
            Some("tasks_list"),
            json!({"view": "all"}),
        )]),
        PROPOSAL_WINDOW,
    );
    for i in 1..=7 {
        add_task(&r, &format!("Task {i}")).await;
    }
    assert_eq!(
        r.a.turn(heard("aaj kya karna hai", "en-IN")).await.unwrap(),
        Turn::Reply
    );
    let t = r.seen.turns.lock().unwrap()[0].clone();
    let reply = t.reply_text.unwrap();
    assert!(reply.starts_with("7 open tasks:"), "{reply}");
    assert!(reply.ends_with(", and 2 more."), "{reply}");
    assert!(
        r.seen.proposals.lock().unwrap().is_empty(),
        "reads never propose"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reminder_without_a_time_asks_instead_of_proposing() {
    let r = rig(
        Script(vec![(
            "doctor ka reminder lagao",
            Intent::Reminder,
            Some("reminder_create"),
            json!({"title": "Doctor", "remind_at": null}),
        )]),
        PROPOSAL_WINDOW,
    );
    assert_eq!(
        r.a.turn(heard("doctor ka reminder lagao", "hi-IN"))
            .await
            .unwrap(),
        Turn::Reply
    );
    let t = r.seen.turns.lock().unwrap()[0].clone();
    assert!(t.reply_text.unwrap().starts_with("Kab yaad dilaoon?"));
    assert!(r.seen.proposals.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tool_executor_runs_reads_and_only_proposes_writes() {
    let r = rig(Script(vec![]), PROPOSAL_WINDOW);
    let tools_api = AssistantTools(r.a.clone());
    let err = tools_api
        .run_read("task_create", &json!({"title": "x"}))
        .await
        .unwrap_err();
    assert_eq!(err.code, VoiceErrorCode::ToolFailed);
    assert!(
        tools_api
            .run_read("tasks_list", &json!({"view": "today"}))
            .await
            .unwrap()
            .ok
    );

    let p = tools_api
        .propose(ProposalRequest {
            interaction_id: None,
            tool: "task_create".into(),
            args: json!({"title": "Follow up with Asha"}),
            summary_text: "Add “Follow up with Asha”?".into(),
            language: "en-IN".into(),
        })
        .await
        .unwrap();
    assert!(open_tasks(&r).await.is_empty(), "propose never executes");
    r.a.respond(&p.id, ProposalResponse::Approve).await.unwrap();
    assert_eq!(open_tasks(&r).await[0].title, "Follow up with Asha");
}

#[test]
fn ordinals() {
    for (t, n) in [
        ("pehla", Some(0)),
        ("doosra wala", Some(1)),
        ("the second one", Some(1)),
        ("Teesra.", Some(2)),
        ("2", Some(1)),
        ("pehla kaam add karo aur", None),
        ("haan", None),
    ] {
        assert_eq!(ordinal(t), n, "{t}");
    }
}

#[test]
fn matching_prefers_the_most_shared_words() {
    let items = vec![
        ("1".to_string(), "Send the budget report".to_string()),
        ("2".to_string(), "Report bug".to_string()),
    ];
    assert_eq!(
        best_matches("budget report", &items),
        vec![items[0].clone()]
    );
    assert_eq!(best_matches("report", &items).len(), 2);
    assert!(best_matches("xyz", &items).is_empty());
}

#[test]
fn clock_times_in_spoken_phrases() {
    for (p, t) in [
        ("tomorrow at 6 am", Some((6, 0))),
        ("today at 5 pm", Some((17, 0))),
        ("5:30 PM", Some((17, 30))),
        ("6 a.m.", Some((6, 0))),
        ("at 17:00", Some((17, 0))),
        ("kal 5 baje", Some((17, 0))),
        ("five PM", Some((17, 0))),
        ("12 am", Some((0, 0))),
        ("12 pm", Some((12, 0))),
        ("kal tak", None),
        ("Friday", None),
        ("in 2 days", None),
    ] {
        assert_eq!(clock_time(p), t, "{p}");
    }
    assert_eq!(friendly_date("2026-11-07"), "Sat, 7 Nov");
    assert_eq!(friendly_date("soon"), "soon");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_with_a_time_also_gets_a_reminder_at_that_time() {
    let r = rig(
        Script(vec![(
            "Have a meeting tomorrow at 6 am. Add it to the task.",
            Intent::Task,
            Some("task_create"),
            json!({"title": "meeting", "due_date": "2025-11-06", "due_phrase": "tomorrow at 6 am"}),
        )]),
        PROPOSAL_WINDOW,
    );
    r.a.turn(heard(
        "Have a meeting tomorrow at 6 am. Add it to the task.",
        "en-IN",
    ))
    .await
    .unwrap();
    let p = r.seen.proposals.lock().unwrap()[0].clone();
    assert_eq!(
        p.summary_text,
        "Add “Meeting” for Thu, 6 Nov, with a reminder at Thu, 6 Nov, 6:00 AM?"
    );
    r.a.respond(&p.id, ProposalResponse::Approve).await.unwrap();
    let task = open_tasks(&r).await.into_iter().next().unwrap();
    assert_eq!(task.title, "Meeting");
    let rems = reminders::list(&r.db, false).await.unwrap();
    assert_eq!(rems.len(), 1);
    assert_eq!(rems[0].task_id.as_deref(), Some(task.id.as_str()));
    // 2025-11-06 06:00 IST.
    assert_eq!(rems[0].remind_at, 1_762_389_000_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reminder_finds_its_time_in_the_when_phrase() {
    let r = rig(
        Script(vec![(
            "five PM so add a reminder",
            Intent::Reminder,
            Some("reminder_create"),
            json!({"title": "five PM", "remind_at": null, "when_phrase": "5 pm"}),
        )]),
        PROPOSAL_WINDOW,
    );
    assert_eq!(
        r.a.turn(heard("five PM so add a reminder", "en-IN"))
            .await
            .unwrap(),
        Turn::Proposal
    );
    let p = r.seen.proposals.lock().unwrap()[0].clone();
    // START is 13:30 IST on 5 Nov, so 5 pm is later today (11:30 UTC).
    assert_eq!(p.args["remind_at"], 1_762_342_200_000i64);
    assert_eq!(p.args["title"], "Reminder");
    assert_eq!(p.summary_text, "Remind you to “Reminder” at 5:00 PM?");
}
