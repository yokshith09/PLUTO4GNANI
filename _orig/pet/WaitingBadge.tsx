import type { WaitingChat } from "@/ipc";
import { cn } from "@/lib/utils";
import { badgeLabel, waited } from "./waiting";

/**
 * The number on the pet's corner (A-1): how many assistant chats wait for you. In the accent, and
 * warm when one of them asks for a permission. Hovering it lists them (see `WaitingList`).
 */
export function WaitingBadge({ chats, onOver, onOut }: { chats: readonly WaitingChat[]; onOver: () => void; onOut: () => void }) {
  if (chats.length === 0) return null;
  const permission = chats.some((c) => c.kind === "permission");
  return (
    <span
      role="status" aria-label={badgeLabel(chats)} data-pet-waiting
      onMouseEnter={onOver} onMouseLeave={onOut}
      className={cn(
        "absolute right-1 top-1 z-10 grid h-6 min-w-6 cursor-default place-items-center rounded-full border-2 border-background px-1.5 text-[12px] font-bold tabular-nums leading-none shadow-md",
        permission ? "bg-amber-400 text-black" : "bg-primary text-primary-foreground",
      )}
    >
      {chats.length}
    </span>
  );
}

/** Who is waiting and for what, one row per chat. */
export function WaitingList({ chats, now, width, onOver, onOut, onGo }: {
  chats: readonly WaitingChat[]; now: number; width: number; onOver?: () => void; onOut?: () => void; onGo?: (c: WaitingChat) => void;
}) {
  return (
    <section aria-label="Waiting for you" onMouseEnter={onOver} onMouseLeave={onOut} className="pet-bubble-in shrink-0 rounded-2xl border bg-popover p-3 text-popover-foreground shadow-lg" style={{ width }}>
      <h2 className="mb-1.5 text-[12px] font-semibold text-muted-foreground">{chats.length} waiting for you</h2>
      <ul className="divide-y">
        {chats.slice(0, 5).map((c) => (
          <li key={`${c.assistant}|${c.folder}`}>
            <RowTag chat={c} onGo={onGo} className="flex w-full items-start gap-2 py-1.5 text-left text-[12.5px]">
            <span className={cn("mt-0.5 shrink-0 rounded-md px-1.5 py-0.5 text-[10.5px] font-medium", c.kind === "permission" ? "bg-amber-400/20 text-amber-300" : "bg-primary/15 text-primary")}>
              {c.kind === "permission" ? "permission" : "needs you"}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate font-medium">{c.folder || c.assistant}</span>
              <span className="block truncate text-muted-foreground">{c.detail ? `${c.detail} · ${c.assistant}` : c.assistant}</span>
            </span>
            <span className="shrink-0 tabular-nums text-muted-foreground">{waited(c.since, now)}</span>
            </RowTag>
          </li>
        ))}
      </ul>
      {chats.length > 5 && <p className="pt-1 text-[11px] text-muted-foreground">and {chats.length - 5} more</p>}
    </section>
  );
}

/** A row you can click to go to that chat's window (A-4) when its window is known; plain text otherwise. */
function RowTag({ chat, onGo, className, children }: { chat: WaitingChat; onGo: ((c: WaitingChat) => void) | undefined; className: string; children: React.ReactNode }) {
  if (!chat.jump || !onGo) return <div className={className}>{children}</div>;
  return (
    <button type="button" onClick={() => onGo(chat)} title="Click to bring this chat's window forward" aria-label={`Go to ${chat.folder || chat.assistant}, ${chat.assistant}`}
      className={cn(className, "cursor-pointer rounded-md outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring")}>
      {children}
    </button>
  );
}
