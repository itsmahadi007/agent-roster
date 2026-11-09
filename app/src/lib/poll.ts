/** Calls `fn` now and then every `ms`, never while a previous call is still running. */
export function startPoll(fn: () => Promise<void>, ms: number): () => void {
  let busy = false;
  const tick = () => {
    if (busy) return;
    busy = true;
    fn()
      .catch(() => {})
      .finally(() => {
        busy = false;
      });
  };
  tick();
  const id = setInterval(tick, ms);
  return () => clearInterval(id);
}
