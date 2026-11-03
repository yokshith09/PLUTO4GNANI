import { useState } from "react";
import { Cake, MessageCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Panel } from "@/features/shell/PageHeader";
import { petSay } from "@/ipc";
import { ageDays, ageLabel, helloLine, type Companion } from "./companion";
import { helloFacts, useCompanion } from "./useCompanion";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/**
 * About you and your pet (FR-93/FR-94): your name and birthday (day and month, never the year),
 * the pet's age and birthday, and the once-a-day hello. Everything stays on this computer.
 */
export function CompanionPanel() {
  const { companion, save } = useCompanion();
  const [name, setName] = useState<string | null>(null);
  const [pet, setPet] = useState<string | null>(null);
  if (!companion) return null;
  const [month, day] = companion.userBirthday?.split("-") ?? ["", ""];
  const set = (patch: Partial<Companion>) => void save({ ...companion, ...patch });
  const setBirthday = (m: string, d: string) => set({ userBirthday: m && d ? `${m}-${d}` : null });
  const born = new Date(`${companion.born}T12:00:00`);
  const petName = companion.petName || "Pluto";
  const days = ageDays(companion, new Date());
  /** The hello again, as a text bubble above the pet; it doesn't use up today's one. */
  const sayHello = async () => {
    const now = new Date();
    await petSay({ text: helloLine(companion, now, await helloFacts(now)), pose: "wave", ms: 10_000 });
  };

  return (
    <section aria-label="About you and your pet" className="mt-6">
      <h2 className="text-[15px] font-semibold">About you and your pet</h2>
      <p className="mt-0.5 text-xs text-muted-foreground">So it can greet you by name and remember birthdays. Stays on this computer.</p>
      <Panel className="mt-3 grid gap-4 p-4 xl:grid-cols-2">
        <div className="space-y-3">
          <label className="block text-xs font-medium text-muted-foreground" htmlFor="pet-name">Pet’s name</label>
          <Input
            id="pet-name" maxLength={30} placeholder="Pluto"
            value={pet ?? companion.petName}
            onChange={(e) => setPet(e.target.value)}
            onBlur={() => { if (pet !== null) set({ petName: pet.trim() }); setPet(null); }}
          />
          <label className="block text-xs font-medium text-muted-foreground" htmlFor="you-name">Your first name</label>
          <Input
            id="you-name" maxLength={40} placeholder="Optional"
            value={name ?? companion.userName}
            onChange={(e) => setName(e.target.value)}
            onBlur={() => { if (name !== null) set({ userName: name.trim() }); setName(null); }}
          />
          <p className="text-xs font-medium text-muted-foreground">Your birthday</p>
          <div className="flex gap-2">
            <select aria-label="Birthday month" value={month} onChange={(e) => setBirthday(e.target.value, day || "01")}
              className="h-8 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring">
              <option value="">Month</option>
              {MONTHS.map((m, i) => <option key={m} value={String(i + 1).padStart(2, "0")}>{m}</option>)}
            </select>
            <select aria-label="Birthday day" value={day} onChange={(e) => setBirthday(month || "01", e.target.value)}
              className="h-8 rounded-md border border-input bg-card px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring">
              <option value="">Day</option>
              {Array.from({ length: 31 }, (_, i) => String(i + 1).padStart(2, "0")).map((d) => <option key={d} value={d}>{Number(d)}</option>)}
            </select>
          </div>
        </div>
        <div className="space-y-3">
          <div className="flex items-center gap-3 rounded-lg bg-muted/40 p-3">
            <Cake className="size-5 text-primary" />
            <div>
              <p className="text-sm font-semibold" aria-live="polite">{petName} is {ageLabel(companion, new Date())}</p>
              <p className="text-xs text-muted-foreground">{days} {days === 1 ? "day" : "days"} together</p>
              <p className="text-xs text-muted-foreground">Birthday: {born.toLocaleDateString(undefined, { day: "numeric", month: "long" })}, the day you met</p>
            </div>
          </div>
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="text-[13px] font-medium">Say hello once a day</p>
              <p className="text-xs text-muted-foreground">The first time you open your PC each day. Never more than once.</p>
            </div>
            <Switch checked={companion.greet} onCheckedChange={(v) => set({ greet: v })} aria-label="Say hello once a day" />
          </div>
          <Button size="sm" variant="outline" onClick={() => void sayHello()}><MessageCircle />Say hello now</Button>
        </div>
      </Panel>
    </section>
  );
}
