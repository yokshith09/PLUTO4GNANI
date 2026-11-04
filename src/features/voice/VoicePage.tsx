import { useEffect, useState } from "react";
import { Mic } from "lucide-react";
import { Button } from "@/components/ui/button";
import { PageHeader } from "@/features/shell/PageHeader";
import { onNamed } from "@/ipc/namedEvents";
import { voiceErrorMessage, voiceIpc } from "@/ipc/voiceClient";
import type { Proposal, ToolResult, VoiceStatus } from "@/ipc/generated";

const LABEL: Record<VoiceStatus["state"], string> = {
  disabled: "Cloud voice is off", idle: "Ready", listening: "Listening…", thinking: "Thinking…",
  awaiting_approval: "Waiting for your answer", executing: "Working…", speaking: "Speaking…", recording: "Recording a meeting", error: "Something went wrong",
};

/** Placeholder screen for T-006: it proves the commands and events work against the mock.
 *  The real design (UI_UX_SPEC) replaces it in T-303. */
export function VoicePage() {
  const [status, setStatus] = useState<VoiceStatus | null>(null);
  const [proposal, setProposal] = useState<Proposal | null>(null);
  const [result, setResult] = useState<ToolResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void voiceIpc.status().then(setStatus);
    const offs = [
      onNamed("voice://status", setStatus),
      onNamed("voice://proposal", (p) => { setProposal(p); setResult(null); }),
      onNamed("voice://error", (e) => setError(e.message)),
    ];
    return () => offs.forEach((off) => off());
  }, []);

  const run = async (f: () => Promise<unknown>) => {
    setError(null);
    try { await f(); } catch (e) { setError(voiceErrorMessage(e)); }
  };
  const listening = status?.state === "listening";
  const answer = (kind: "approve" | "reject") => run(async () => {
    if (!proposal) return;
    setResult(await voiceIpc.proposalRespond(proposal.id, { kind }));
    setProposal(null);
  });

  return (
    <div className="space-y-6">
      <PageHeader title="Voice" description="Talk to Pluto. Nothing changes until you approve it." />
      <p role="status" className="text-[13px] text-muted-foreground">{status ? LABEL[status.state] : "…"}{status?.detail ? ` · ${status.detail}` : ""}</p>
      <Button onClick={() => void run(() => (listening ? voiceIpc.pttStop() : voiceIpc.pttStart()))}>
        <Mic className="size-4" />{listening ? "Stop and send" : "Push to talk"}
      </Button>
      {error && <p role="alert" className="text-[13px] text-destructive">{error}</p>}
      {proposal && (
        <section aria-label="Approval" className="max-w-md space-y-3 rounded-lg border p-4">
          <p className="text-[13.5px]">{proposal.summary_text}</p>
          <div className="flex gap-2">
            <Button onClick={() => void answer("approve")}>Approve</Button>
            <Button variant="outline" onClick={() => void answer("reject")}>Reject</Button>
          </div>
        </section>
      )}
      {result && <p className="text-[13px]">{result.message}</p>}
    </div>
  );
}
