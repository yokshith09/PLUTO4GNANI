import { useEffect, useState } from "react";
import { Check, Clock, Pencil, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { Proposal, ProposalResponse } from "@/ipc/generated";

/** Arguments the person may change before approving. Anything else is shown but fixed. */
export const EDITABLE: Record<string, string> = { title: "Title", body: "Note", match: "Which one", due_date: "Due date (YYYY-MM-DD)", query: "Search for" };

export function secondsLeft(expiresAt: number, now: number): number {
  return Math.max(0, Math.ceil((expiresAt - now) / 1000));
}

/**
 * The write the assistant wants to make (T-304): what will happen, a countdown, and Approve /
 * Reject / Edit. Nothing changes until one of them is chosen; when the time runs out it closes.
 */
export function ProposalCard({ proposal, onRespond, onExpire, now = Date.now }: {
  proposal: Proposal;
  onRespond: (r: ProposalResponse) => void;
  onExpire: () => void;
  now?: () => number;
}) {
  const [left, setLeft] = useState(() => secondsLeft(proposal.expires_at, now()));
  const [editing, setEditing] = useState(false);
  const args = (proposal.args && typeof proposal.args === "object" ? proposal.args : {}) as Record<string, unknown>;
  const [draft, setDraft] = useState<Record<string, string>>(() =>
    Object.fromEntries(Object.keys(EDITABLE).filter((k) => typeof args[k] === "string").map((k) => [k, String(args[k])])),
  );

  useEffect(() => {
    const t = setInterval(() => setLeft(secondsLeft(proposal.expires_at, now())), 250);
    return () => clearInterval(t);
  }, [proposal.expires_at, now]);
  useEffect(() => {
    if (left === 0) onExpire();
  }, [left, onExpire]);

  const save = () => onRespond({ kind: "edit", args: { ...args, ...draft } });
  return (
    <section role="alertdialog" aria-label="Approve this change?" className="w-full max-w-md rounded-xl border bg-card p-4 shadow-lg">
      <p className="text-[14px] font-medium leading-snug">{proposal.summary_text}</p>
      {editing && (
        <div className="mt-3 space-y-2">
          {Object.keys(draft).map((k) => (
            <div key={k} className="space-y-1">
              <label htmlFor={`prop-${k}`} className="text-xs font-medium text-muted-foreground">{EDITABLE[k]}</label>
              <Input id={`prop-${k}`} value={draft[k] ?? ""} onChange={(e) => setDraft((d) => ({ ...d, [k]: e.target.value }))} />
            </div>
          ))}
        </div>
      )}
      <div className="mt-3 flex flex-wrap items-center gap-2">
        {editing ? (
          <>
            <Button size="sm" onClick={save}><Check />Save and approve</Button>
            <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>Back</Button>
          </>
        ) : (
          <>
            <Button size="sm" onClick={() => onRespond({ kind: "approve" })}><Check />Approve</Button>
            <Button size="sm" variant="outline" onClick={() => onRespond({ kind: "reject" })}><X />Reject</Button>
            {Object.keys(draft).length > 0 && <Button size="sm" variant="ghost" onClick={() => setEditing(true)}><Pencil />Edit</Button>}
          </>
        )}
        <span className="ml-auto flex items-center gap-1 text-xs tabular-nums text-muted-foreground" role="timer" aria-label={`${left} seconds left`}>
          <Clock className="size-3" />{left}s
        </span>
      </div>
      <p className="mt-2 text-[11px] text-muted-foreground">You can also say “haan” or “nahi”.</p>
    </section>
  );
}
