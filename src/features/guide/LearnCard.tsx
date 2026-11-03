import { useState } from "react";
import { Check, GraduationCap, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Page } from "@/features/shell/Sidebar";
import { cn } from "@/lib/utils";
import { LEARN_PREF, parseLearn, questionsFor } from "./learn";
import { ROLE_PREF, parseRole } from "./roles";
import { TOUR_PREF, parseTour } from "./tourSteps";
import { usePref } from "./usePref";

function ShowMe({ target, onShow }: { target: { page: Page; label: string }; onShow: (page: Page) => void }) {
  return <Button size="sm" variant="outline" onClick={() => onShow(target.page)}>{target.label}</Button>;
}

/**
 * "Learn Pluto": five quick questions on Today, in the order that suits who you are. Each answer
 * says what the feature does and can take you there. Privacy tracking is in every set because it
 * is off by default and easy to miss. Dismiss any time; Settings → Your profile restarts it.
 */
export function LearnCard({ onShow }: { onShow: (page: Page) => void }) {
  const role = usePref(ROLE_PREF, parseRole);
  const learn = usePref(LEARN_PREF, parseLearn);
  const tour = usePref(TOUR_PREF, parseTour);
  const [picked, setPicked] = useState<number | null>(null);
  if (role.loading || learn.loading || tour.loading || !role.value || !learn.value) return null;
  if (tour.value === "pending" || learn.value.dismissed) return null;

  const state = learn.value;
  const questions = questionsFor(role.value);
  const next = questions.find((q) => state.answers[q.id] === undefined);
  const answered = questions.length - (next ? questions.filter((q) => state.answers[q.id] === undefined).length : 0);
  const save = (patch: Partial<typeof state>) => learn.set({ ...state, ...patch });

  const choose = (idx: number) => {
    if (!next || picked !== null) return;
    setPicked(idx);
  };
  const advance = () => {
    if (!next || picked === null) return;
    const answers = { ...state.answers, [next.id]: picked };
    setPicked(null);
    void learn.set({ ...state, answers });
  };

  return (
    <section aria-label="Learn Pluto" className="rounded-xl border bg-card p-4">
      <div className="flex items-center gap-2">
        <GraduationCap className="size-4 text-primary" aria-hidden />
        <h2 className="text-[13px] font-semibold">Learn Pluto</h2>
        <span className="text-xs text-muted-foreground">{next ? `Question ${answered + 1} of ${questions.length}` : "All done"}</span>
        <button type="button" aria-label="Hide Learn Pluto" onClick={() => void save({ dismissed: true })} className="ml-auto grid size-6 place-items-center rounded text-muted-foreground outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring"><X className="size-3.5" /></button>
      </div>

      {next ? (
        <div className="mt-3">
          <p id="learn-q" className="text-sm font-medium">{next.q}</p>
          <div role="group" aria-labelledby="learn-q" className="mt-2.5 grid gap-1.5">
            {next.options.map((opt, idx) => {
              const reveal = picked !== null;
              const right = idx === next.answer;
              return (
                <button
                  key={opt} type="button" disabled={reveal && idx !== picked && !right} aria-pressed={picked === idx} onClick={() => choose(idx)}
                  className={cn(
                    "flex items-center gap-2 rounded-lg border px-3 py-2 text-left text-[13px] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
                    !reveal && "hover:border-primary/60 hover:bg-accent/40",
                    reveal && right && "border-success/60 bg-success/10",
                    reveal && picked === idx && !right && "border-destructive/50 bg-destructive/5",
                  )}
                >
                  <span className="min-w-0 flex-1">{opt}</span>
                  {reveal && right && <Check className="size-4 text-success" aria-label="Correct answer" />}
                </button>
              );
            })}
          </div>
          {picked !== null && (
            <div className="mt-3" role="status">
              <p className="text-[13px] text-muted-foreground"><span className="font-medium text-foreground">{picked === next.answer ? "Yes. " : "Not quite. "}</span>{next.why}</p>
              <div className="mt-2.5 flex gap-2">
                {next.show && <ShowMe target={next.show} onShow={onShow} />}
                <Button size="sm" onClick={advance}>{answered + 1 === questions.length ? "Finish" : "Next question"}</Button>
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="mt-3 flex items-center gap-3">
          <p className="min-w-0 flex-1 text-[13px] text-muted-foreground">That is the main tour of Pluto. You answered {questions.filter((q) => state.answers[q.id] === q.answer).length} of {questions.length} the first time. Settings → Your profile has the tour and the quiz again.</p>
          <Button size="sm" onClick={() => void save({ dismissed: true })}>Close</Button>
        </div>
      )}
    </section>
  );
}
