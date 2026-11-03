import type { ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { Panel } from "./PageHeader";

export function ComingSoon({ icon, title, description }: { icon: ReactNode; title: string; description: string }) {
  return (
    <div className="mx-auto max-w-xl pt-10">
      <Panel className="flex flex-col items-center px-8 py-12 text-center">
        <span className="mb-4 grid size-11 place-items-center rounded-xl bg-muted text-muted-foreground [&_svg]:size-5">{icon}</span>
        <div className="mb-2 flex items-center gap-2">
          <h1 className="text-lg font-semibold tracking-tight">{title}</h1>
          <Badge variant="violet">Planned</Badge>
        </div>
        <p className="max-w-sm text-[13px] leading-relaxed text-muted-foreground">{description}</p>
      </Panel>
    </div>
  );
}
