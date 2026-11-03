import { createContext, useContext } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { notify } from "@/lib/notify";
import { errorMessage, ipc } from "@/ipc";
import { cn } from "@/lib/utils";

/** The source line of the list item being drawn: react-markdown puts it on the item, not on its box. */
const ItemLine = createContext<number | undefined>(undefined);

function Box({ checked, onToggle }: { checked: boolean; onToggle: ((line: number) => void) | undefined }) {
  const line = useContext(ItemLine);
  return (
    <input
      type="checkbox" checked={checked} aria-label="Done"
      disabled={!onToggle || line === undefined} readOnly={!onToggle}
      onChange={() => line !== undefined && onToggle?.(line)}
      className="size-3.5 translate-y-0.5 accent-[var(--primary)]"
    />
  );
}

const open = async (href: string) => {
  try {
    await ipc.openUrl(href);
  } catch (e) {
    notify.error(errorMessage(e));
  }
};

/**
 * A note's Markdown (R1-11). Headings, lists, task-list boxes (clickable when `onToggle` is given, which receives the source line), code,
 * links, bold and italic. Raw HTML in the text is shown as text, never rendered. Links open in
 * the default browser through the app's own `open_url` (http, https and mailto only).
 */
export function MarkdownView({ source, className, onToggle }: { source: string; className?: string; onToggle?: (line: number) => void }) {
  if (!source.trim()) return <p className={cn("text-[15px] italic text-muted-foreground", className)}>Nothing to preview yet.</p>;
  return (
    <div className={cn("space-y-3 text-[15px] leading-relaxed", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: ({ children }) => <h1 className="mt-4 text-xl font-semibold tracking-tight first:mt-0">{children}</h1>,
          h2: ({ children }) => <h2 className="mt-4 text-lg font-semibold tracking-tight first:mt-0">{children}</h2>,
          h3: ({ children }) => <h3 className="mt-3 text-base font-semibold first:mt-0">{children}</h3>,
          h4: ({ children }) => <h4 className="mt-3 text-[15px] font-semibold first:mt-0">{children}</h4>,
          p: ({ children }) => <p>{children}</p>,
          ul: ({ children, className: c }) => <ul className={cn("list-disc space-y-1 pl-5", c?.includes("contains-task-list") && "list-none pl-0")}>{children}</ul>,
          ol: ({ children }) => <ol className="list-decimal space-y-1 pl-5">{children}</ol>,
          li: ({ children, className: c, node }) => (
            <ItemLine.Provider value={node?.position?.start.line}>
              <li className={cn(c?.includes("task-list-item") && "flex items-baseline gap-2")}>{children}</li>
            </ItemLine.Provider>
          ),
          input: ({ checked }) => <Box checked={!!checked} onToggle={onToggle} />,
          blockquote: ({ children }) => <blockquote className="border-l-2 border-primary/50 pl-3 text-muted-foreground">{children}</blockquote>,
          hr: () => <hr className="border-border" />,
          pre: ({ children }) => <pre className="overflow-x-auto rounded-md bg-muted/70 p-3 text-[13px] leading-snug">{children}</pre>,
          code: ({ className: c, children }) => (
            <code className={cn(c ? "" : "rounded bg-muted/70 px-1 py-0.5 text-[0.9em]", "font-mono")}>{children}</code>
          ),
          table: ({ children }) => <div className="overflow-x-auto"><table className="w-full border-collapse text-[13px]">{children}</table></div>,
          th: ({ children }) => <th className="border-b px-2 py-1 text-left font-semibold">{children}</th>,
          td: ({ children }) => <td className="border-b px-2 py-1">{children}</td>,
          a: ({ href, children }) => (
            <a
              href={href} rel="noreferrer"
              onClick={(e) => { e.preventDefault(); if (href) void open(href); }}
              className="text-primary underline underline-offset-2 hover:opacity-80"
            >
              {children}
            </a>
          ),
        }}
      >
        {source}
      </ReactMarkdown>
    </div>
  );
}
