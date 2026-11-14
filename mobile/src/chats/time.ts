/** Times from the server are Unix seconds. */
export function shortTime(seconds: number, now = Date.now()): string {
  const date = new Date(seconds * 1000);
  const today = new Date(now);
  if (date.toDateString() === today.toDateString()) {
    return date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  }
  const days = Math.floor((today.getTime() - date.getTime()) / 86_400_000);
  if (days < 7) return date.toLocaleDateString(undefined, { weekday: 'short' });
  return date.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}
