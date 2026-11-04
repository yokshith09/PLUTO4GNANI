# AS-4 — 50 commands (synthetic)

Reference time: Wednesday 4 Nov 2026, 09:00 IST. Expected tool and arguments are in `test-data/ground-truth/as4.json`.

| # | Language | Say | Expected |
|---|---|---|---|
| c01 | hi-en | Aaj ke tasks dikhao | tasks_list |
| c02 | en | What are my tasks for today | tasks_list |
| c03 | ta-en | Inniku enna tasks irukku | tasks_list |
| c04 | kn-en | Indina tasks yenu ide | tasks_list |
| c05 | en | Show my overdue tasks | tasks_list |
| c06 | hi-en | Mere reminders dikhao | reminders_list |
| c07 | en | List my reminders | reminders_list |
| c08 | hi-en | Kal tak budget report bhejna hai, task add karo | task_create |
| c09 | hi-en | Friday ko client call ke liye task banao | task_create |
| c10 | en | Add a task to review the pull request by tomorrow | task_create |
| c11 | ta-en | Naalaiku invoice anuppanum, task add pannu | task_create |
| c12 | kn-en | Naale report submit maadbeku, task haaku | task_create |
| c13 | en | Create a task to book flights next Monday | task_create |
| c14 | hi-en | Budget report wala task complete kar do | task_complete |
| c15 | en | Mark the pull request review as done | task_complete |
| c16 | ta-en | Invoice task mudinjiduchu, complete pannu | task_complete |
| c17 | kn-en | Report task mugiyitu, complete maadi | task_complete |
| c18 | hi-en | Client call ko Monday tak aage badhao | task_update |
| c19 | en | Move the flights task to Friday | task_update |
| c20 | ta-en | Invoice task-a Thursday-ku maathu | task_update |
| c21 | hi-en | Kal subah nau baje yaad dilana, dentist ko call karna hai | reminder_create |
| c22 | en | Remind me at 5 pm today to pick up the parcel | reminder_create |
| c23 | ta-en | Naalaiku kaalai 8 mani-ku remind pannu, gym povanum | reminder_create |
| c24 | kn-en | Naale bilige hattu ghantege nenapisu, bank hogbeku | reminder_create |
| c25 | en | Set a reminder in two hours to drink water | reminder_create |
| c26 | hi-en | Dentist wala reminder done mark karo | reminder_done |
| c27 | hi-en | Pichli meeting mein launch date kya decide hui thi | meeting_search |
| c28 | en | What did we decide about the annual plan | meeting_search |
| c29 | ta-en | Last meeting-la release date enna decide pannom | meeting_search |
| c30 | kn-en | Hindina meeting nalli office shift yaavaga anta nirdharisidvi | meeting_search |
| c31 | hi-en | Ravi ne kya commit kiya tha | commitments_list |
| c32 | en | What is Meena supposed to deliver | commitments_list |
| c33 | en | Show all overdue commitments | commitments_list |
| c34 | hi-en | Notes mein pricing search karo | notes_search |
| c35 | en | Find my notes about the offsite | notes_search |
| c36 | hi-en | Ek note banao: vendor ko quote ke liye call karna | note_create |
| c37 | en | Take a note: ideas for the demo video | note_create |
| c38 | ta-en | Oru note ezhudhu: sprint retro points | note_create |
| c39 | hi-en | Pachchis minute ka focus shuru karo | focus_start |
| c40 | en | Start a 45 minute focus session | focus_start |
| c41 | kn-en | Hattu nimisha focus shuru maadi | focus_start |
| c42 | en | Stop the focus session | focus_stop |
| c43 | hi-en | Focus band karo | focus_stop |
| c44 | en | Show my projects | projects_list |
| c45 | hi-en | Haan kar do | approval |
| c46 | en | Yes go ahead | approval |
| c47 | ta-en | Aama pannunga | approval |
| c48 | hi-en | Nahi rehne do | reject |
| c49 | en | No cancel that | cancel |
| c50 | en | Hmm okay never mind the weather is nice | noise |
