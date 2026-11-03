import { CalendarDays, ExternalLink, Repeat2, Star } from "lucide-react";
import { ipc, type Task, type TaskLink } from "@/ipc";
import { errorMessage } from "@/ipc";
import { notify } from "@/lib/notify";
import { repeatLabel } from "@/lib/repeat";
import { cn } from "@/lib/utils";

/** What to show for a link: its name, else the address without `https://` and `www.`. */
export function linkLabel(l: Pick<TaskLink, "url" | "title">): string {
  if (l.title) return l.title;
  if (l.url.startsWith("mailto:")) return l.url.slice(7);
  return l.url.replace(/^https?:\/\//i, "").replace(/^www\./i, "").replace(/\/$/, "");
}

export const openLink = (url: string) => void ipc.openUrl(url).catch((e: unknown) => notify.error(errorMessage(e)));

/** A star that marks a task or reminder as important (FR-107). Shown always when on, on hover when off. */
export function StarButton({ starred, title, onToggle, className }: { starred: boolean; title: string; onToggle: () => void; className?: string }) {
  return (
    <button
      type="button" aria-pressed={starred} aria-label={starred ? `Unstar “${title}”` : `Star “${title}”`} title={starred ? "Starred: shown first" : "Star it to keep it on top"}
      onClick={(e) => { e.stopPropagation(); onToggle(); }}
      className={cn(
        "grid size-6 shrink-0 place-items-center rounded outline-none hover:bg-accent focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring",
        starred ? "text-primary" : "text-muted-foreground opacity-0 group-hover:opacity-100",
        className,
      )}
    >
      <Star className={cn("size-3.5", starred && "fill-current")} />
    </button>
  );
}

/** A task's links as small chips right next to its title (FR-106). Click opens the page. */
export function LinkChips({ links, max = 3 }: { links: readonly TaskLink[]; max?: number }) {
  if (links.length === 0) return null;
  const shown = links.slice(0, max);
  const more = links.length - shown.length;
  return (
    <span className="flex min-w-0 shrink items-center gap-1" aria-label="Links">
      {shown.map((l) => (
        <button
          key={l.id} type="button" title={l.url} onClick={(e) => { e.stopPropagation(); openLink(l.url); }}
          aria-label={`Open link ${linkLabel(l)}`}
          className="flex max-w-40 items-center gap-1 truncate rounded-md border border-border bg-muted/60 px-1.5 py-0.5 text-[11px] text-foreground/90 outline-none hover:border-primary/50 hover:text-primary focus-visible:ring-2 focus-visible:ring-ring"
        >
          <ExternalLink className="size-3 shrink-0" /><span className="truncate">{linkLabel(l)}</span>
        </button>
      ))}
      {more > 0 && <span className="text-[11px] text-muted-foreground" title={links.slice(max).map(linkLabel).join("\n")}>+{more}</span>}
    </span>
  );
}

/** "Mon, Wed, Fri" for a repeating task, "3 days" for one on chosen days. */
export function PlanBadge({ task }: { task: Pick<Task, "repeat" | "days"> }) {
  if (task.repeat) {
    return (
      <span className="flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground" title="Repeats; tick it each day">
        <Repeat2 className="size-3" />{repeatLabel(task.repeat)}
      </span>
    );
  }
  if (task.days.length > 1) {
    const list = task.days.map((d) => new Date(`${d}T00:00:00`).toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" })).join(", ");
    return (
      <span className="flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground" title={`Planned on ${list}`}>
        <CalendarDays className="size-3" />{task.days.length} days
      </span>
    );
  }
  return null;
}
