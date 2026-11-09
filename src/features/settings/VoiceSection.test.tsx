import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { DISCLOSURE, MAX_LANGUAGES, VoiceSection } from "./VoiceSection";

afterEach(cleanup);
const wrap = () => render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><VoiceSection /></QueryClientProvider>);

// Against the in-memory shell (the voice mock keeps one settings copy per test run, so each test
// uses its own controls and leaves the others alone).
describe("Voice and privacy settings (T-401)", () => {
  it("cloud voice is off, and turning it on shows what is sent and needs a confirmation", async () => {
    const user = userEvent.setup();
    wrap();
    const toggle = await screen.findByRole("switch", { name: "Use cloud voice" });
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText(DISCLOSURE)).toBeTruthy();

    await user.click(toggle);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Turn on cloud voice\?/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Not now" }));
    expect(screen.getByRole("switch", { name: "Use cloud voice" }).getAttribute("aria-checked")).toBe("false");

    await user.click(screen.getByRole("switch", { name: "Use cloud voice" }));
    await user.click(await screen.findByRole("button", { name: "Turn on" }));
    await waitFor(() => expect(screen.getByRole("switch", { name: "Use cloud voice" }).getAttribute("aria-checked")).toBe("true"));

    await user.click(screen.getByRole("switch", { name: "Use cloud voice" }));
    await waitFor(() => expect(screen.getByRole("switch", { name: "Use cloud voice" }).getAttribute("aria-checked")).toBe("false"));
  });

  it("a saved key is masked, then shown only as Saved, and can be removed", async () => {
    const user = userEvent.setup();
    wrap();
    const input = await screen.findByLabelText("Groq key");
    expect(input.getAttribute("type")).toBe("password");
    await user.type(input, "gsk_secret_value");
    await user.click(screen.getAllByRole("button", { name: "Save" })[2] as HTMLElement);
    await screen.findByText(/Saved on this computer/);
    expect(screen.queryByDisplayValue("gsk_secret_value")).toBeNull();
    expect(document.body.textContent).not.toContain("gsk_secret_value");
    await user.click(screen.getByRole("button", { name: "Remove" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByText(/Saved on this computer/)).toBeNull());
  });

  it("allows at most four languages", async () => {
    const user = userEvent.setup();
    wrap();
    const list = await screen.findByRole("list", { name: "Languages" });
    for (const name of ["Hindi", "English", "Tamil", "Kannada"]) await user.click(within(list).getByRole("checkbox", { name }));
    await waitFor(() => expect(within(list).getAllByRole("checkbox").filter((c) => (c as HTMLInputElement).checked)).toHaveLength(MAX_LANGUAGES));
    expect((within(list).getByRole("checkbox", { name: "Telugu" }) as HTMLInputElement).disabled).toBe(true);
    await user.click(within(list).getByRole("checkbox", { name: "Hindi" }));
    await waitFor(() => expect((within(list).getByRole("checkbox", { name: "Telugu" }) as HTMLInputElement).disabled).toBe(false));
  });

  it("lists what was sent to the cloud, and clears the list", async () => {
    const user = userEvent.setup();
    wrap();
    const list = await screen.findByRole("list", { name: "Sent to the cloud" });
    expect(within(list).getAllByRole("listitem").length).toBeGreaterThan(0);
    expect(within(list).getByText(/Gnani \(meeting audio\)/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Clear" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Clear" }));
    expect(await screen.findByText("Nothing has been sent yet.")).toBeTruthy();
  });
});

describe("Voice and hotkey settings (T-401)", () => {
  it("the voice can be changed and previewed", async () => {
    const user = userEvent.setup();
    wrap();
    const select = (await screen.findByLabelText("Voice")) as HTMLSelectElement;
    await user.selectOptions(select, "Avinash");
    await waitFor(() => expect((screen.getByLabelText("Voice") as HTMLSelectElement).value).toBe("Avinash"));
    expect(screen.getByRole("button", { name: "Preview" })).toBeTruthy();
  });

  it("the push-to-talk key is changed by pressing the new keys, and can be reset", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: "Change push-to-talk key" }));
    expect(screen.getByRole("status").textContent).toMatch(/Press the new keys/);
    await user.keyboard("{Control>}{Alt>}k{/Alt}{/Control}");
    await waitFor(() => expect(screen.getByText("Ctrl+Alt+K")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: "Reset push-to-talk key" }));
    await waitFor(() => expect(screen.getByText("Ctrl+Shift+Space")).toBeTruthy());
  });
});

describe("Hands-free and dictation keys", () => {
  it("each says what it does, and each can be changed", async () => {
    const user = userEvent.setup();
    wrap();
    expect(await screen.findByText(/Press once, say your command, stop talking/)).toBeTruthy();
    expect(screen.getByText(/typing what you say wherever your cursor is/)).toBeTruthy();
    expect(screen.getByText("Ctrl+Alt+Space")).toBeTruthy();
    expect(screen.getByText("Ctrl+Alt+Shift+Space")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Change shortcut: Dictate" }));
    await user.keyboard("{Control>}{Alt>}j{/Alt}{/Control}");
    await waitFor(() => expect(screen.getByText("Ctrl+Alt+J")).toBeTruthy());
    await user.click(screen.getByRole("button", { name: "Reset shortcut: Dictate" }));
    await waitFor(() => expect(screen.getByText("Ctrl+Alt+Shift+Space")).toBeTruthy());
  });

  it("refuses a key another Pluto shortcut already uses", async () => {
    const user = userEvent.setup();
    wrap();
    await user.click(await screen.findByRole("button", { name: "Change shortcut: Dictate" }));
    // Ctrl+Alt+Space belongs to hands-free.
    await user.keyboard("{Control>}{Alt>}[Space]{/Alt}{/Control}");
    await waitFor(() => expect(screen.queryByText(/Press the new keys/)).toBeNull());
    expect(screen.getByText("Ctrl+Alt+Shift+Space")).toBeTruthy();
  });
});

describe("Offer to record Google Meet (auto mode)", () => {
  it("is off by default and can be switched on and off", async () => {
    const user = userEvent.setup();
    wrap();
    const toggle = await screen.findByRole("switch", { name: "Offer to record Google Meet" });
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText(/only record when everyone knows/i)).toBeTruthy();
    await user.click(toggle);
    await waitFor(() => expect(screen.getByRole("switch", { name: "Offer to record Google Meet" }).getAttribute("aria-checked")).toBe("true"));
    await user.click(screen.getByRole("switch", { name: "Offer to record Google Meet" }));
    await waitFor(() => expect(screen.getByRole("switch", { name: "Offer to record Google Meet" }).getAttribute("aria-checked")).toBe("false"));
  });
});
