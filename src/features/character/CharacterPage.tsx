import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { PageHeader, Panel } from "@/features/shell/PageHeader";
import { BEAR_POSES, Bear, type BearPose } from "@/features/shell/Bear";
import { BACK_TURN_SHAKE, playEmote, type EmoteRun } from "@/pet/emotes";
import { PetLibrary } from "./PetLibrary";
import { PackMirror, PackPreview } from "./PackPreview";
import { useQuery } from "@tanstack/react-query";
import { ipc } from "@/ipc";
import { NudgeSettings } from "@/features/settings/NudgeSettings";
import { CompanionPanel } from "@/features/companion/CompanionPanel";
import { WardrobePanel } from "@/features/wardrobe/WardrobePanel";
import { useWorn } from "@/features/wardrobe/useWardrobe";
import { ReactionsPanel } from "@/features/companion/ReactionsPanel";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useSetting } from "@/hooks/useTheme";

const SIZES = { S: 64, M: 96, L: 128 } as const;

/** What each pose is for, as shown on the preview buttons. */
const POSE_LABELS: Record<BearPose, string> = {
  idle: "Idle", wave: "Hello (hover)", type: "Typing with you", wait: "Waiting for you", stow: "Laptop away", sleep: "Sleepy", work: "Working", cheer: "Task done", party: "All done",
  catch: "Task added", worried: "Worried", ring: "Reminder", toss: "Task deleted", jump: "Jump", sad: "Failed", review: "Reviewing",
  turn: "Three-quarter view", back: "Back view", back_look: "Looking back",
};

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 py-3.5">
      <div className="min-w-[10rem] flex-1">
        <p className="text-[13px] font-medium">{label}</p>
        {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
      </div>
      <div className="ml-auto flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

export function CharacterPage() {
  const { data: packs = [] } = useQuery({ queryKey: ["pets", "library"], queryFn: ipc.petsList });
  const [activePack] = useSetting("pet.pack");
  // The big preview follows the character in use (owner 2026-10-27); clicking another card previews
  // it until the character in use changes.
  const [picked, setPicked] = useState<string | null | undefined>(undefined);
  useEffect(() => setPicked(undefined), [activePack]);
  const shown = picked === undefined ? (activePack ?? null) : picked;
  const shownPack = shown ? packs.find((p) => p.id === shown) : undefined;
  const [pose, setPose] = useState<BearPose>("idle");
  const worn = useWorn();
  const [replay, setReplay] = useState(0);
  const [visible, setVisible] = useSetting("pet.visible");
  const [size, setSize] = useSetting("pet.size");
  const [opacity, setOpacity] = useSetting("pet.opacity");
  const [onTop, setOnTop] = useSetting("pet.always_on_top");
  const [typing, setTyping] = useSetting("pet.typing");
  const [hoverCard, setHoverCard] = useSetting("pet.hover_card");
  const [hideFullscreen, setHideFullscreen] = useSetting("pet.hide_fullscreen");
  const px = SIZES[size ?? "M"];
  const emote = useRef<EmoteRun | null>(null);
  useEffect(() => () => emote.current?.cancel(), []);
  const playBackTurn = () => {
    emote.current?.cancel();
    const reduced = typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    emote.current = playEmote(BACK_TURN_SHAKE, { restore: "idle", set: (p) => { setPose(p); setReplay((n) => n + 1); }, reducedMotion: reduced });
  };

  return (
    <div className="mx-auto max-w-5xl">
      <PageHeader title="Character" description="Your desktop companion. It sits on your screen and stays out of the way." />
      <div className="grid items-stretch gap-4 lg:grid-cols-2 [&_section]:mt-0 [&>*]:min-w-0">
        <Panel className="flex h-full flex-col items-center justify-between gap-5 p-6">
          {shownPack ? <PackPreview key={shownPack.id} pack={shownPack} /> : (
            <>
          <div className="grid min-h-72 w-full flex-1 place-items-center rounded-lg border bg-muted/40 [background-image:radial-gradient(circle_at_center,var(--border)_1px,transparent_1px)] [background-size:16px_16px]">
            <Bear key={`${pose}-${replay}`} pose={pose} wear={worn} animated className="size-56" />
          </div>
          <div className="flex w-full items-center justify-between">
            <div>
              <p className="text-sm font-semibold">Pluto the bear</p>
              <p className="text-xs text-muted-foreground">Default character</p>
            </div>
            <p className="text-xs text-muted-foreground">Pick a moment to preview it</p>
          </div>
          <div className="grid w-full grid-cols-2 gap-1.5 xl:grid-cols-3" role="group" aria-label="Preview pose">
            {BEAR_POSES.map((p) => (
              <Button
                key={p} size="sm" variant={p === pose ? "default" : "outline"} aria-pressed={p === pose} className="justify-start truncate"
                onClick={() => { setPose(p); setReplay((n) => n + 1); }}
              >
                {POSE_LABELS[p]}
              </Button>
            ))}
            <Button size="sm" variant="secondary" className="justify-start truncate" onClick={playBackTurn}>Play back-turn shake</Button>
          </div>
            </>
          )}
        </Panel>

        <Panel className="h-full divide-y px-5">
          <Row label="Show on desktop" hint="Hide the companion without quitting Pluto.">
            <Switch checked={visible ?? true} onCheckedChange={(v) => void setVisible(v)} aria-label="Show on desktop" />
          </Row>
          <Row label="Size" hint={`${px} px on screen`}>
            <ToggleGroup type="single" value={size ?? "M"} onValueChange={(v) => v && void setSize(v as "S" | "M" | "L")} aria-label="Size">
              <ToggleGroupItem value="S">S</ToggleGroupItem>
              <ToggleGroupItem value="M">M</ToggleGroupItem>
              <ToggleGroupItem value="L">L</ToggleGroupItem>
            </ToggleGroup>
          </Row>
          <Row label="Opacity" hint={`${opacity ?? 100}%`}>
            <Slider className="w-36" min={20} max={100} step={5} value={[opacity ?? 100]} onValueChange={([v]) => v !== undefined && void setOpacity(v)} aria-label="Opacity" />
          </Row>
          <Row label="Card when I hover the pet" hint="Today at a glance when you hover the pet. Off: the same facts (focus time left, the next task, the next reminder) come as a short bubble instead.">
            <Switch checked={hoverCard ?? true} onCheckedChange={(v) => void setHoverCard(v)} aria-label="Card when I hover the pet" />
          </Row>
          <Row label="Get out of the way in full screen" hint="Hide while a game, a film, a presentation or a shared screen fills the display. The pet comes back by itself.">
            <Switch checked={hideFullscreen ?? true} onCheckedChange={(v) => void setHideFullscreen(v)} aria-label="Get out of the way in full screen" />
          </Row>
          <Row label="Always on top" hint="Keep the companion above other windows.">
            <Switch checked={onTop ?? true} onCheckedChange={(v) => void setOnTop(v)} aria-label="Always on top" />
          </Row>
          <Row
            label="Type along with me"
            hint="When you type in any app, it takes out a laptop and types too, and waits while a text box is open. Pluto only counts key presses (never which keys) and checks whether a text box is focused (never what is in it). Nothing is saved or sent. On macOS, allow Input Monitoring, and Accessibility for the text-box part."
          >
            <Switch checked={typing ?? false} onCheckedChange={(v) => void setTyping(v)} aria-label="Type along with me" />
          </Row>
        </Panel>

        <div className="lg:col-span-2"><PetLibrary shown={shownPack ? shown : null} onShow={setPicked} /></div>
        {/* The Wardrobe stretches to the height of the column beside it, and its mirror takes the spare
            room, so neither side has an empty gap at any window size (owner 2026-10-31). */}
        <WardrobePanel mirror={shownPack ? <PackMirror pack={shownPack} /> : <Bear pose="idle" wear={worn} animated className="size-36" />} />
        <div className="flex flex-col gap-4">
          {/* Takes up whatever height the Wardrobe beside it needs. */}
          <div className="flex flex-1 flex-col [&>section]:flex [&>section]:flex-1 [&>section]:flex-col [&>section>section]:flex-1 [&>section>section]:content-center"><CompanionPanel /></div>
          <NudgeSettings />
        </div>
        <div className="lg:col-span-2"><ReactionsPanel /></div>
      </div>
    </div>
  );
}
