import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ConfirmAction } from "@/components/ConfirmAction";
import { RotateCcw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { errorMessage, ipc } from "@/ipc";
import { notify } from "@/lib/notify";

/** Completed tasks moved here by auto-delete (FR-46): restore, or delete for good. */
export function BinTasks() {
  const qc = useQueryClient();
  const { data: tasks = [] } = useQuery({ queryKey: ["tasks", "binned"], queryFn: ipc.tasksBinned });
  if (tasks.length === 0) return null;
  const act = async (p: Promise<unknown>, done: string) => {
    try {
      await p;
      await qc.invalidateQueries({ queryKey: ["tasks"] });
      notify.info(done);
    } catch (e) {
      notify.error(errorMessage(e));
    }
  };
  return (
    <section aria-label="Tasks in the Bin" className="mt-4">
      <h2 className="mb-2 text-[13px] font-semibold">Completed tasks <span className="font-normal text-muted-foreground">· moved here by auto-delete, kept 30 days</span></h2>
      <ul className="divide-y rounded-lg border">
        {tasks.map((t) => (
          <li key={t.id} className="flex items-center gap-2 px-3 py-2 text-[13px]">
            <span className="min-w-0 flex-1 truncate">{t.title}</span>
            <Button size="sm" variant="ghost" aria-label={`Restore task “${t.title}”`} onClick={() => void act(ipc.taskRestore(t.id), "Task restored")}><RotateCcw />Restore</Button>
            <ConfirmAction message="Delete this task for good? It can't be brought back." onConfirm={() => void act(ipc.taskDelete(t.id), "Task deleted")}>
              <Button size="sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label={`Delete task “${t.title}” for good`}><Trash2 /></Button>
            </ConfirmAction>
          </li>
        ))}
      </ul>
    </section>
  );
}
