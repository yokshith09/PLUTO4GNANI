import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ipc } from "@/ipc";

/** One preference (`user_preferences`), read once, cached, and written through `set`. */
export function usePref<T>(key: string, parse: (raw: unknown) => T) {
  const qc = useQueryClient();
  const queryKey = ["prefs", key] as const;
  const query = useQuery({ queryKey, queryFn: async () => parse(await ipc.prefsGet<unknown>(key)) });
  const set = async (value: T) => {
    qc.setQueryData(queryKey, value);
    await ipc.prefsSet(key, value);
  };
  return { value: query.data, loading: query.isPending, set };
}
