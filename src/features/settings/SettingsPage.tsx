import { PageHeader } from "@/features/shell/PageHeader";
import { mod } from "@/lib/platform";
import { GuideSection } from "@/features/guide/GuideSection";
import { DataSection } from "./DataSection";
import { AutoDeleteSettings } from "./AutoDeleteSettings";
import { VoiceSection } from "./VoiceSection";
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

export type SettingsTab = "general" | "voice";

/** Two tabs: everything about the app, and everything about voice, keys and what is sent to the cloud. */
export function SettingsPage({ tab, onTab }: { tab?: SettingsTab; onTab?: (tab: SettingsTab) => void } = {}) {
  const [own, setOwn] = useState<SettingsTab>("general");
  const current = tab ?? own;
  const choose = (next: SettingsTab) => { setOwn(next); onTab?.(next); };
  const [theme, setTheme] = useSetting("general.theme");
  const [font, setFont] = useSetting("general.font_size");
  const [autostart, setAutostart] = useSetting("general.autostart");
  const [openGlobal] = useSetting("shortcuts.global_open");
  const [noteGlobal] = useSetting("shortcuts.global_new_note");
  const [petGlobal] = useSetting("shortcuts.global_pet");
  const globals: Record<string, string | null | undefined> = { "shortcuts.global_open": openGlobal, "shortcuts.global_new_note": noteGlobal, "shortcuts.global_pet": petGlobal };
  const [demoOnly, setDemoOnly] = useSetting("advanced.demo_pages_only");
  const [confirmReset, setConfirmReset] = useState(false);
  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <PageHeader title="Settings" />
      <ToggleGroup type="single" value={current} onValueChange={(v) => v && choose(v as SettingsTab)} aria-label="Settings sections">
        <ToggleGroupItem value="general">General</ToggleGroupItem>
        <ToggleGroupItem value="voice">Voice &amp; privacy</ToggleGroupItem>
      </ToggleGroup>
      {current === "voice" && <VoiceSection />}
      {current === "general" && (<>
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
        <Row label="Show only the main pages" hint="Hides Time, Character and Archive. Nothing is deleted; turn it off to bring them back.">
          <Switch checked={demoOnly ?? false} onCheckedChange={(v) => void setDemoOnly(v)} aria-label="Show only the main pages" />
        </Row>
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
      </Section>
      </>)}
    </div>
  );
}
