import { useState, type ReactElement } from "react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

/**
 * Asks again before something is deleted or removed (owner 2026-10-27: every delete and every ✕
 * confirms). Wrap the button that would do it; its own `onClick` is left off, the action goes in
 * `onConfirm`. Cancel has the focus, so pressing Enter by habit does not delete.
 */
export function ConfirmAction({ message, verb = "Delete", onConfirm, children }: {
  message: string;
  /** The word on the confirming button: Delete, Remove, Close… */
  verb?: string;
  onConfirm: () => void;
  children: ReactElement;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      {/* The popover is drawn elsewhere on the page, but React still sends its clicks up to whatever
          contains the button (a note card that opens on click, say), so they are stopped here. Escape is left alone. */}
      <PopoverContent
        role="alertdialog" aria-label={message} className="w-60" align="end"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && e.stopPropagation()}
      >
        <p className="text-[13px] font-medium">{message}</p>
        <div className="mt-3 flex justify-end gap-1.5">
          <Button size="sm" variant="outline" onClick={() => setOpen(false)}>Cancel</Button>
          <Button size="sm" variant="destructive" onClick={() => { setOpen(false); onConfirm(); }}>{verb}</Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
