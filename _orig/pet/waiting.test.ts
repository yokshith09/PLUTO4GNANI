import { describe, expect, it } from "vitest";
import type { WaitingChat } from "@/ipc";
import { WAIT_EXPIRES_MS, badgeLabel, stillWaiting, waited } from "./waiting";

const c = (folder: string, since: number, kind: WaitingChat["kind"] = "needs"): WaitingChat => ({ assistant: "Claude Code", folder, detail: "", kind, since, jump: false });

describe("waiting chats (A-1)", () => {
  it("keeps what waited less than half an hour, longest waiting first", () => {
    const now = 10 * WAIT_EXPIRES_MS;
    const list = stillWaiting([c("new", now - 1000), c("old", now - 60_000), c("gone", now - WAIT_EXPIRES_MS)], now);
    expect(list.map((x) => x.folder)).toEqual(["old", "new"]);
  });

  it("says how long in minutes then hours", () => {
    expect(waited(1000, 1000 + 20_000)).toBe("now");
    expect(waited(0, 4 * 60_000 + 5)).toBe("4m");
    expect(waited(0, 90 * 60_000)).toBe("1h");
  });

  it("reads out the count and the permissions", () => {
    expect(badgeLabel([c("a", 0)])).toBe("1 chat waiting for you");
    expect(badgeLabel([c("a", 0), c("b", 0, "permission")])).toBe("2 chats waiting for you, 1 asks permission");
    expect(badgeLabel([c("a", 0, "permission"), c("b", 0, "permission")])).toBe("2 chats waiting for you, 2 ask permission");
  });
});
