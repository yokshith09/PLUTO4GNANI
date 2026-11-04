//! The synthetic test sets (T-008) are consistent: every ground-truth quote is in its script,
//! owners are participants, dates are real, and AS-4 covers only known tools.

use std::path::{Path, PathBuf};

use serde_json::Value;

const TOOLS: [&str; 14] = [
    "tasks_list",
    "task_create",
    "task_complete",
    "task_update",
    "reminders_list",
    "reminder_create",
    "reminder_done",
    "meeting_search",
    "commitments_list",
    "notes_search",
    "note_create",
    "focus_start",
    "focus_stop",
    "projects_list",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data")
}

fn json(file: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(root().join("ground-truth").join(file)).unwrap())
        .unwrap()
}

fn script(prefix: &str) -> String {
    let dir = root().join("audio-scripts");
    let path = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix))
        .unwrap_or_else(|| panic!("no script for {prefix}"));
    std::fs::read_to_string(path).unwrap().replace("**", "")
}

fn valid_date(d: &str) -> bool {
    let p: Vec<_> = d.split('-').collect();
    p.len() == 3 && p[0].len() == 4 && p[1].len() == 2 && p[2].len() == 2 && {
        let (y, m, day): (u32, u32, u32) = (
            p[0].parse().unwrap(),
            p[1].parse().unwrap(),
            p[2].parse().unwrap(),
        );
        y == 2026 && (1..=12).contains(&m) && (1..=31).contains(&day)
    }
}

#[test]
fn meeting_ground_truth_matches_its_script() {
    for (file, prefix) in [
        ("as1.json", "as1"),
        ("as2.json", "as2"),
        ("as3.json", "as3"),
    ] {
        let gt = json(file);
        let text = script(prefix);
        let people: Vec<&str> = gt["participants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        let mut actions = 0;
        for group in ["decisions", "action_items", "questions", "risks"] {
            for item in gt[group].as_array().unwrap() {
                let quote = item["quote"].as_str().unwrap();
                assert!(text.contains(quote), "{file}: quote not in script: {quote}");
                if group == "action_items" {
                    actions += 1;
                    assert!(
                        people.contains(&item["owner"].as_str().unwrap()),
                        "{file}: owner not a participant"
                    );
                    assert!(
                        valid_date(item["due_date"].as_str().unwrap()),
                        "{file}: bad date"
                    );
                }
            }
        }
        assert!(actions >= 2, "{file}: needs action items");
    }
    assert_eq!(
        json("as1.json")["action_items"].as_array().unwrap().len(),
        5
    );
    assert_eq!(json("as1.json")["decisions"].as_array().unwrap().len(), 4);
}

#[test]
fn as4_has_fifty_commands_over_known_tools() {
    let gt = json("as4.json");
    let cmds = gt["commands"].as_array().unwrap();
    assert_eq!(cmds.len(), 50);
    let mut seen = std::collections::BTreeSet::new();
    for c in cmds {
        if let Some(tool) = c["tool"].as_str() {
            assert!(TOOLS.contains(&tool), "unknown tool {tool}");
            seen.insert(tool.to_owned());
        } else {
            assert!(
                ["approval", "reject", "cancel", "noise"].contains(&c["intent"].as_str().unwrap())
            );
        }
        for key in ["due_date"] {
            if let Some(d) = c["args"][key].as_str() {
                assert!(valid_date(d), "bad date {d}");
            }
        }
    }
    assert_eq!(seen.len(), TOOLS.len(), "every tool is covered");
    let langs: std::collections::BTreeSet<_> = cmds
        .iter()
        .map(|c| c["language"].as_str().unwrap())
        .collect();
    assert_eq!(langs.len(), 4, "hi-en, ta-en, kn-en and en");
}
