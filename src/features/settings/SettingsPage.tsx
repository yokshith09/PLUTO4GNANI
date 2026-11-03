import { PageHeader } from "@/features/shell/PageHeader";
import { mod } from "@/lib/platform";
import { YourProfile } from "@/features/guide/YourProfile";
import { GuideSection } from "@/features/guide/GuideSection";
import { CREDIT } from "@/features/guide/content";
import { DataSection } from "./DataSection";
import { AutoDeleteSettings } from "./AutoDeleteSettings";
import { Row, Section } from "./parts";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useState } from "react";
import { notify } from "@/lib/notify";
import { Button } from "@/components/ui/button";
import { errorMessage, ipc } from "@/ipc";
import { GLOBAL_SHORTCUTS } from "./shortcuts";
import { ShortcutRow } from "./ShortcutRow";
import { useSetting } from "@/hooks/useTheme";

/** The main projects Pluto ships with and their licenses (About › Open-source software). */
const LICENSES: [string, string][] = [
  ["Tauri", "MIT or Apache-2.0"], ["Rust standard library", "MIT or Apache-2.0"], ["Tokio", "MIT"], ["rusqlite / SQLite", "MIT / public domain"],
  ["serde", "MIT or Apache-2.0"], ["chrono", "MIT or Apache-2.0"], ["tracing", "MIT"], ["React", "MIT"],
  ["TanStack Query", "MIT"], ["Radix UI", "MIT"], ["Tailwind CSS", "MIT"], ["lucide icons", "ISC"],
  ["dnd kit", "MIT"], ["react-markdown / remark-gfm", "MIT"], ["class-variance-authority", "Apache-2.0"], ["clsx, tailwind-merge", "MIT"],
];

export function SettingsPage() {
  const [theme, setTheme] = useSetting("general.theme");
  const [font, setFont] = useSetting("general.font_size");
  const [autostart, setAutostart] = useSetting("general.autostart");
  const [openGlobal] = useSetting("shortcuts.global_open");
  const [noteGlobal] = useSetting("shortcuts.global_new_note");
  const [petGlobal] = useSetting("shortcuts.global_pet");
  const globals: Record<string, string | null | undefined> = { "shortcuts.global_open": openGlobal, "shortcuts.global_new_note": noteGlobal, "shortcuts.global_pet": petGlobal };
  const [confirmReset, setConfirmReset] = useState(false);
  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <PageHeader title="Settings" />
      <Section title="General">
        <Row label="Theme" hint="System follows your operating system.">
          <ToggleGroup type="single" value={theme ?? "dark"} onValueChange={(v) => v && void setTheme(v as "system" | "light" | "dark")} aria-label="Theme">
            <ToggleGroupItem value="system">System</ToggleGroupItem>
            <ToggleGroupItem value="light">Light</ToggleGroupItem>
            <ToggleGroupItem value="dark">Dark</ToggleGroupItem>
          </ToggleGroup>
        </Row>
        <Row label="Text size">
          <ToggleGroup type="single" value={font ?? "M"} onValueChange={(v) => v && void setFont(v as "S" | "M" | "L")} aria-label="Text size">
            <ToggleGroupItem value="S">Small</ToggleGroupItem>
            <ToggleGroupItem value="M">Default</ToggleGroupItem>
            <ToggleGroupItem value="L">Large</ToggleGroupItem>
          </ToggleGroup>
        </Row>
        <Row label="Open Pluto at login" hint="Starts quietly in the tray.">
          <Switch checked={autostart ?? false} onCheckedChange={(v) => void setAutostart(v)} aria-label="Open Pluto at login" />
        </Row>
      </Section>
      <YourProfile />
      <GuideSection />
      <Section title="Keyboard shortcuts">
        {GLOBAL_SHORTCUTS.map((g) => (
          <ShortcutRow
            key={g.key} settingKey={g.key} label={g.label} fallback={g.fallback}
            taken={GLOBAL_SHORTCUTS.filter((o) => o.key !== g.key).map((o) => globals[o.key] ?? o.fallback).filter(Boolean)}
          />
        ))}
        {[
          [`${mod}+1 … 5`, "Today overview, Tasks, Notes, Time, Meetings"],
          [`${mod}+T / ${mod}+M`, "New task / new meeting"],
          [`${mod}+K / ${mod}+N`, "Search notes / new note (on Notes)"],
          [`${mod}+,`, "Settings"],
        ].map(([keys, what]) => (
          <Row key={keys} label={what ?? ""}><span className="rounded-md border bg-muted px-2 py-0.5 font-mono text-[12px]">{keys}</span></Row>
        ))}
      </Section>
      <AutoDeleteSettings />
      <DataSection />
      <Section title="Advanced">
        {/* Log rows removed (FR-47, owner V2-07): logging stays on for bug reports, it is just not shown. */}
        <Row label="Reset settings" hint="Every setting on this page back to its default. Your notes, tasks, meetings and layout stay.">
          {confirmReset ? (
            <div className="flex gap-2">
              <Button size="sm" variant="ghost" onClick={() => setConfirmReset(false)}>Keep</Button>
              <Button size="sm" variant="destructive" onClick={() => void ipc.settingsReset().then(() => { setConfirmReset(false); notify.info("Settings are back to their defaults"); }, (e: unknown) => notify.error(errorMessage(e)))}>Reset</Button>
            </div>
          ) : (
            <Button size="sm" variant="secondary" onClick={() => setConfirmReset(true)}>Reset settings…</Button>
          )}
        </Row>
      </Section>
      <Section title="About">
        <Row label="Pluto" hint={`Version ${__APP_VERSION__} · Your notes, tasks and reminders are stored only on this computer.`}>
          <span />
        </Row>
        <Row label={CREDIT} hint="Pluto: small companion, big universe. Made with care for people who want a calm place to plan their day.">
          <span />
        </Row>
        <details className="py-3.5 text-[13px]">
          <summary className="cursor-pointer font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring">Open-source software</summary>
          <p className="mt-2 text-xs text-muted-foreground">Pluto is built on these projects, used under their licenses:</p>
          <ul className="mt-2 grid gap-x-6 gap-y-1 text-xs text-muted-foreground sm:grid-cols-2">
            {LICENSES.map(([name, license]) => <li key={name}><span className="text-foreground">{name}</span> · {license}</li>)}
          </ul>
        </details>
      </Section>
    </div>
  );
}
