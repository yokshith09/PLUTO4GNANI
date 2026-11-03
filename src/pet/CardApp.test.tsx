import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { CardApp } from "./CardApp";

beforeAll(() => {
  window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as never;
});
afterEach(cleanup);

type Msg = { kind: string; value: boolean };

describe("the card's own window (owner 2026-11-01)", () => {
  it("shows today's card, tells the pet when the pointer is on it, and passes Sleep and Wake back", async () => {
    const sent: Msg[] = [];
    const listen = (e: Event) => sent.push((e as CustomEvent<Msg>).detail);
    window.addEventListener("pluto-pet-card", listen);
    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <CardApp />
      </QueryClientProvider>,
    );
    const card = await screen.findByRole("status", { name: "Today at a glance" });
    const root = card.parentElement as HTMLElement;
    fireEvent.mouseEnter(root);
    fireEvent.mouseLeave(root);
    expect(sent).toEqual([{ kind: "hover", value: true }, { kind: "hover", value: false }]);
    fireEvent.click(screen.getByRole("button", { name: "Sleep" }));
    expect(sent).toContainEqual({ kind: "nap", value: true });
    expect(screen.getByRole("button", { name: "Wake" })).toBeTruthy();
    // The pet woke up (a click on it): the card is told and shows Sleep again.
    act(() => { window.dispatchEvent(new CustomEvent("pluto-pet-card", { detail: { kind: "napping", value: false } })); });
    await waitFor(() => expect(screen.getByRole("button", { name: "Sleep" })).toBeTruthy());
    window.removeEventListener("pluto-pet-card", listen);
  });
});
