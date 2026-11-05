import type { RuleField, TableChoice, TableLimit } from './company-rules-model';

/** A rule's limit from a Library table (LB4): the number in one column of the row
 * whose key equals one of the request's fields. */
export function TableLimitPicker({ tables, fields, limit, change }: { tables: TableChoice[]; fields: RuleField[]; limit: TableLimit | null; change: (limit: TableLimit | null) => void }) {
  const usable = tables.filter(t => t.columns.length);
  const matches = fields.filter(f => f.type === 'string');
  if (!usable.length || !matches.length) return null;
  const table = usable.find(t => t.id === limit?.table) ?? usable[0];
  return <>
    <label>Compare with<select value={limit ? 'table' : 'number'} onChange={event => change(event.target.value === 'table'
      ? { table: table.id, title: table.title, column: table.columns[0], match: matches[0].name } : null)}>
      <option value="number">A number</option><option value="table">A Library table's number</option></select></label>
    {limit && <>
      <label>Table<select value={table.id} onChange={event => { const next = usable.find(t => t.id === event.target.value)!; change({ ...limit, table: next.id, title: next.title, column: next.columns[0] }); }}>
        {usable.map(t => <option key={t.id} value={t.id}>{t.title}</option>)}</select></label>
      <label>Column<select value={limit.column} onChange={event => change({ ...limit, column: event.target.value })}>
        {table.columns.map(c => <option key={c} value={c}>{c}</option>)}</select></label>
      <label>Row named by<select value={limit.match} onChange={event => change({ ...limit, match: event.target.value })}>
        {matches.map(f => <option key={f.name} value={f.name}>{f.name.replaceAll('_', ' ')}</option>)}</select></label>
    </>}
  </>;
}
