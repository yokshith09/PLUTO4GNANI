import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { BEAR_POSES } from "@/features/shell/Bear";
import { Panel } from "@/features/shell/PageHeader";
import { ipc } from "@/ipc";
import { REACTION_INFO, REACTION_KEYS, parseReactionSettings, type ReactionKey, type ReactionSetting, type ReactionSettings } from "@/pet/reactionSettings";

const KEY = ["prefs", "ui.reactions"] as const;
const DEFAULT: ReactionSetting = { enabled: true, text: "", pose: "" };

/** What the pet does when something happens: the defaults, each one changeable (owner 2026-10-26, J3). */
export function ReactionsPanel() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: KEY, queryFn: async () => parseReactionSettings(await ipc.prefsGet("ui.reactions")) });
  if (!data) return null;
  const save = async (next: ReactionSettings) => {
    qc.setQueryData(KEY, next);
    await ipc.prefsSet("ui.reactions", next);
  };
  const change = (key: ReactionKey, patch: Partial<ReactionSetting>) => void save({ ...data, [key]: { ...DEFAULT, ...data[key], ...patch } });
  const reset = (key: ReactionKey) => {
    void save(Object.fromEntries(Object.entries(data).filter(([k]) => k !== key)) as ReactionSettings);
  };
  return (
    <section aria-label="Reactions" className="mt-6">
      <h2 className="text-[15px] font-semibold">Reactions</h2>
      <p className="mt-0.5 text-xs text-muted-foreground">What it does when something happens. Turn one off, pick another pose, or write your own words with {"{title}"} or {"{left}"}.</p>
      <Panel className="mt-3 divide-y">
        {REACTION_KEYS.map((key) => {
          const info = REACTION_INFO[key];
          const s = { ...DEFAULT, ...data[key] };
          const changed = !!data[key] && (!s.enabled || s.text !== "" || s.pose !== "");
          return (
            <div key={key} role="group" aria-label={info.label} className="grid items-center gap-x-4 gap-y-2 px-4 py-3 md:grid-cols-[minmax(0,14rem)_minmax(0,1fr)_9rem_auto]">
              <div className="flex items-center gap-3">
                <Switch checked={s.enabled} onCheckedChange={(v) => change(key, { enabled: v })} aria-label={`React when ${info.label.toLowerCase()}`} />
                <div className="min-w-0">
                  <p className="text-[13px] font-medium">{info.label}</p>
                  <p className="text-xs text-muted-foreground">{info.hint}</p>
                </div>
              </div>
              <Input
                aria-label={`Words for: ${info.label}`} disabled={!s.enabled} maxLength={80} defaultValue={s.text} key={`${key}-${s.text}`}
                placeholder={info.text || "Default words"}
                onBlur={(e) => e.target.value.trim() !== s.text && change(key, { text: e.target.value.trim() })}
              />
              <select
                aria-label={`Pose for: ${info.label}`} disabled={!s.enabled} value={s.pose} onChange={(e) => change(key, { pose: e.target.value as ReactionSetting["pose"] })}
                className="h-8 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
              >
                <option value="">Default pose</option>
                {BEAR_POSES.map((p) => <option key={p} value={p}>{p}</option>)}
              </select>
              <Button size="sm" variant="ghost" disabled={!changed} onClick={() => reset(key)} aria-label={`Reset ${info.label}`}><RotateCcw />Default</Button>
            </div>
          );
        })}
      </Panel>
    </section>
  );
}
