import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { MessageSquare } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { PageHeader } from "@/features/shell/PageHeader";
import { onNamed } from "@/ipc/namedEvents";
import { voiceIpc } from "@/ipc/voiceClient";
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

/** The Voice page (spec S3): how to use it, and the history of what you asked and what happened. */
export function VoicePage() {
  const qc = useQueryClient();
  const { data: turns = [] } = useQuery({ queryKey: ["voice-history"], queryFn: () => voiceIpc.history(50) });
  useEffect(() => onNamed("voice://turn", () => void qc.invalidateQueries({ queryKey: ["voice-history"] })), [qc]);

  return (
    <div>
      <PageHeader title="Voice" description="Press Talk at the bottom of the window and speak. Pluto asks before it changes anything." />
      <section aria-label="History" className="overflow-hidden rounded-xl border bg-card">
        <h2 className="flex items-center gap-2 border-b px-4 py-2.5 text-sm font-semibold"><MessageSquare className="size-4 text-muted-foreground" />History</h2>
        {turns.length === 0 ? (
          <p className="px-4 py-10 text-center text-[13px] text-muted-foreground">Nothing yet. Try “Aaj ke tasks dikhao” or “Add a task to send the report tomorrow”.</p>
        ) : (
          <ol aria-label="Assistant history" className="divide-y">{turns.map((t) => <Turn key={t.interaction_id} t={t} />)}</ol>
        )}
      </section>
    </div>
  );
}
