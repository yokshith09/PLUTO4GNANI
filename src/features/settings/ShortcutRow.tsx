import { useEffect, useState } from "react";
import { notify } from "@/lib/notify";
import { Button } from "@/components/ui/button";
import { useSetting } from "@/hooks/useTheme";
import { comboFromEvent, showCombo } from "./shortcuts";
import { Row } from "./parts";

type Key = "shortcuts.global_open" | "shortcuts.global_new_note" | "shortcuts.global_pet";

function Keys({ combo }: { combo: string }) {
  if (!combo) return <span className="text-[12px] text-muted-foreground">Off</span>;
  return <span className="rounded-md border bg-muted px-2 py-0.5 font-mono text-[12px]">{showCombo(combo)}</span>;
}

/** A global shortcut you can change: "Change", press the keys, done. Esc cancels. */
export function ShortcutRow({ settingKey, label, fallback, taken }: { settingKey: Key; label: string; fallback: string; taken: string[] }) {
  const [value, setValue] = useSetting(settingKey);
  const [recording, setRecording] = useState(false);
  const combo = value ?? fallback;

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") return setRecording(false);
      const next = comboFromEvent(e);
      if (!next) return; // still holding modifiers, or not a usable key
      setRecording(false);
      if (taken.includes(next)) return void notify.error(`${showCombo(next)} is already used by another Pluto shortcut.`);
      void setValue(next === fallback ? null : next);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, taken, fallback, setValue]);

  return (
    <Row label={label} {...(value ? { hint: fallback ? `Default: ${showCombo(fallback)}` : "Off by default" } : {})}>
      <div className="flex items-center gap-2">
        {recording ? (
          <span className="rounded-md border border-primary px-2 py-0.5 text-[12px] text-primary" role="status">Press the new keys… (Esc to cancel)</span>
        ) : (
          <Keys combo={combo} />
        )}
        <Button size="sm" variant="ghost" onClick={() => setRecording((r) => !r)} aria-label={`Change shortcut: ${label}`}>{recording ? "Cancel" : "Change"}</Button>
        {value && !recording && <Button size="sm" variant="ghost" onClick={() => void setValue(null)} aria-label={`Reset shortcut: ${label}`}>{fallback ? "Reset" : "Turn off"}</Button>}
      </div>
    </Row>
  );
}

export { Keys };
