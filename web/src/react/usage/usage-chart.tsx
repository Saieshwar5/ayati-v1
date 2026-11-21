import { useState } from 'react';
import { dollars, type UsageRow } from './usage-model';

const DAY = /^\d{4}-\d{2}-\d{2}$/;
const iso = (date: Date) => date.toISOString().slice(0, 10);
const shortDay = (key: string) => DAY.test(key) ? new Date(`${key}T00:00:00Z`).toLocaleDateString(undefined, { day: 'numeric', month: 'short', timeZone: 'UTC' }) : key;

/** Every day of the period, oldest first, with nothing charged on the days the report leaves out. */
export function chartDays(rows: UsageRow[], days: number, today = new Date()): { key: string; charge: number }[] {
  const dated = rows.filter(row => DAY.test(row.key));
  if (dated.length !== rows.length) return rows.map(row => ({ key: row.key, charge: row.charge_usd }));
  const local = iso(new Date(today.getTime() - today.getTimezoneOffset() * 60000));
  const last = dated.reduce((latest, row) => row.key > latest ? row.key : latest, local);
  const end = new Date(`${last}T00:00:00Z`).getTime();
  const charged = new Map(dated.map(row => [row.key, row.charge_usd]));
  return Array.from({ length: days }, (_, index) => {
    const key = iso(new Date(end - (days - 1 - index) * 86400000));
    return { key, charge: charged.get(key) ?? 0 };
  });
}

/** Charges per day as one row of bars. Pointing at a day shows its date and amount;
 * the same numbers are in the table under "By day". */
export function UsageChart({ rows, days }: { rows: UsageRow[]; days: number }) {
  const [hover, setHover] = useState(-1);
  const list = chartDays(rows, days);
  const top = Math.max(...list.map(day => day.charge), 0);
  if (!list.length || top <= 0) return null;
  const peak = list.findIndex(day => day.charge === top);
  const shown = list[hover] ?? list[peak];
  return <figure className="usage-chart">
    <figcaption><span>Charges per day</span>
      <span className="usage-chart-readout">{hover >= 0 ? shortDay(shown.key) : `Highest day · ${shortDay(shown.key)}`} <strong>{dollars(shown.charge)}</strong></span></figcaption>
    <div className="usage-chart-bars" role="img" aria-label={`Charges per day over ${list.length} days. The highest was ${dollars(top)} on ${shortDay(list[peak].key)}.`}
      onMouseLeave={() => setHover(-1)}>
      {list.map((day, index) => <span key={day.key} className={index === hover ? 'on' : undefined} onMouseEnter={() => setHover(index)}>
        <i style={{ height: day.charge > 0 ? `${Math.max(3, day.charge / top * 100)}%` : 0 }} /></span>)}
    </div>
    <div className="usage-chart-axis" aria-hidden="true"><span>{shortDay(list[0].key)}</span><span>{shortDay(list[list.length - 1].key)}</span></div>
  </figure>;
}
