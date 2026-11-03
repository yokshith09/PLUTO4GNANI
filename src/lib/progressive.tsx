import { useCallback, useEffect, useRef, useState } from "react";

/** How many rows a long list draws at first, and how many more each step adds. */
export const PAGE = 100;

/**
 * Draw a long list in steps instead of all at once. With thousands of rows the first screen used
 * to take over a second (measured: 1,000 tasks = 1.4 s, 1,000 notes = 1.5 s, in a browser), almost
 * all of it drawing rows nobody could see. Every row stays reachable: scrolling near the end
 * adds the next step by itself, and a button does the same for keyboards and screen readers.
 * Counts shown elsewhere still say the true total.
 */
export function useProgressive<T>(items: readonly T[], step = PAGE) {
  const [limit, setLimit] = useState(step);
  const visible = items.length > limit ? items.slice(0, limit) : items;
  const more = items.length - visible.length;
  const showMore = useCallback(() => setLimit((l) => l + step), [step]);
  return { visible, more, showMore };
}

/** The "keep going" control under a progressively drawn list. Renders nothing when all is shown. */
export function ShowMore({ more, onMore, noun }: { more: number; onMore: () => void; noun: string }) {
  const button = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const el = button.current;
    if (!el || typeof IntersectionObserver === "undefined") return;
    // Fires straight away if the button is already near the screen, so a tall window fills up.
    const watcher = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) onMore();
    }, { rootMargin: "800px 0px" });
    watcher.observe(el);
    return () => watcher.disconnect();
  }, [more, onMore]);
  if (more <= 0) return null;
  return (
    <button
      ref={button} type="button" onClick={onMore}
      className="mx-auto mt-3 block rounded-md px-3 py-1.5 text-xs text-muted-foreground outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring"
    >
      Show more {noun} ({more} more)
    </button>
  );
}
