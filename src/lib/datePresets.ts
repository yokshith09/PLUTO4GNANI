import { ymd } from "./dates";

const add = (d: Date, days: number) => { const x = new Date(d.getFullYear(), d.getMonth(), d.getDate()); x.setDate(x.getDate() + days); return x; };

/** Day presets (FR-45): Today, Tomorrow, This weekend (Saturday; today if it's the weekend), Next week (Monday), In 3 days. */
export function dayPresets(now = new Date()): { label: string; value: string }[] {
  const dow = now.getDay(); // 0 Sun … 6 Sat
  const weekend = dow === 6 || dow === 0 ? now : add(now, 6 - dow);
  const nextMonday = add(now, ((8 - dow) % 7) || 7);
  const all = [
    { label: "Today", value: ymd(now) },
    { label: "Tomorrow", value: ymd(add(now, 1)) },
    { label: "This weekend", value: ymd(weekend) },
    { label: "Next week", value: ymd(nextMonday) },
    { label: "In 3 days", value: ymd(add(now, 3)) },
  ];
  // On a Saturday "This weekend" is today: show one button per day, never two lit at once.
  return all.filter((o, i) => all.findIndex((x) => x.value === o.value) === i);
}

/** `YYYY-MM-DDTHH:mm` for a datetime-local input. */
export const localInput = (d: Date) => `${ymd(d)}T${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;

/** Time presets for reminders: In 1 hour (to the next 5 min), This evening 18:00 (if still ahead), Tomorrow 9:00, Next Monday 9:00. */
export function timePresets(now = new Date()): { label: string; value: string }[] {
  const inHour = new Date(now.getTime() + 3_600_000);
  inHour.setMinutes(Math.ceil(inHour.getMinutes() / 5) * 5, 0, 0);
  const at = (d: Date, h: number) => { const x = new Date(d); x.setHours(h, 0, 0, 0); return x; };
  const out = [{ label: "In 1 hour", value: localInput(inHour) }];
  if (now.getHours() < 17) out.push({ label: "This evening", value: localInput(at(now, 18)) });
  out.push({ label: "Tomorrow 9:00", value: localInput(at(add(now, 1), 9)) });
  const dow = now.getDay();
  out.push({ label: "Monday 9:00", value: localInput(at(add(now, ((8 - dow) % 7) || 7), 9)) });
  return out;
}
