import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ipc, type PetPack } from "@/ipc";
import { BEAR_POSES } from "@/features/shell/Bear";
import { PetLibrary } from "./PetLibrary";
import { ROW_FOR_POSE, SPRITE_ROWS, SpritePet, frameAt } from "./SpritePet";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const boba: PetPack = { id: "boba", display_name: "Boba", description: "", rows: 9, format: "webp", source: "codex", reactions: {} };
const mine: PetPack = { id: "pip", display_name: "Pip", description: "", rows: 9, format: "webp", source: "pluto", reactions: {} };

function renderLibrary(settings: Record<string, unknown> = { "pet.pack": null }) {
  vi.spyOn(ipc, "settingsGetAll").mockResolvedValue(settings as never);
  vi.spyOn(ipc, "petsSheet").mockResolvedValue("data:image/webp;base64,AA==");
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <PetLibrary />
    </QueryClientProvider>,
  );
}

describe("sprite player", () => {
  it("uses the OpenPets frame counts and a row for every bear pose", () => {
    expect(Object.values(SPRITE_ROWS).map((r) => r.frames)).toEqual([6, 8, 8, 4, 5, 8, 6, 6, 6]);
    for (const p of BEAR_POSES) expect(SPRITE_ROWS[ROW_FOR_POSE[p]]).toBeTruthy();
    expect(frameAt("jumping", 0)).toBe(0);
    expect(frameAt("jumping", 839)).toBe(4);
    expect(frameAt("jumping", 840)).toBe(0);
  });

  it("shows the right cell of the sheet", () => {
    render(<SpritePet sheet="x.webp" row="failed" animated={false} size={208} label="Boba (failed)" />);
    const el = screen.getByRole("img", { name: "Boba (failed)" });
    expect(el.style.backgroundPosition).toBe("0px -1040px");
  });
});

describe("pet library", () => {
  it("lists Codex and imported pets and picks one as the desktop pet", async () => {
    vi.spyOn(ipc, "petsList").mockResolvedValue([boba, mine]);
    const set = vi.spyOn(ipc, "settingSet").mockResolvedValue(null);
    renderLibrary();
    expect(await screen.findByText("Boba")).toBeTruthy();
    expect(screen.getByText(/From Codex \/ Petdex/)).toBeTruthy();
    // Codex's own packs can't be removed from Pluto; imported ones can.
    expect(screen.queryByRole("button", { name: "Remove Boba" })).toBeNull();
    expect(screen.getByRole("button", { name: "Remove Pip" })).toBeTruthy();
    fireEvent.click(screen.getAllByRole("button", { name: "Use" })[0] as HTMLElement);
    await waitFor(() => expect(set).toHaveBeenCalledWith("pet.pack", "boba"));
  });

  it("imports a folder and refreshes the list", async () => {
    const list = vi.spyOn(ipc, "petsList").mockResolvedValue([]);
    vi.spyOn(ipc, "petsImport").mockResolvedValue(mine);
    renderLibrary();
    expect(await screen.findByText(/No pets yet/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Import folder/ }));
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  });

  it("imports a downloaded pet ZIP and refreshes the list", async () => {
    const list = vi.spyOn(ipc, "petsList").mockResolvedValue([]);
    const zip = vi.spyOn(ipc, "petsImportZip").mockResolvedValue({ ...mine, id: "kayadu-lohar", display_name: "Kayadu" });
    renderLibrary();
    expect(await screen.findByText(/No pets yet/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Import ZIP/ }));
    await waitFor(() => expect(zip).toHaveBeenCalled());
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  });

  it("removing the pet in use goes back to the bear", async () => {
    vi.spyOn(ipc, "petsList").mockResolvedValue([mine]);
    const remove = vi.spyOn(ipc, "petsRemove").mockResolvedValue(null);
    const set = vi.spyOn(ipc, "settingSet").mockResolvedValue(null);
    renderLibrary({ "pet.pack": "pip" });
    fireEvent.click(await screen.findByRole("button", { name: "Remove Pip" }));
    expect(remove).not.toHaveBeenCalled(); // asks first
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(remove).toHaveBeenCalledWith("pip"));
    await waitFor(() => expect(set).toHaveBeenCalledWith("pet.pack", null));
  });
});

describe("the wardrobe on a picture character", () => {
  it("draws what is worn over the sprite, and nothing when nothing is worn", () => {
    const plain = render(<SpritePet sheet="x.webp" label="Boba" />);
    expect(plain.container.querySelector("[data-accessory]")).toBeNull();
    cleanup();
    const { container } = render(<SpritePet sheet="x.webp" label="Boba" wear={["party-hat", "scarf"]} />);
    expect(container.querySelector('[data-accessory="party-hat"]')).toBeTruthy();
    expect(container.querySelector('[data-accessory="scarf"]')).toBeTruthy();
    expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  });
});
