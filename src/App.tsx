import { useEffect, useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Plug } from "lucide-react";
import { useRoutineReminder } from "@/features/routine/useRoutine";
import { useDailyGreeting } from "@/features/companion/useCompanion";
import { useWellbeingNudges } from "@/features/wellbeing/useWellbeing";
import { useRepeatReminder } from "@/features/tasks/useRepeatReminder";
import { Notices } from "@/components/Notices";
import { useAutoDelete } from "@/features/settings/autoDelete";
import { setPetShown } from "@/lib/notify";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useBin, useBusSync, useLabels, useNoteActions, useNotes, useSettings } from "@/hooks/usePluto";
import { useAgenda } from "@/features/overview/TodayAgenda";
import { useTheme } from "@/hooks/useTheme";
import { ipc, onCommand, type NoteSort } from "@/ipc";
import { BinPage } from "@/features/bin/BinPage";
import { CharacterPage } from "@/features/character/CharacterPage";
import { NoteEditor } from "@/features/notes/NoteEditor";
import { NotesPage } from "@/features/notes/NotesPage";
import type { NotesLayout } from "@/features/notes/NotesBoard";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { ComingSoon } from "@/features/shell/ComingSoon";
import { Sidebar, type Page } from "@/features/shell/Sidebar";
import { TasksPage } from "@/features/tasks/TasksPage";
import { TimePage } from "@/features/time/TimePage";
import { TodayOverviewPage } from "@/features/overview/TodayOverviewPage";
import { MeetingsPage } from "@/features/meetings/MeetingsPage";
import { ProjectsPage } from "@/features/projects/ProjectsPage";
import { DiaryPage } from "@/features/diary/DiaryPage";
import { StudyPage } from "@/features/student/StudyPage";
import type { TaskFocus } from "@/features/overview/YesterdayLine";
import { Welcome, useFirstRun, type WelcomeNext } from "@/features/welcome/Welcome";
import { Tour } from "@/features/guide/Tour";
import { ROLE_PREF, parseRole } from "@/features/guide/roles";
import { TOUR_PREF, parseTour, stepsFor } from "@/features/guide/tourSteps";
import { usePref } from "@/features/guide/usePref";

const PAGES: readonly Page[] = ["today", "tasks", "time", "notes", "projects", "study", "diary", "meetings", "archive", "bin", "character", "mcp", "settings"];

function Screen() {
  const [page, setPageState] = useState<Page>("today");
  const [labelId, setLabelId] = useState<string | null>(null);
  const [sort, setSort] = useState<NoteSort>("last_edited");
  const [layout, setLayoutState] = useState<NotesLayout>("grid");
  const [editing, setEditing] = useState<string | null>(null);
  const [newMeeting, setNewMeeting] = useState(0);
  const [taskFocus, setTaskFocus] = useState<TaskFocus | null>(null);

  useBusSync();
  const theme = useTheme();
  const firstRun = useFirstRun();
  const tour = usePref(TOUR_PREF, parseTour);
  const role = usePref(ROLE_PREF, parseRole);
  // FR-85: "You have N daily routine tasks pending", once a day at the chosen time.
  useRoutineReminder();
  // FR-93/94: hello once a day, and birthdays.
  useDailyGreeting();
  // FR-109: in the evening, repeating tasks still open today.
  useRepeatReminder();
  useWellbeingNudges();
  // FR-46: auto-delete by the user's rules (never by default).
  useAutoDelete();
  // Messages go above the pet; while the pet is hidden they show here instead.
  const { data: settings } = useSettings();
  const petShown = settings?.["pet.visible"] ?? true;
  useEffect(() => setPetShown(petShown), [petShown]);
  const actions = useNoteActions();
  const { data: allNotes = [] } = useNotes(false, null, "last_edited");
  const { data: labels = [] } = useLabels();
  const agenda = useAgenda();
  const { data: bin = [] } = useBin();

  // Restore where the person left off (ui.last_view, ui.notes_layout).
  useEffect(() => {
    void ipc.prefsGet<Page>("ui.last_view").then((p) => p && PAGES.includes(p) && setPageState(p));
    void ipc.prefsGet<NotesLayout>("ui.notes_layout").then((l) => (l === "grid" || l === "list") && setLayoutState(l));
  }, []);
  const setPage = (p: Page) => {
    setPageState(p);
    setTaskFocus(null);
    void ipc.prefsSet("ui.last_view", p);
  };
  const setLayout = (l: NotesLayout) => {
    setLayoutState(l);
    void ipc.prefsSet("ui.notes_layout", l);
  };

  // App-wide shortcuts (UI brief §8): Mod+1…5 jump between pages, Mod+, opens Settings,
  // Mod+T starts a new task, Mod+M a new meeting. New note and search live on the Notes page.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.shiftKey || e.altKey) return;
      const pages: Record<string, Page> = { "1": "today", "2": "tasks", "3": "notes", "4": "time", "5": "meetings", ",": "settings", t: "tasks", m: "meetings" };
      const target = pages[e.key.toLowerCase()];
      if (!target) return;
      e.preventDefault();
      setPage(target);
      if (e.key.toLowerCase() === "t") setTimeout(() => document.getElementById("new-task")?.focus(), 50);
      if (e.key.toLowerCase() === "m") setNewMeeting((n) => n + 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // A due reminder shows above the pet (with Done) and as a system notification; nothing pops up
  // inside the dashboard (owner 2026-10-23).

  // From the shell: global shortcuts and the pet's menu.
  useEffect(() => onCommand((command) => {
    if (command === "new-note") void newNote();
    else if (command === "new-task") {
      setPage("tasks");
      setTimeout(() => document.getElementById("new-task")?.focus(), 80);
    } else if (command === "focus") {
      setPage("today");
      setTimeout(() => document.getElementById("focus-tile")?.scrollIntoView?.({ block: "center" }), 120);
    } else if (command === "tasks") setPage("tasks");
    else if (command === "settings") setPage("settings");
    // The pet's right-click menu: "page:<name>" opens that page (only real pages).
    else if (command.startsWith("page:")) {
      const target = command.slice("page:".length) as Page;
      if (PAGES.includes(target)) setPage(target);
    }
  }));

  const newNote = async () => {
    const note = await actions.create({ label_ids: labelId ? [labelId] : [] });
    if (page !== "notes") setPage("notes");
    setEditing(note.id);
  };
  const labelName = labelId ? labels.find((l) => l.label.id === labelId)?.label.name : undefined;

  if (firstRun.show) {
    const start = async (next: WelcomeNext) => {
      await firstRun.finish();
      if (next === "note") return void newNote();
      if (next === "meeting") {
        setPage("meetings");
        setNewMeeting((n) => n + 1);
        return;
      }
      if (next === "task") {
        setPage("tasks");
        setTimeout(() => document.getElementById("new-task")?.focus(), 80);
        return;
      }
      setPage("today");
    };
    return (
      <>
        <Welcome onDone={(next) => void start(next)} />
      </>
    );
  }

  return (
    <div className="flex h-screen min-h-[600px] bg-background text-foreground">
      <Sidebar
        page={page}
        counts={{ today: agenda.open, notes: allNotes.length, bin: bin.length }}
        onPage={setPage} isDark={theme.isDark} onToggleTheme={() => void theme.toggle()}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <main className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[1280px] px-4 py-6 sm:px-6">
            {(page === "notes" || page === "archive") && (
              <NotesPage
                archived={page === "archive"} labelId={page === "notes" ? labelId : null} {...(labelName ? { labelName } : {})}
                labels={labels} onLabel={setLabelId}
                sort={sort} layout={layout} onSort={setSort} onLayout={setLayout}
                onOpen={setEditing} onNew={() => void newNote()} onNavigate={setPage}
              />
            )}
            {page === "today" && (
              <TodayOverviewPage onShowTasks={(focus) => { setPage("tasks"); setTaskFocus(focus); }} onNavigate={setPage} />
            )}
            {page === "bin" && <BinPage />}
            {page === "character" && <CharacterPage />}
            {page === "settings" && <SettingsPage />}
            {page === "tasks" && <TasksPage focus={taskFocus} onClearFocus={() => setTaskFocus(null)} />}
            {page === "meetings" && <MeetingsPage newTick={newMeeting} />}
            {page === "projects" && <ProjectsPage />}
            {page === "study" && <StudyPage />}
            {page === "diary" && <DiaryPage />}
            {page === "time" && <TimePage />}
            {page === "mcp" && <ComingSoon icon={<Plug />} title="MCP" description="Connect external tools and services to Pluto through the Model Context Protocol. This arrives in a later release." />}
          </div>
        </main>
      </div>
      <NoteEditor noteId={editing} onClose={() => setEditing(null)} />
      {tour.value === "pending" && role.value && (
        <Tour steps={stepsFor(role.value)} onNavigate={setPage} onClose={() => void tour.set("done")} />
      )}
      <Notices />
    </div>
  );
}

export function App() {
  const [client] = useState(() => new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: false } } }));
  return (
    <QueryClientProvider client={client}>
      <TooltipProvider delayDuration={300}>
        <Screen />
      </TooltipProvider>
    </QueryClientProvider>
  );
}
