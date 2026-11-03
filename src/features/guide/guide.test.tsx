import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { BANK, QUIZ_ORDER, parseLearn, questionsFor } from "./learn";
import { LearnCard } from "./LearnCard";
import { ROLES, parseRole, parseStudentPack, studentPackOn } from "./roles";
import { Tour } from "./Tour";
import { STEPS, TOUR_ORDER, stepsFor } from "./tourSteps";
import { YourProfile } from "./YourProfile";
import { GuideSection } from "./GuideSection";
import { CREDIT, GUIDE } from "./content";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("what each role is shown (data)", () => {
  it("an unknown or missing role is General", () => {
    expect(parseRole("student")).toBe("student");
    expect(parseRole("wizard")).toBe("general");
    expect(parseRole(null)).toBe("general");
  });
  it("every role's tour is made of real steps, starts on Today, and covers privacy and the way to replay it", () => {
    for (const r of ROLES) {
      const ids = TOUR_ORDER[r];
      expect(ids[0]).toBe("today");
      expect(ids).toContain("privacy");
      expect(ids[ids.length - 1]).toBe("settings");
      expect(new Set(ids).size).toBe(ids.length);
      for (const id of ids) expect(STEPS[id]).toBeTruthy();
    }
  });
  it("every role gets five questions, with the privacy ones in every set and a valid right answer", () => {
    for (const r of ROLES) {
      const qs = questionsFor(r);
      expect(qs).toHaveLength(5);
      expect(QUIZ_ORDER[r]).toContain("privacy-default");
      expect(QUIZ_ORDER[r]).toContain("privacy-radar");
      for (const q of qs) {
        expect([0, 1, 2]).toContain(q.answer);
        expect(q.options).toHaveLength(3);
        expect(q.why.length).toBeGreaterThan(20);
      }
    }
    // The right answer is not always the first button.
    expect(new Set(Object.values(BANK).map((q) => q.answer)).size).toBeGreaterThan(1);
  });
  it("only the student pack differs by role, and a saved choice overrides the role", () => {
    expect(ROLES.filter((r) => studentPackOn(r, null))).toEqual(["student", "student_dev"]);
    expect(studentPackOn("general", true)).toBe(true);
    expect(studentPackOn("student", false)).toBe(false);
    expect(parseStudentPack("yes")).toBeNull();
  });
  it("saved quiz answers drop anything unknown", () => {
    expect(parseLearn({ answers: { focus: 1, nope: 2, pet: 7 }, dismissed: true })).toEqual({ answers: { focus: 1 }, dismissed: true });
    expect(parseLearn("junk")).toEqual({ answers: {}, dismissed: false });
  });
});

function wrap(ui: React.ReactElement) {
  return <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{ui}</QueryClientProvider>;
}

describe("the spotlight tour", () => {
  beforeEach(() => {
    document.body.innerHTML = '<button data-tour="nav-today">Today</button><button data-tour="nav-tasks">Tasks</button>';
  });
  const steps = [STEPS.today, STEPS.tasks, STEPS.pet];

  it("goes Next and Back, opens each step's page, and Done finishes it", () => {
    const onNavigate = vi.fn();
    const onClose = vi.fn();
    render(<Tour steps={steps} onNavigate={onNavigate} onClose={onClose} />);
    expect(screen.getByRole("dialog", { name: "Pluto tour" })).toBeTruthy();
    expect(screen.getByText("Step 1 of 3")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.getByText("Step 2 of 3")).toBeTruthy();
    expect(onNavigate).toHaveBeenLastCalledWith("tasks");
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByText("Step 1 of 3")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(onClose).toHaveBeenCalledWith(true);
  });
  it("Skip and Escape end it without finishing, and Next has the focus", () => {
    const onClose = vi.fn();
    render(<Tour steps={steps} onNavigate={() => undefined} onClose={onClose} />);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Next" }));
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenLastCalledWith(false);
    fireEvent.click(screen.getByRole("button", { name: "Skip tour" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
  it("keeps Tab inside the card", () => {
    render(<Tour steps={steps} onNavigate={() => undefined} onClose={() => undefined} />);
    const next = screen.getByRole("button", { name: "Next" });
    next.focus();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Skip tour" }));
  });
  it("a step whose target is missing still shows its card", () => {
    document.body.innerHTML = "";
    render(<Tour steps={[STEPS.privacy]} onNavigate={() => undefined} onClose={() => undefined} />);
    expect(screen.getByText(/Privacy radar/)).toBeTruthy();
  });
  it("the real steps are in the order a student or a developer expects", () => {
    expect(stepsFor("developer").map((s) => s.id).slice(0, 3)).toEqual(["today", "projects", "tasks"]);
    expect(stepsFor("student").map((s) => s.id)).not.toContain("projects");
  });
});

describe("the Learn Pluto card", () => {
  function prefs(initial: Record<string, unknown>) {
    const store = { ...initial };
    vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (store[k] ?? null) as never);
    vi.spyOn(ipc, "prefsSet").mockImplementation(async (k: string, v: unknown) => { store[k] = v; return null as never; });
    return store;
  }

  it("asks the role's first question, explains the answer, and the 'show me' button goes there", async () => {
    const store = prefs({ "ui.role": "general", "ui.tour": "done" });
    const onShow = vi.fn();
    render(wrap(<LearnCard onShow={onShow} />));
    expect(await screen.findByText("What does one click on the pet do?")).toBeTruthy();
    expect(screen.getByText("Question 1 of 5")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Hides the pet" }));
    expect(screen.getByRole("status").textContent).toContain("Not quite.");
    fireEvent.click(screen.getByRole("button", { name: "Open Character" }));
    expect(onShow).toHaveBeenCalledWith("character");
    fireEvent.click(screen.getByRole("button", { name: "Next question" }));
    await waitFor(() => expect(store["ui.learn"]).toEqual({ answers: { pet: 1 }, dismissed: false }));
    expect(await screen.findByText("Question 2 of 5")).toBeTruthy();
  });

  it("teaches that tracking is off until you switch it on", async () => {
    prefs({ "ui.role": "student", "ui.tour": "done", "ui.learn": { answers: { "quick-add": 0, focus: 1 }, dismissed: false } });
    render(wrap(<LearnCard onShow={() => undefined} />));
    expect(await screen.findByText(/first time you open Pluto, what does it track/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Nothing: tracking is off/ }));
    expect(screen.getByRole("status").textContent).toContain("Yes.");
    expect(screen.getByRole("status").textContent).toContain("off until you switch them on");
  });

  it("waits while the tour is on, can be hidden, and ends with a summary", async () => {
    prefs({ "ui.role": "student", "ui.tour": "pending" });
    const { container } = render(wrap(<LearnCard onShow={() => undefined} />));
    await new Promise((r) => setTimeout(r, 30));
    expect(container.textContent).toBe("");
    cleanup();

    const store = prefs({ "ui.role": "student", "ui.tour": "done", "ui.learn": { answers: Object.fromEntries(QUIZ_ORDER.student.map((id) => [id, BANK[id]?.answer])), dismissed: false } });
    render(wrap(<LearnCard onShow={() => undefined} />));
    const card = await screen.findByRole("region", { name: "Learn Pluto" });
    expect(within(card).getByText(/You answered 5 of 5/)).toBeTruthy();
    fireEvent.click(within(card).getByRole("button", { name: "Hide Learn Pluto" }));
    await waitFor(() => expect((store["ui.learn"] as { dismissed: boolean }).dismissed).toBe(true));
    expect(screen.queryByRole("region", { name: "Learn Pluto" })).toBeNull();
  });
});

describe("Settings → Your profile", () => {
  it("changes the role, switches the student pack, and replays the tour and the quiz", async () => {
    const store: Record<string, unknown> = { "ui.role": "general", "ui.tour": "done", "ui.learn": { answers: { pet: 0 }, dismissed: true } };
    vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (store[k] ?? null) as never);
    vi.spyOn(ipc, "prefsSet").mockImplementation(async (k: string, v: unknown) => { store[k] = v; return null as never; });
    render(wrap(<YourProfile />));
    const group = await screen.findByRole("radiogroup", { name: "What describes you best" });
    expect(within(group).getByRole("radio", { name: /Something else/ }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(within(group).getByRole("radio", { name: /^Developer/ }));
    await waitFor(() => expect(store["ui.role"]).toBe("developer"));
    const pack = screen.getByRole("checkbox", { name: /Student pack/ }) as HTMLInputElement;
    expect(pack.checked).toBe(false);
    fireEvent.click(pack);
    await waitFor(() => expect(store["ui.studentPack"]).toBe(true));
    fireEvent.click(within(group).getByRole("radio", { name: /^Student Tasks/ }));
    await waitFor(() => expect(store["ui.role"]).toBe("student"));
    fireEvent.click(screen.getByRole("button", { name: "Replay the tour" }));
    await waitFor(() => expect(store["ui.tour"]).toBe("pending"));
    fireEvent.click(screen.getByRole("button", { name: "Restart the quiz" }));
    await waitFor(() => expect(store["ui.learn"]).toEqual({ answers: {}, dismissed: false }));
  });
});

describe("Settings → Guide", () => {
  it("has a short answer per feature, covers tracking and privacy, and shows the student entries only with the pack", async () => {
    expect(new Set(GUIDE.map((g) => g.id)).size).toBe(GUIDE.length);
    expect(GUIDE.map((g) => g.id)).toEqual(expect.arrayContaining(["tracking", "privacy", "data"]));
    expect(CREDIT).toBe("Built by QUORBIT TECHNOLOGIES");
    vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (k === "ui.role" ? "general" : null) as never);
    render(wrap(<GuideSection />));
    expect(await screen.findByText("Add a task")).toBeTruthy();
    expect(screen.getByText("App and website time")).toBeTruthy();
    expect(screen.queryByText("Courses and exams")).toBeNull();
    cleanup();
    vi.spyOn(ipc, "prefsGet").mockImplementation(async (k: string) => (k === "ui.role" ? "student" : null) as never);
    render(wrap(<GuideSection />));
    expect(await screen.findByText("Courses and exams")).toBeTruthy();
    expect(screen.getByText("Weekly report")).toBeTruthy();
  });
});
