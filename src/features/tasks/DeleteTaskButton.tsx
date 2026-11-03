import { Trash2 } from "lucide-react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { notify } from "@/lib/notify";
import { useTaskActions } from "@/hooks/usePluto";
import { cn } from "@/lib/utils";
import type { Task } from "@/ipc";

/**
 * Delete a task from a list. It asks first, like every other delete (owner 2026-10-28). It used to
 * turn into a "Delete?" button for three seconds, which was easy to click straight through.
 */
export function DeleteTaskButton({ task, className }: { task: Task; className?: string }) {
  const { removeAny } = useTaskActions();
  return (
    <ConfirmAction
      message={`Delete “${task.title}”?`}
      onConfirm={() => void removeAny(task).then(() => notify.info(`Deleted “${task.title}”`))}
    >
      <button
        type="button" aria-label={`Delete ${task.title}`} title="Delete task"
        className={cn("grid size-6 shrink-0 place-items-center rounded text-muted-foreground opacity-0 outline-none hover:bg-destructive/15 hover:text-destructive focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring group-hover:opacity-100 data-[state=open]:opacity-100", className)}
      >
        <Trash2 className="size-3.5" />
      </button>
    </ConfirmAction>
  );
}
