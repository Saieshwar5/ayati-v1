import { compare, type Week } from './improving-model';

const rows: [keyof Week, string, boolean, boolean][] = [
  // measure, label, lower is better, shown per 100 answers
  ['corrected', 'Answers corrected, per 100', true, true],
  ['accepted', 'Answers accepted, per 100', false, true],
  ['followed_up', 'Answers followed up, per 100', true, true],
  ['missed', 'Memory searches that found nothing', true, false],
  ['learned', 'Notes learned', false, false],
  ['held', 'Notes held after a correction', true, false],
  ['skills_approved', 'Skills approved', false, false],
];
const words = { better: 'better', worse: 'worse', same: 'no change', none: '' };

/** This week's measures beside last week's, with whether each got better as a word, never colour alone. */
export function WeekTable({ now, before, caption }: { now: Week; before: Week; caption: string }) {
  return <table className="improving-week"><caption>{caption}</caption>
    <thead><tr><th scope="col">This week</th><th scope="col">Now</th><th scope="col">Last week</th></tr></thead>
    <tbody>
      <tr><th scope="row">Answers</th><td>{now.answers}</td><td>{before.answers}</td></tr>
      {rows.map(([key, label, lower]) => {
        const a = now[key] as number | null, b = before[key] as number | null;
        const change = compare(a, b, lower);
        return <tr key={key}><th scope="row">{label}</th>
          <td className={change === 'better' || change === 'worse' ? `improving-${change}` : undefined}>{a ?? '–'}{change !== 'none' && change !== 'same' && <small> {words[change]}</small>}</td>
          <td>{b ?? '–'}</td></tr>;
      })}
    </tbody></table>;
}
