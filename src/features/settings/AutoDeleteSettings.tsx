import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { errorMessage, ipc, type RetentionRules } from "@/ipc";
import { notify } from "@/lib/notify";
import { AGES, previewText, useAutoDeleteRules } from "./autoDelete";
import { Row, Section } from "./parts";

const select = "h-8 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring";

/**
 * Auto-delete (FR-46). Only time history and completed tasks; notes, meetings, reminders and
 * open tasks are never touched. A stricter rule shows what it would do before it is saved.
 */
export function AutoDeleteSettings() {
  const { rules, save } = useAutoDeleteRules();
  const [pending, setPending] = useState<{ next: RetentionRules; text: string } | null>(null);

  const change = async (next: RetentionRules) => {
    const stricter = (n: number, o: number) => n !== 0 && (o === 0 || n < o);
    if (!stricter(next.time_days, rules.time_days) && !stricter(next.tasks_days, rules.tasks_days)) return void save(next);
    try {
      setPending({ next, text: previewText(await ipc.retentionPreview(next)) });
    } catch (e) {
      notify.error(errorMessage(e));
    }
  };
  const confirm = async () => {
    if (!pending) return;
    await save(pending.next);
    const c = await ipc.retentionApply(pending.next).catch((e: unknown) => { notify.error(errorMessage(e)); return null; });
    setPending(null);
    if (c) notify.info("Auto-delete is on", { description: previewText(c).replace(" will be deleted", " deleted").replace(" will move", " moved") });
  };

  return (
    <Section title="Auto-delete">
      <Row label="App and website time" hint="Delete time history older than this. Notes, meetings, reminders and open tasks are never touched.">
        <select aria-label="Delete time history after" className={select} value={rules.time_days} onChange={(e) => void change({ ...rules, time_days: Number(e.target.value) })}>
          {AGES.map((a) => <option key={a.days} value={a.days}>{a.label}</option>)}
        </select>
      </Row>
      <Row label="Completed tasks" hint="Move tasks finished longer ago than this to the Bin. You can restore them there for 30 days.">
        <select aria-label="Move completed tasks to the Bin after" className={select} value={rules.tasks_days} onChange={(e) => void change({ ...rules, tasks_days: Number(e.target.value) })}>
          {AGES.map((a) => <option key={a.days} value={a.days}>{a.label}</option>)}
        </select>
      </Row>
      <Dialog open={pending !== null} onOpenChange={(o) => !o && setPending(null)}>
        <DialogContent className="max-w-md">
          <DialogTitle>Turn on auto-delete?</DialogTitle>
          <DialogDescription>{pending?.text}</DialogDescription>
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setPending(null)}>Keep everything</Button>
            <Button onClick={() => void confirm()}>Turn on</Button>
          </div>
        </DialogContent>
      </Dialog>
    </Section>
  );
}
