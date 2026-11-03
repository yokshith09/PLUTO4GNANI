import type { ReactNode } from "react";
import { useTileHandle } from "./TileHandle";

export function PageHeader({ title, description, actions }: { title: string; description?: string; actions?: ReactNode }) {
  return (
    <div className="mb-5 flex flex-wrap items-end gap-x-4 gap-y-3">
      <div className="min-w-0">
        <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
        {description && <p className="mt-0.5 text-[13px] text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="ml-auto flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

export function Panel({ className = "", children }: { className?: string; children: ReactNode }) {
  return <section className={`rounded-xl border bg-card ${className}`}>{children}</section>;
}

/** A dashboard tile: header strip, scrollable body, optional footer. One structure everywhere. */
export function Tile({
  title, count, action, tabs, footer, className = "", bodyClassName = "", children,
}: {
  title?: string | undefined; count?: ReactNode; action?: ReactNode; tabs?: ReactNode; footer?: ReactNode;
  className?: string | undefined; bodyClassName?: string | undefined; children: ReactNode;
}) {
  const handle = useTileHandle();
  return (
    <section aria-label={title} className={`flex min-h-0 flex-col overflow-hidden rounded-xl border bg-card ${className}`}>
      <header className="flex h-12 shrink-0 items-center gap-2 border-b px-5">
        {tabs ?? <h2 className="text-[13px] font-semibold">{title}</h2>}
        {count !== undefined && <span className="rounded-md bg-muted px-1.5 text-[11px] font-medium tabular-nums leading-5 text-muted-foreground">{count}</span>}
        {action && <div className="ml-auto flex items-center gap-1">{action}</div>}
        {handle && <div className={action ? "" : "ml-auto"}>{handle}</div>}
      </header>
      <div className={`min-h-0 flex-1 overflow-y-auto overflow-x-hidden p-4 ${bodyClassName}`}>{children}</div>
      {footer && <footer className="shrink-0 border-t px-4 py-3">{footer}</footer>}
    </section>
  );
}
