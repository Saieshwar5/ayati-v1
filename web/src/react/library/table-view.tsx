import { useRef, useState } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation } from '../requests';
import type { Item } from './model';

export type Column = { name: string; type: 'text' | 'number' };
export type Table = { type: 'table'; columns: Column[]; rows: (string | number | null)[][] };
const PAGE = 100;

/** A table from pasted CSV or tab-separated text: the first line names the columns;
 * a column is numbers when every value in it is a number. */
export function parseTable(text: string): Table {
  const lines = text.split(/\r?\n/).filter((line) => line.trim());
  const split = (line: string) =>
    (line.includes('\t') ? line.split('\t') : line.split(',')).map((cell) => cell.trim());
  const [head = [], ...body] = lines.map(split);
  const numeric = head.map(
    (_, i) => i > 0 && body.every((row) => !row[i] || !Number.isNaN(Number(row[i]))),
  );
  return {
    type: 'table',
    columns: head.map((name, i) => ({
      name: name || `Column ${i + 1}`,
      type: numeric[i] ? 'number' : 'text',
    })),
    rows: body.map((row) =>
      head.map((_, i) => (!row[i] ? null : numeric[i] ? Number(row[i]) : row[i])),
    ),
  };
}

/** A Library table (LB4): a key column and more, read by everyone with access and edited
 * by editors as a whole, saved as a new version from the revision they read. */
export function TableView({ item, saved }: { item: Item; saved: () => void }) {
  const original = item.document as unknown as Table;
  const [draft, setDraft] = useState<Table>(original);
  const [filter, setFilter] = useState('');
  const [page, setPage] = useState(0);
  const [pasting, setPasting] = useState('');
  const mutation = useMutation();
  const attempt = useRef<{ key: string; request: string }>(undefined);
  const changed = JSON.stringify(draft) !== JSON.stringify(original);
  const shown = draft.rows
    .map((row, index) => ({ row, index }))
    .filter(
      ({ row }) =>
        !filter.trim() ||
        row.some((cell) =>
          String(cell ?? '')
            .toLowerCase()
            .includes(filter.trim().toLowerCase()),
        ),
    );
  const visible = shown.slice(page * PAGE, page * PAGE + PAGE);
  const setCell = (index: number, column: number, value: string) =>
    setDraft((old) => ({
      ...old,
      rows: old.rows.map((row, i) =>
        i !== index
          ? row
          : row.map((cell, c) =>
              c !== column
                ? cell
                : value === ''
                  ? null
                  : old.columns[c].type === 'number'
                    ? Number(value)
                    : value,
            ),
      ),
    }));
  const save = () =>
    void mutation.run(async () => {
      const body = { id: item.id, revision: item.revision, title: item.title, table: draft };
      const key = JSON.stringify(body);
      if (attempt.current?.key !== key) attempt.current = { key, request: crypto.randomUUID() };
      await identityRequest(`/workspaces/${workspaceId}/library-tables`, {
        ...body,
        request: attempt.current.request,
      });
      mutation.setStatus('Saved as a new version.');
      saved();
    });
  return (
    <div className="library-table-view">
      <h1>{item.title}</h1>
      <div className="library-table-tools">
        <input
          type="search"
          aria-label="Filter rows"
          placeholder="Filter rows"
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setPage(0);
          }}
        />
        <span className="library-muted">
          Revision {item.revision} · {draft.rows.length} row{draft.rows.length === 1 ? '' : 's'} · the
          first column is the key agents look up
        </span>
      </div>
      <div className="library-grid-scroll">
        <table className="library-grid">
          <thead>
            <tr>
              {draft.columns.map((column, c) => (
                <th key={c} scope="col">
                  {item.editable ? (
                    <input
                      aria-label={`Column ${c + 1} name`}
                      value={column.name}
                      onChange={(e) =>
                        setDraft((old) => ({
                          ...old,
                          columns: old.columns.map((x, i) =>
                            i === c ? { ...x, name: e.target.value } : x,
                          ),
                        }))
                      }
                    />
                  ) : (
                    column.name
                  )}
                  <small>{c === 0 ? 'Key' : column.type === 'number' ? 'Number' : 'Text'}</small>
                </th>
              ))}
              {item.editable && (
                <th scope="col">
                  <span className="visually-hidden">Row actions</span>
                </th>
              )}
            </tr>
          </thead>
          <tbody>
            {visible.map(({ row, index }) => (
              <tr key={index}>
                {row.map((cell, c) => (
                  <td key={c}>
                    {item.editable ? (
                      <input
                        aria-label={`${draft.columns[c].name}, row ${index + 1}`}
                        type={draft.columns[c].type === 'number' ? 'number' : 'text'}
                        value={cell ?? ''}
                        onChange={(e) => setCell(index, c, e.target.value)}
                      />
                    ) : (
                      (cell ?? '')
                    )}
                  </td>
                ))}
                {item.editable && (
                  <td>
                    <Button
                      aria-label={`Delete row ${index + 1}`}
                      onClick={() =>
                        setDraft((old) => ({
                          ...old,
                          rows: old.rows.filter((_, i) => i !== index),
                        }))
                      }
                    >
                      ×
                    </Button>
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {shown.length > PAGE && (
        <div className="library-pagination">
          <Button disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
            Previous
          </Button>
          <span>
            Rows {page * PAGE + 1}–{Math.min(shown.length, page * PAGE + PAGE)} of {shown.length}
          </span>
          <Button
            disabled={(page + 1) * PAGE >= shown.length}
            onClick={() => setPage((p) => p + 1)}
          >
            Next
          </Button>
        </div>
      )}
      {item.editable && (
        <>
          <div className="apps-row library-table-actions">
            <Button
              onClick={() =>
                setDraft((old) => ({ ...old, rows: [...old.rows, old.columns.map(() => null)] }))
              }
            >
              Add row
            </Button>
            <Button
              disabled={draft.columns.length >= 20}
              onClick={() =>
                setDraft((old) => ({
                  ...old,
                  columns: [
                    ...old.columns,
                    { name: `Column ${old.columns.length + 1}`, type: 'text' },
                  ],
                  rows: old.rows.map((row) => [...row, null]),
                }))
              }
            >
              Add text column
            </Button>
            <Button
              disabled={draft.columns.length >= 20}
              onClick={() =>
                setDraft((old) => ({
                  ...old,
                  columns: [
                    ...old.columns,
                    { name: `Column ${old.columns.length + 1}`, type: 'number' },
                  ],
                  rows: old.rows.map((row) => [...row, null]),
                }))
              }
            >
              Add number column
            </Button>
          </div>
          <details className="library-table-paste">
            <summary>Replace from pasted rows</summary>
            <textarea
              aria-label="Pasted rows"
              rows={5}
              placeholder={'Product,Price\nStanding desk,1250'}
              value={pasting}
              onChange={(e) => setPasting(e.target.value)}
            />
            <Button
              disabled={!pasting.trim()}
              onClick={() => {
                setDraft(parseTable(pasting));
                setPasting('');
              }}
            >
              Use these rows
            </Button>
          </details>
          <div className="agent-save-row">
            <Button primary disabled={!changed || mutation.busy} onClick={save}>
              Save table
            </Button>
            {changed && (
              <Button disabled={mutation.busy} onClick={() => setDraft(original)}>
                Discard
              </Button>
            )}
            <p className="ui-status" role="status">
              {mutation.busy ? 'Saving…' : mutation.status}
            </p>
          </div>
        </>
      )}
    </div>
  );
}
