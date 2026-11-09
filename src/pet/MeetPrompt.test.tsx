import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { ipc } from "@/ipc";
import { elapsedText } from "./MeetPrompt";
import { PetApp } from "./PetApp";

beforeAll(() => {
  window.PointerEvent ??= class extends MouseEvent {} as never;
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as never;
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const renderPet = () =>
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><PetApp /></QueryClientProvider>);
const send = (name: string, detail: unknown) => act(() => { window.dispatchEvent(new CustomEvent(name, { detail })); });

describe("Approval questions on the pet", () => {
  it("a voice change waiting for approval can be approved from the pet and goes to the assistant", async () => {
    const proposal = vi.spyOn(ipc, "petProposalAnswer").mockResolvedValue(null);
    const meet = vi.spyOn(ipc, "autoMeetingAnswer").mockResolvedValue(null);
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    send("pluto-pet-ask", { id: 99, title: "Approve this change?", detail: "Move “Send report to Akhil” to Wed, 11 Nov?", options: ["Approve", "Reject"], kind: "proposal" });
    const card = await screen.findByRole("alertdialog", { name: "Approve this change?" });
    fireEvent.click(within(card).getByRole("button", { name: "Approve" }));
    expect(proposal).toHaveBeenCalledWith(99, "Approve");
    expect(meet).not.toHaveBeenCalled();
  });
});

describe("Google Meet auto mode on the pet", () => {
  it("shows the question with its buttons and sends the answer back", async () => {
    const answer = vi.spyOn(ipc, "autoMeetingAnswer").mockResolvedValue(null);
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    send("pluto-pet-ask", { id: 7, title: "Google Meet is open. Record this meeting?", detail: "Meeting abc-defg-hij. Everyone in it should know it is being recorded.", options: ["Record", "Not now"] });
    const card = await screen.findByRole("alertdialog", { name: /Record this meeting/ });
    expect(card.textContent).toContain("Everyone in it should know");
    fireEvent.click(within(card).getByRole("button", { name: "Record" }));
    expect(answer).toHaveBeenCalledWith(7, "Record");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("takes the question down when the shell withdraws it, and offers to turn Cloud voice on when it is off", async () => {
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    send("pluto-pet-ask", { id: 8, title: "Google Meet is open. Record this meeting?", detail: "Cloud voice is off. Turn it on to record? The audio is sent to Gnani to be transcribed.", options: ["Turn on and record", "Not now"] });
    const card = await screen.findByRole("alertdialog");
    expect(within(card).getByRole("button", { name: "Turn on and record" })).toBeTruthy();
    send("pluto-pet-ask-done", 8);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("shows that it is recording, with the time, and Stop ends it", async () => {
    const stop = vi.spyOn(ipc, "petRecordingStop").mockResolvedValue(null);
    renderPet();
    await screen.findByRole("button", { name: /^Pluto\./ });
    send("pluto-pet-recording", { session_id: "s", meeting_id: "m", started_at: Date.now() - 125_000 });
    const chip = await screen.findByRole("status", { name: /^Recording 2:0\d$/ });
    fireEvent.click(within(chip).getByRole("button", { name: "Stop recording" }));
    expect(stop).toHaveBeenCalled();
    send("pluto-pet-recording", null);
    await waitFor(() => expect(screen.queryByRole("status", { name: /^Recording/ })).toBeNull());
  });

  it("hovering the pet shows the recording inside the card, with Stop, instead of hiding it", async () => {
    const stop = vi.spyOn(ipc, "petRecordingStop").mockResolvedValue(null);
    const { PetCard } = await import("./PetCard");
    render(<QueryClientProvider client={new QueryClient()}><PetCard recording={{ elapsed: "2:05", onStop: () => void ipc.petRecordingStop() }} /></QueryClientProvider>);
    const row = await screen.findByRole("group", { name: "Recording controls" });
    expect(row.textContent).toContain("Recording 2:05");
    fireEvent.click(within(row).getByRole("button", { name: "Stop recording" }));
    expect(stop).toHaveBeenCalled();
  });

  it("the recording time reads like a clock", () => {
    expect(elapsedText(0)).toBe("0:00");
    expect(elapsedText(65_000)).toBe("1:05");
    expect(elapsedText(3_725_000)).toBe("1:02:05");
    expect(elapsedText(-5)).toBe("0:00");
  });
});
