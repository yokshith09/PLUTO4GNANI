import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { HandHeart } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { meetingAi } from "@/ipc/meetingAi";
import type { Commitment } from "@/ipc/generated";

const shortDate = (ymd: string) => new Date(`${ymd}T00:00:00`).toLocaleDateString([], { month: "short", day: "numeric" });

export function StatusBadge({ c }: { c: Commitment }) {
  switch (c.status) {
    case "overdue": return <Badge variant="destructive">{c.days_overdue ? `${c.days_overdue} day${c.days_overdue === 1 ? "" : "s"} late` : "Late"}</Badge>;
    case "done": return <Badge variant="success">Done</Badge>;
    case "no_task": return <Badge title="No task has been made from this action item">No task yet</Badge>;
    case "open": return <Badge variant="primary">Open</Badge>;
  }
}

/** Who promised what across all meetings, late ones first (T-503). Shows nothing until there is something to show. */
export function CommitmentsPanel() {
  const { data = [] } = useQuery({ queryKey: ["meetings", "commitments"], queryFn: () => meetingAi.commitmentsList() });
  const [owner, setOwner] = useState("");
  const [all, setAll] = useState(false);
  const owners = useMemo(() => [...new Set(data.map((c) => c.owner).filter((o): o is string => !!o))].sort((a, b) => a.localeCompare(b)), [data]);
  const open = data.filter((c) => c.status !== "done" && (!owner || c.owner === owner));
  if (data.length === 0) return null;
  const shown = all ? open : open.slice(0, 6);
  const late = data.filter((c) => c.status === "overdue").length;

  return (
    <section aria-label="Commitments" className="mb-5 overflow-hidden rounded-xl border bg-card">
      <div className="flex flex-wrap items-center gap-2 border-b px-4 py-2.5">
        <HandHeart className="size-4 text-muted-foreground" />
        <h2 className="text-sm font-semibold">Commitments</h2>
        {late > 0 && <Badge variant="destructive">{late} late</Badge>}
        <label htmlFor="commit-owner" className="sr-only">Whose commitments</label>
        <select id="commit-owner" value={owner} onChange={(e) => setOwner(e.target.value)} className="ml-auto h-8 rounded-md border bg-transparent px-2 text-[13px]">
          <option value="">Everyone</option>
          {owners.map((o) => <option key={o} value={o}>{o}</option>)}
        </select>
      </div>
      {open.length === 0 ? (
        <p className="px-4 py-5 text-[13px] text-muted-foreground">{owner ? `${owner} has nothing open.` : "Nothing is open. Everything promised is done."}</p>
      ) : (
        <ul aria-label="Open commitments" className="divide-y">
          {shown.map((c) => (
            <li key={c.action_item_id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2.5 text-[13px]">
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{c.body}</span>
                <span className="block truncate text-xs text-muted-foreground">{c.owner ?? "No owner"} · {c.meeting_title}{c.due_date ? ` · due ${shortDate(c.due_date)}` : ""}</span>
              </span>
              <StatusBadge c={c} />
            </li>
          ))}
        </ul>
      )}
      {open.length > 6 && (
        <button type="button" onClick={() => setAll((v) => !v)} className="w-full border-t px-4 py-2 text-left text-xs text-muted-foreground outline-none hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring">
          {all ? "Show fewer" : `Show all ${open.length}`}
        </button>
      )}
    </section>
  );
}
