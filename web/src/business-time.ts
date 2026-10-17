export type BusinessClock = { location: string; timezone: string; hour12: boolean };
export function localMinute(ms: number, timezone: string): string {
  const parts = new Intl.DateTimeFormat('en-CA', { timeZone: timezone, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).formatToParts(ms);
  const part = (name: string): string => parts.find(p => p.type === name)!.value;
  return `${part('year')}-${part('month')}-${part('day')}T${part('hour')}:${part('minute')}`;
}
// Return all matching instants so DST gaps and repeated hours are never silently guessed.
export function matchingInstants(local: string, timezone: string): number[] {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/.test(local)) return [];
  const anchor = Date.parse(`${local}Z`);
  if (!Number.isFinite(anchor)) return [];
  const offsets = new Set<number>();
  for (const hours of [-48, -24, 0, 24, 48]) {
    const sample = anchor + hours * 3600000;
    offsets.add(Date.parse(`${localMinute(sample, timezone)}Z`) - sample);
  }
  return [...offsets].map(offset => anchor - offset).filter(ms => localMinute(ms, timezone) === local).sort((a, b) => a - b);
}
