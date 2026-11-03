import { useQuery, useQueryClient } from "@tanstack/react-query";
import { notify } from "@/lib/notify";
import { errorMessage, ipc, type Shortcut } from "@/ipc";
import { parseLinks } from "@/lib/links";

const KEY = ["prefs", "ui.shortcuts"] as const;

/** Pasted links, kept in preferences and opened in the default browser. */
export function useShortcuts() {
  const qc = useQueryClient();
  const { data: shortcuts = [] } = useQuery({ queryKey: KEY, queryFn: async () => (await ipc.prefsGet<Shortcut[]>("ui.shortcuts")) ?? [] });
  const save = async (next: Shortcut[]) => {
    qc.setQueryData(KEY, next);
    await ipc.prefsSet("ui.shortcuts", next);
  };
  return {
    shortcuts,
    /** Add every link found in `text`. Returns how many were new. */
    add: async (text: string, name?: string) => {
      const found = parseLinks(text).filter((f) => !shortcuts.some((s) => s.url === f.url));
      if (found.length === 0) return 0;
      const made = found.map((f, i) => ({ id: `${Date.now()}-${i}`, url: f.url, label: found.length === 1 && name?.trim() ? name.trim() : f.label }));
      await save([...shortcuts, ...made].slice(0, 24));
      return made.length;
    },
    remove: (id: string) => save(shortcuts.filter((s) => s.id !== id)),
    /**
     * Rename a link and optionally change its address (FR-61). Returns false, and changes
     * nothing, when the address is not a link. An empty name falls back to the site name.
     */
    rename: async (id: string, name: string, url: string) => {
      const [link] = parseLinks(url.trim());
      if (!link) return false;
      await save(shortcuts.map((s) => (s.id === id ? { ...s, url: link.url, label: name.trim().slice(0, 40) || link.label } : s)));
      return true;
    },
    open: async (url: string) => {
      try {
        await ipc.openUrl(url);
      } catch (e) {
        notify.error(errorMessage(e));
      }
    },
  };
}
