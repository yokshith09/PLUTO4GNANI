import { Section } from "@/features/settings/parts";
import { useStudentPack } from "@/features/student/useStudent";
import { GUIDE } from "./content";

/** Settings → Guide: short how-to answers, each open on demand. */
export function GuideSection() {
  const student = useStudentPack();
  return (
    <Section title="Guide">
      <div className="divide-y">
        {GUIDE.filter((g) => student || !g.studentOnly).map((g) => (
          <details key={g.id} className="py-3 text-[13px]">
            <summary className="cursor-pointer font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring">{g.title}</summary>
            <p className="mt-2 text-muted-foreground">{g.body}</p>
          </details>
        ))}
      </div>
    </Section>
  );
}
