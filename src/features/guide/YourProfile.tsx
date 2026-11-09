import { Button } from "@/components/ui/button";
import { Section } from "@/features/settings/parts";
import { cn } from "@/lib/utils";
import { LEARN_PREF, parseLearn } from "./learn";
import { ROLES, ROLE_INFO, ROLE_PREF, parseRole, type Role } from "./roles";
import { TOUR_PREF, parseTour } from "./tourSteps";
import { usePref } from "./usePref";

/** Settings → Your profile: who you are (it only changes order and emphasis), and the tour and quiz again. */
export function YourProfile() {
  const role = usePref(ROLE_PREF, parseRole);
  const tour = usePref(TOUR_PREF, parseTour);
  const learn = usePref(LEARN_PREF, parseLearn);
  const current: Role = role.value ?? "general";

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
        <p className="text-xs text-muted-foreground">This only changes what the tour and the quiz show first. It never turns on tracking.</p>

        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" onClick={() => void tour.set("pending")}>Replay the tour</Button>
          <Button variant="outline" size="sm" onClick={() => void learn.set({ answers: {}, dismissed: false })}>Restart the quiz</Button>
        </div>
      </div>
    </Section>
  );
}
