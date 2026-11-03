import { Button } from "@/components/ui/button";
import { Section } from "@/features/settings/parts";
import { cn } from "@/lib/utils";
import { LEARN_PREF, parseLearn } from "./learn";
import { ROLES, ROLE_INFO, ROLE_PREF, STUDENT_PACK_FEATURES, STUDENT_PACK_PREF, parseRole, parseStudentPack, studentPackOn, type Role } from "./roles";
import { TOUR_PREF, parseTour } from "./tourSteps";
import { usePref } from "./usePref";

/** Settings → Your profile: who you are (it only changes order and emphasis), and the tour and quiz again. */
export function YourProfile() {
  const role = usePref(ROLE_PREF, parseRole);
  const tour = usePref(TOUR_PREF, parseTour);
  const learn = usePref(LEARN_PREF, parseLearn);
  const pack = usePref(STUDENT_PACK_PREF, parseStudentPack);
  const current: Role = role.value ?? "general";
  const packOn = studentPackOn(current, pack.value);

  return (
    <Section title="Your profile">
      <div className="space-y-4 py-4">
        <div role="radiogroup" aria-label="What describes you best" className="grid gap-2 sm:grid-cols-2">
          {ROLES.map((r) => (
            <button
              key={r} type="button" role="radio" aria-checked={current === r} onClick={() => void role.set(r)}
              className={cn("rounded-lg border p-3 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring", current === r ? "border-primary bg-primary/10" : "hover:border-primary/50")}
            >
              <span className="block text-[13px] font-medium">{ROLE_INFO[r].label}</span>
              <span className="block text-xs text-muted-foreground">{ROLE_INFO[r].blurb}</span>
            </button>
          ))}
        </div>
        <p className="text-xs text-muted-foreground">This only changes what the tour and the quiz show first. It only sets the student pack default below and never turns on tracking.</p>

        <label className="flex items-start gap-3 rounded-lg border p-3 text-[13px]">
          <input type="checkbox" className="mt-0.5 size-4" checked={packOn} onChange={(e) => void pack.set(e.target.checked)} />
          <span>
            <span className="block font-medium">Student pack</span>
            <span className="block text-xs text-muted-foreground">{STUDENT_PACK_FEATURES.join(", ")}. On by default for students, off for everyone else. Every other feature is on for everyone.</span>
          </span>
        </label>

        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" onClick={() => void tour.set("pending")}>Replay the tour</Button>
          <Button variant="outline" size="sm" onClick={() => void learn.set({ answers: {}, dismissed: false })}>Restart the quiz</Button>
        </div>
      </div>
    </Section>
  );
}
