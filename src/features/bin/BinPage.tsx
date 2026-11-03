import { Info } from "lucide-react";
import { Panel, PageHeader } from "@/features/shell/PageHeader";
import { BIN_DAYS, BinList, EmptyBinButton } from "./BinList";
import { BinTasks } from "./BinTasks";

export function BinPage() {
  return (
    <div className="mx-auto max-w-3xl">
      <PageHeader title="Bin" description={`Deleted notes are kept for ${BIN_DAYS} days, then removed for good.`} actions={<EmptyBinButton />} />
      <Panel className="p-4"><BinList /><BinTasks /></Panel>
      <p className="mt-3 flex items-center gap-1.5 text-xs text-muted-foreground"><Info className="size-3.5" />Restoring a note puts it back in Notes with its labels.</p>
    </div>
  );
}
