import { Button } from "@/components/ui/button";
import { Row, Section } from "@/features/settings/parts";
import { TOUR_PREF, parseTour } from "./tourSteps";
import { usePref } from "./usePref";
import { GUIDE } from "./content";

/** Settings → Guide: replay the tour, and short how-to answers, each open on demand. */
export function GuideSection() {
  const tour = usePref(TOUR_PREF, parseTour);
  return (
    <Section title="Guide">
      <Row label="Product tour" hint="A short walk through the pages. It plays once on first use.">
        <Button variant="outline" size="sm" onClick={() => void tour.set("pending")}>Replay the tour</Button>
      </Row>
      <div className="divide-y">
        {GUIDE.map((g) => (
          <details key={g.id} className="py-3 text-[13px]">
            <summary className="cursor-pointer font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring">{g.title}</summary>
            <p className="mt-2 text-muted-foreground">{g.body}</p>
          </details>
        ))}
      </div>
    </Section>
  );
}
