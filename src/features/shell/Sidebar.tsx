import {
  Archive, BookHeart, CheckSquare, Clock3, FolderKanban, GraduationCap, LayoutDashboard, Moon, NotebookPen, PawPrint, Plug, Settings, Sun, Trash2, Users,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { useStudentPack } from "@/features/student/useStudent";
import { Bear } from "./Bear";

export type Page = "today" | "tasks" | "time" | "notes" | "projects" | "study" | "diary" | "meetings" | "archive" | "bin" | "character" | "mcp" | "settings";

interface Props {
  page: Page;
  counts: { today: number; notes: number; bin: number };
  onPage: (p: Page) => void;
  isDark: boolean;
  onToggleTheme: () => void;
}

function Item({
  active, onClick, icon, children, count, tour,
}: { active: boolean; onClick: () => void; icon: React.ReactNode; children: React.ReactNode; count?: number; tour?: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      data-tour={tour}
      title={typeof children === "string" ? children : undefined}
      className={cn(
        "group relative flex h-8 w-full items-center justify-center gap-2.5 rounded-md px-0 text-left text-[13px] font-medium lg:justify-start lg:px-2.5 outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-4 [&_svg]:shrink-0",
        active ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
      )}
    >
      {active && <span aria-hidden className="absolute -left-2 top-1.5 h-5 w-[3px] rounded-r bg-primary" />}
      {icon}
      {/* Narrow windows show icons only; the name stays for screen readers (owner 2026-10-25). */}
      <span className="sr-only truncate lg:not-sr-only">{children}</span>
      {count !== undefined && count > 0 && (
        <span className="ml-auto hidden text-[11px] tabular-nums text-muted-foreground lg:inline">{count}</span>
      )}
    </button>
  );
}

function Group({ title, children }: { title?: string; children: React.ReactNode }) {
  return (
    <nav aria-label={title} className="space-y-0.5">
      {title && <p className="mb-1 mt-4 hidden px-2.5 text-[11px] font-medium text-muted-foreground/80 lg:block">{title}</p>}
      {title && <div aria-hidden className="mx-2 my-3 border-t lg:hidden" />}
      {children}
    </nav>
  );
}

export function Sidebar({ page, counts, onPage, isDark, onToggleTheme }: Props) {
  const go = (p: Page) => () => onPage(p);
  const study = useStudentPack();
  return (
    <aside className="flex w-14 shrink-0 flex-col border-r bg-[var(--sidebar)] px-2 pb-3 pt-3.5 lg:w-56 lg:px-3">
      <div className="mb-3 flex items-center justify-center gap-2.5 lg:justify-start lg:px-1.5">
        <span className="grid size-7 place-items-center overflow-hidden rounded-md bg-primary/12 ring-1 ring-primary/25">
          <Bear className="mt-1 size-7" />
        </span>
        <span className="hidden text-sm font-semibold tracking-tight lg:inline">Pluto</span>
        <span className="ml-auto hidden rounded bg-muted px-1.5 py-px text-[10px] font-medium text-muted-foreground lg:inline">Local</span>
      </div>

      <Group>
        <Item active={page === "today"} onClick={go("today")} tour="nav-today" icon={<LayoutDashboard />} count={counts.today}>Today overview</Item>
        <Item active={page === "tasks"} onClick={go("tasks")} tour="nav-tasks" icon={<CheckSquare />}>Tasks</Item>
        <Item active={page === "time"} onClick={go("time")} tour="nav-time" icon={<Clock3 />}>Time</Item>
        <Item active={page === "notes"} onClick={go("notes")} tour="nav-notes" icon={<NotebookPen />} count={counts.notes}>Notes</Item>
        <Item active={page === "projects"} onClick={go("projects")} tour="nav-projects" icon={<FolderKanban />}>Projects</Item>
        {(study || page === "study") && <Item active={page === "study"} onClick={go("study")} icon={<GraduationCap />}>Study</Item>}
        <Item active={page === "diary"} onClick={go("diary")} icon={<BookHeart />}>Diary</Item>
        <Item active={page === "meetings"} onClick={go("meetings")} icon={<Users />}>Meetings</Item>
        <Item active={page === "character"} onClick={go("character")} tour="nav-character" icon={<PawPrint />}>Character</Item>
        <Item active={page === "mcp"} onClick={go("mcp")} icon={<Plug />}>MCP</Item>
      </Group>

      <Group title="Library">
        <Item active={page === "archive"} onClick={go("archive")} icon={<Archive />}>Archive</Item>
        <Item active={page === "bin"} onClick={go("bin")} icon={<Trash2 />} count={counts.bin}>Bin</Item>
      </Group>

      <div className="mt-auto space-y-0.5 border-t pt-3">
        <div className="flex flex-col items-center gap-1 lg:flex-row">
          <div className="min-w-0 flex-1"><Item active={page === "settings"} onClick={go("settings")} tour="nav-settings" icon={<Settings />}>Settings</Item></div>
          <button
            type="button" onClick={onToggleTheme} aria-label={isDark ? "Switch to light theme" : "Switch to dark theme"} title={isDark ? "Light theme" : "Dark theme"}
            className="grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground outline-none hover:bg-accent/60 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring [&_svg]:size-4"
          >
            {isDark ? <Sun /> : <Moon />}
          </button>
        </div>
        <p className="hidden px-2.5 pt-2 text-[11px] leading-4 text-muted-foreground/80 lg:block">Everything stays on this computer.</p>
      </div>
    </aside>
  );
}
