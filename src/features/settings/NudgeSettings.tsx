import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Switch } from "@/components/ui/switch";
import { ipc } from "@/ipc";
import { GAP_CHOICES, MAX_PER_DAY_RANGE, parseNudges, type NudgeState } from "@/lib/nudges";
import { useRepeatRemindPrefs } from "@/features/tasks/useRepeatReminder";
import { useWellbeingPrefs } from "@/features/wellbeing/useWellbeing";
import { EVERY_CHOICES, KINDS, type WellbeingKind } from "@/features/wellbeing/wellbeing";
import { Row, Section } from "./parts";

const KEY = ["prefs", "ui.nudges"] as const;
const gapLabel = (m: number) => (m < 60 ? `${m} min` : `${m / 60} h`);
const select = "h-8 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring";

/**
 * Pet notifications (FR-96): how many unasked nudges a day, how far apart, and quiet hours.
 * Reminders you set yourself always come on time and don't count.
 */
export function NudgeSettings() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: KEY, queryFn: async () => parseNudges(await ipc.prefsGet("ui.nudges")) });
  const repeat = useRepeatRemindPrefs();
  if (!data) return null;
  const save = async (patch: Partial<NudgeState>) => {
    const next = { ...data, ...patch };
    qc.setQueryData(KEY, next);
    await ipc.prefsSet("ui.nudges", next);
  };
  return (
    <Section title="Pet notifications">
      <Row label="Nudges a day" hint="Greetings, the routine reminder and other friendly nudges. Each kind at most once a day. Reminders you set yourself don't count.">
        <select aria-label="Nudges a day" className={select} value={data.maxPerDay} onChange={(e) => void save({ maxPerDay: Number(e.target.value) })}>
          {Array.from({ length: MAX_PER_DAY_RANGE[1] }, (_, i) => i + 1).map((n) => <option key={n} value={n}>{n}</option>)}
        </select>
      </Row>
      <Row label="Time between nudges">
        <select aria-label="Time between nudges" className={select} value={data.gapMin} onChange={(e) => void save({ gapMin: Number(e.target.value) })}>
          {GAP_CHOICES.map((m) => <option key={m} value={m}>{gapLabel(m)}</option>)}
        </select>
      </Row>
      <Row label="Quiet hours" hint="No nudges during these hours; they wait or are skipped.">
        <div className="flex items-center gap-2">
          {data.quiet.on && (
            <>
              <input type="time" aria-label="Quiet from" className={select} value={data.quiet.from} onChange={(e) => e.target.value && void save({ quiet: { ...data.quiet, from: e.target.value } })} />
              <span className="text-xs text-muted-foreground">to</span>
              <input type="time" aria-label="Quiet until" className={select} value={data.quiet.to} onChange={(e) => e.target.value && void save({ quiet: { ...data.quiet, to: e.target.value } })} />
            </>
          )}
          <Switch checked={data.quiet.on} onCheckedChange={(on) => void save({ quiet: { ...data.quiet, on } })} aria-label="Quiet hours" />
        </div>
      </Row>
      <WellbeingRows />
      <Row label="Repeating tasks still open" hint="Once in the evening the pet names the repeating tasks you haven't ticked today. Counts as one nudge.">
        <div className="flex items-center gap-2">
          {repeat.state.on && (
            <input type="time" aria-label="Repeating tasks reminder time" className={select} value={repeat.state.at}
              onChange={(e) => e.target.value && void repeat.save({ ...repeat.state, at: e.target.value, remindedOn: null })} />
          )}
          <Switch checked={repeat.state.on} onCheckedChange={(on) => void repeat.save({ ...repeat.state, on })} aria-label="Repeating tasks reminder" />
        </div>
      </Row>
    </Section>
  );
}

const WELLBEING_LABEL: Record<WellbeingKind, { label: string; hint: string }> = {
  break: { label: "Stretch break", hint: "The pet suggests a short break. Counted from your time at the screen when app tracking is on, otherwise from the clock." },
  water: { label: "Drink of water", hint: "A friendly reminder to drink something." },
};

/** Break and water nudges (owner 2026-10-27). They repeat through the day, so they do not use up
 * the budget above; they do keep quiet hours. */
function WellbeingRows() {
  const { state, save, loaded } = useWellbeingPrefs();
  if (!loaded || !state) return null;
  return (
    <>
      {KINDS.map((kind) => (
        <Row key={kind} label={WELLBEING_LABEL[kind].label} hint={WELLBEING_LABEL[kind].hint}>
          {state[kind].on && (
            <select
              aria-label={`${WELLBEING_LABEL[kind].label} every`} className={select} value={state[kind].everyMin}
              onChange={(e) => void save({ ...state, [kind]: { ...state[kind], everyMin: Number(e.target.value), lastAt: Date.now() } })}
            >
              {EVERY_CHOICES.map((m) => <option key={m} value={m}>every {gapLabel(m)}</option>)}
            </select>
          )}
          <Switch
            checked={state[kind].on} aria-label={WELLBEING_LABEL[kind].label}
            onCheckedChange={(on) => void save({ ...state, [kind]: { ...state[kind], on, lastAt: on ? Date.now() : null, lastScreenSec: null } })}
          />
        </Row>
      ))}
    </>
  );
}
