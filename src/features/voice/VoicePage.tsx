import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { MessageSquare, Send, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { PageHeader } from "@/features/shell/PageHeader";
import { onNamed } from "@/ipc/namedEvents";
import { voiceErrorMessage, voiceIpc } from "@/ipc/voiceClient";
import { notify } from "@/lib/notify";
import type { AssistantTurn } from "@/ipc/generated";

export const INTENT_LABEL: Record<AssistantTurn["decision"]["intent"], string> = {
  meeting_query: "Meetings", task: "Tasks", reminder: "Reminders", note: "Notes", focus: "Focus", project: "Projects", navigate: "Navigation",
  approval: "Yes", reject: "No", cancel: "Cancel", noise: "Not a command",
};

function outcome(t: AssistantTurn): { text: string; tone: "success" | "destructive" | "default" } {
  if (t.result) return { text: t.result.message, tone: t.result.ok ? "success" : "default" };
  if (t.proposal) return { text: "Waiting for your answer", tone: "default" };
  return { text: t.decision.tool ? "Answered" : "No action", tone: "default" };
}

function Turn({ t }: { t: AssistantTurn }) {
  const o = outcome(t);
  return (
    <li className="space-y-1.5 px-4 py-3">
      <p className="text-[13.5px] font-medium">“{t.transcript}”</p>
      <div className="flex flex-wrap items-center gap-1.5 text-[11px]">
        <Badge>{INTENT_LABEL[t.decision.intent]}</Badge>
        <Badge variant={t.decision.source === "rules" ? "default" : "violet"} title={t.decision.source === "rules" ? "Answered by a built-in rule: nothing was sent to the cloud for the decision" : "Understood by the language service"}>
          {t.decision.source === "rules" ? "Rule" : "AI"}
        </Badge>
        {t.decision.tool && <Badge title="The action Pluto picked">{t.decision.tool}</Badge>}
        {t.language && <span className="text-muted-foreground">{t.language}</span>}
        <span className="text-muted-foreground tabular-nums">{t.decision.latency_ms} ms</span>
      </div>
      {t.reply_text && <p className="text-[13px] text-muted-foreground">Pluto: {t.reply_text}</p>}
      <p className={o.tone === "success" ? "text-[12px] text-success" : "text-[12px] text-muted-foreground"}>{o.text}</p>
    </li>
  );
}

/** Things to say, grouped by what they do. Pressing one runs it as if it had been said. */
export const EXAMPLES: { group: string; lines: string[] }[] = [
  { group: "Tasks", lines: ["Add a task to send the report to Akhil on the 11th", "Which tasks do I have today?", "On which day is the report task?", "Move the report task to the 11th", "Mark the report task done"] },
  { group: "Reminders", lines: ["Remind me at 5 pm to call the dentist", "What are my reminders?"] },
  { group: "Notes and focus", lines: ["Add a note: budget review went well", "Start a 25 minute focus"] },
  { group: "Meetings and pages", lines: ["What did we decide about the budget?", "Open Meetings"] },
];

/**
 * The Voice page (spec S3): type or say a command, things to try, and the history of what you
 * asked and what happened (older sessions included) with a way to clear it.
 */
export function VoicePage() {
  const qc = useQueryClient();
  const { data: turns = [] } = useQuery({ queryKey: ["voice-history"], queryFn: () => voiceIpc.history(100) });
  useEffect(() => onNamed("voice://turn", () => void qc.invalidateQueries({ queryKey: ["voice-history"] })), [qc]);
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [confirm, setConfirm] = useState(false);

  const run = async (command: string) => {
    const t = command.trim();
    if (!t || sending) return;
    setSending(true);
    try {
      await voiceIpc.textTurn(t);
      setText("");
    } catch (e) {
      notify.error(voiceErrorMessage(e));
    } finally {
      setSending(false);
      void qc.invalidateQueries({ queryKey: ["voice-history"] });
    }
  };
  const clear = async () => {
    try {
      await voiceIpc.historyClear();
      setConfirm(false);
      await qc.invalidateQueries({ queryKey: ["voice-history"] });
      notify.info("History cleared");
    } catch (e) {
      notify.error(voiceErrorMessage(e));
    }
  };

  return (
    <div>
      <PageHeader title="Voice" description="Say “Pluto” or press Talk at the bottom of the window, or type a command here. Pluto asks before it changes anything." />

      <section aria-label="Ways to talk" className="mb-4 rounded-xl border bg-card p-4">
        <h2 className="text-sm font-semibold">Ways to talk</h2>
        <ul className="mt-2 grid gap-x-6 gap-y-1.5 text-[13px] sm:grid-cols-2">
          <li><kbd className="rounded border bg-muted px-1.5 py-0.5 text-[11px]">Ctrl</kbd> <kbd className="rounded border bg-muted px-1.5 py-0.5 text-[11px]">Alt</kbd> <kbd className="rounded border bg-muted px-1.5 py-0.5 text-[11px]">Space</kbd> <span className="text-muted-foreground">Press once, say your command, stop talking. It ends by itself.</span></li>
          <li><kbd className="rounded border bg-muted px-1.5 py-0.5 text-[11px]">Ctrl</kbd> <kbd className="rounded border bg-muted px-1.5 py-0.5 text-[11px]">Alt</kbd> <kbd className="rounded border bg-muted px-1.5 py-0.5 text-[11px]">D</kbd> <span className="text-muted-foreground">Dictation: what you say is typed where your cursor is. Press again to stop.</span></li>
          <li><span className="font-medium">Hold the talk key</span> <span className="text-muted-foreground">(Settings → Voice &amp; privacy) and release to send.</span></li>
          <li><span className="font-medium">Say “Pluto…”</span> <span className="text-muted-foreground">with always-on listening turned on and taught.</span></li>
        </ul>
      </section>

      <section aria-label="Commands" className="mb-4 rounded-xl border bg-card p-4">
        <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void run(text); }}>
          <label htmlFor="voice-command" className="sr-only">Type a command</label>
          <Input id="voice-command" value={text} onChange={(e) => setText(e.target.value)} placeholder="Add a task to send the report to Akhil on the 11th" disabled={sending} />
          <Button type="submit" disabled={sending || !text.trim()}><Send />Send</Button>
        </form>
        <div className="mt-3 space-y-2">
          {EXAMPLES.map((g) => (
            <div key={g.group} className="flex flex-wrap items-center gap-1.5">
              <span className="w-28 shrink-0 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{g.group}</span>
              {g.lines.map((l) => (
                <button key={l} type="button" disabled={sending} onClick={() => void run(l)} className="rounded-full border bg-muted/40 px-2.5 py-1 text-[12px] outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50">{l}</button>
              ))}
            </div>
          ))}
        </div>
      </section>

      <section aria-label="History" className="overflow-hidden rounded-xl border bg-card">
        <div className="flex items-center gap-2 border-b px-4 py-2.5">
          <h2 className="flex items-center gap-2 text-sm font-semibold"><MessageSquare className="size-4 text-muted-foreground" />History</h2>
          {turns.length > 0 && (
            <div className="ml-auto flex items-center gap-2">
              {confirm ? (
                <>
                  <span className="text-xs text-muted-foreground">Delete all {turns.length} entries?</span>
                  <Button size="sm" variant="ghost" onClick={() => setConfirm(false)}>Keep</Button>
                  <Button size="sm" variant="destructive" onClick={() => void clear()}>Delete</Button>
                </>
              ) : (
                <Button size="sm" variant="ghost" onClick={() => setConfirm(true)}><Trash2 />Clear history</Button>
              )}
            </div>
          )}
        </div>
        {turns.length === 0 ? (
          <p className="px-4 py-10 text-center text-[13px] text-muted-foreground">Nothing yet. Try one of the examples above.</p>
        ) : (
          <ol aria-label="Assistant history" className="divide-y">{turns.map((t) => <Turn key={t.interaction_id} t={t} />)}</ol>
        )}
      </section>
    </div>
  );
}
