import { useState } from "react";
import { ConfirmAction } from "@/components/ConfirmAction";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FileArchive, FolderOpen, Trash2 } from "lucide-react";
import { notify } from "@/lib/notify";
import { Button } from "@/components/ui/button";
import { Panel } from "@/features/shell/PageHeader";
import { useSetting } from "@/hooks/useTheme";
import { errorMessage, ipc, type PetPack } from "@/ipc";
import { cn } from "@/lib/utils";
import { SpritePet } from "./SpritePet";

const LIBRARY = ["pets", "library"] as const;

/** One pack: its idle animation, name, where it came from, and the actions. */
function PackCard({ pack, active, shown, onShow, onUse, onRemove }: { pack: PetPack; active: boolean; shown: boolean; onShow: () => void; onUse: () => void; onRemove: () => void }) {
  const { data: sheet } = useQuery({ queryKey: ["pets", "sheet", pack.id], queryFn: () => ipc.petsSheet(pack.id), staleTime: Infinity });
  return (
    <Panel className={cn("flex items-center gap-3 p-3", shown && "ring-2 ring-primary")}>
      <button type="button" onClick={onShow} aria-pressed={shown} aria-label={`Preview ${pack.display_name}`} className="flex min-w-0 flex-1 items-center gap-3 rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <span className="grid size-16 shrink-0 place-items-center overflow-hidden rounded-md bg-muted/40">
          {sheet && <SpritePet sheet={sheet} size={64} label={`${pack.display_name} (idle)`} />}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-semibold">{pack.display_name}</span>
          <span className="block truncate text-xs text-muted-foreground">
            {pack.source === "builtin" ? "Built in" : pack.source === "codex" ? "From Codex / Petdex" : "Imported"}
          </span>
        </span>
      </button>
      <Button size="sm" variant={active ? "default" : "outline"} aria-pressed={active} onClick={onUse}>
        {active ? "In use" : "Use"}
      </Button>
      {pack.source === "pluto" && (
        <ConfirmAction message={`Remove ${pack.display_name} from your characters?`} verb="Remove" onConfirm={onRemove}>
          <Button size="icon" variant="ghost" aria-label={`Remove ${pack.display_name}`}>
            <Trash2 className="size-4" />
          </Button>
        </ConfirmAction>
      )}
    </Panel>
  );
}

/**
 * Pet library (FR-63): every Codex / OpenPets pack Pluto can see, the button to import one from a
 * folder, and which one the desktop pet shows. Packs are art only; nothing in them runs.
 */
export function PetLibrary({ shown = null, onShow }: { shown?: string | null; onShow?: (id: string | null) => void } = {}) {
  const qc = useQueryClient();
  const { data: packs = [] } = useQuery({ queryKey: LIBRARY, queryFn: ipc.petsList });
  const [current, setCurrent] = useSetting("pet.pack");
  const [busy, setBusy] = useState(false);
  const refresh = () => qc.invalidateQueries({ queryKey: LIBRARY });

  const importPack = async (from: "zip" | "folder") => {
    setBusy(true);
    try {
      const pack = await (from === "zip" ? ipc.petsImportZip() : ipc.petsImport());
      if (pack) {
        notify.success(`${pack.display_name} added`);
        await refresh();
      }
    } catch (e) {
      notify.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const remove = async (pack: PetPack) => {
    await ipc.petsRemove(pack.id);
    if (current === pack.id) await setCurrent(null);
    await refresh();
  };

  return (
    <section aria-label="Characters" className="mt-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 className="text-[15px] font-semibold">Characters</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">Pick who sits on your desktop. Codex and OpenPets pets work too.</p>
        </div>
        <div className="flex shrink-0 gap-1.5">
          <Button size="sm" disabled={busy} onClick={() => void importPack("zip")}>
            <FileArchive className="size-4" /> Import ZIP
          </Button>
          <Button size="sm" variant="secondary" disabled={busy} onClick={() => void importPack("folder")}>
            <FolderOpen className="size-4" /> Import folder
          </Button>
        </div>
      </div>
      <div className="mt-3 grid gap-2 lg:grid-cols-2">
        <Panel className={cn("flex items-center gap-3 p-3", shown === null && onShow && "ring-2 ring-primary")}>
          <button type="button" onClick={() => onShow?.(null)} aria-pressed={shown === null} aria-label="Preview Pluto the bear" className="min-w-0 flex-1 rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring">
            <span className="block text-sm font-semibold">Pluto the bear</span>
            <span className="block text-xs text-muted-foreground">Built in</span>
          </button>
          <Button size="sm" variant={current ? "outline" : "default"} aria-pressed={!current} onClick={() => void setCurrent(null)}>
            {current ? "Use" : "In use"}
          </Button>
        </Panel>
        {packs.map((p) => (
          <PackCard key={`${p.source}-${p.id}`} pack={p} active={current === p.id} shown={shown === p.id} onShow={() => onShow?.(p.id)} onUse={() => void setCurrent(p.id)} onRemove={() => void remove(p)} />
        ))}
      </div>
      {packs.length === 0 && (
        <p className="mt-2 text-xs text-muted-foreground">No pets yet. Install one with Petdex (for example <code>npx petdex install boba</code>) and it appears here, or use <strong>Import ZIP</strong> for a downloaded pet.</p>
      )}
    </section>
  );
}
