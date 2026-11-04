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
