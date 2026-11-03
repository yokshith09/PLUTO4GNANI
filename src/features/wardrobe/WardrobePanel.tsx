import type { ReactNode } from "react";
import { Shirt } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Panel } from "@/features/shell/PageHeader";
import { petSay } from "@/ipc";
import { cn } from "@/lib/utils";
import { AccessoryIcon } from "./Accessories";
import { ITEMS, OUTFITS, toggleItem, toggleOutfit, type Wardrobe } from "./wardrobe";
import { useWardrobe } from "./useWardrobe";

/** Every tile is the same size, with the picture above the name, so the rows line up (owner 2026-10-28). */
const tile = (on: boolean) =>
  cn("flex flex-col items-center gap-1.5 rounded-lg border px-2 py-2.5 text-[12.5px] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
    on ? "border-primary bg-primary/10 font-medium text-foreground" : "bg-card text-muted-foreground hover:border-foreground/25 hover:text-foreground");

/** Accessories and outfits for any character. All free, none locked; one thing per place on the body. */
export function WardrobePanel({ mirror }: { mirror?: ReactNode } = {}) {
  const { wardrobe, save } = useWardrobe();
  if (!wardrobe) return null;
  const apply = (next: Wardrobe) => {
    void save(next);
    // The pet window is a separate window: this makes it look at the new look straight away.
    void petSay({ text: next.worn.length ? "New look!" : "Back to plain me", pose: "cheer", ms: 3500 });
  };
  return (
    <section aria-label="Accessories and outfits" className="flex h-full flex-col">
      <h2 className="text-[15px] font-semibold">Accessories and outfits</h2>
      <p className="mt-0.5 text-xs text-muted-foreground">Dress your pet, whichever character it is. Everything is free. One thing for the head, the face and the neck.</p>
      <Panel className="mt-3 flex flex-1 flex-col gap-4 p-4">
        {mirror && (
          <div role="img" aria-label="Your pet in its current look" className="grid min-h-36 flex-1 place-items-center rounded-lg border bg-muted/40 [background-image:radial-gradient(circle_at_center,var(--border)_1px,transparent_1px)] [background-size:16px_16px]">
            {mirror}
          </div>
        )}
        <div role="group" aria-label="Outfits">
          <p className="text-xs font-medium text-muted-foreground">Outfits</p>
          <div className="mt-2 grid grid-cols-2 gap-1.5 sm:grid-cols-3">
            {OUTFITS.map((o) => (
              <button key={o.id} type="button" aria-pressed={wardrobe.outfit === o.id} className={tile(wardrobe.outfit === o.id)} onClick={() => apply(toggleOutfit(wardrobe, o.id))}>
                <span className="flex h-9 w-full items-center justify-center gap-0.5">
                  {o.items.map((i) => <AccessoryIcon key={i} id={i} className="h-8 min-w-0 flex-1" />)}
                </span>
                <span className="w-full truncate text-center">{o.name}</span>
              </button>
            ))}
          </div>
        </div>
        <div role="group" aria-label="Accessories">
          <p className="text-xs font-medium text-muted-foreground">Accessories</p>
          <div className="mt-2 grid grid-cols-2 gap-1.5 sm:grid-cols-3">
            {ITEMS.map((i) => {
              const on = wardrobe.worn.includes(i.id);
              return (
                <button key={i.id} type="button" aria-pressed={on} className={tile(on)} onClick={() => apply(toggleItem(wardrobe, i.id))}>
                  <AccessoryIcon id={i.id} className="h-9 w-full" />
                  <span className="w-full truncate text-center">{i.name}</span>
                </button>
              );
            })}
          </div>
        </div>
        <div className="flex items-center justify-between gap-3 border-t pt-3">
          <div>
            <p className="text-[13px] font-medium">Party hat on birthdays</p>
            <p className="text-xs text-muted-foreground">Yours and the pet's. It goes back to your look the next day.</p>
          </div>
          <Switch checked={wardrobe.partyOnBirthday} onCheckedChange={(v) => void save({ ...wardrobe, partyOnBirthday: v })} aria-label="Party hat on birthdays" />
        </div>
        <Button size="sm" variant="outline" disabled={wardrobe.worn.length === 0} onClick={() => apply({ ...wardrobe, worn: [], outfit: "" })}><Shirt />Take everything off</Button>
      </Panel>
    </section>
  );
}
