import { useCallback, useEffect, useState } from "react";

/** Load data on mount and whenever `deps` change; `reload` re-runs the loader. */
export function useLoad<T>(loader: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const run = useCallback(loader, deps);
  const reload = useCallback(() => {
    setError(null);
    run().then(setData).catch((e) => setError(String(e)));
  }, [run]);
  useEffect(reload, [reload]);
  return { data, error, reload };
}
