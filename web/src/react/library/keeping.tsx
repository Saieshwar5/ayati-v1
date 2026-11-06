import { useCallback, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { dateLabel, request, type Item } from './model';

type KeepingInfo = {
  keep_days: number | null;
  folder_keep_days: number | null;
  deletes_at: number | null;
  hold: { reason: string; at: number; by: string; on_folder: boolean } | null;
  can_change: boolean;
};
const periods: [number, string][] = [
  [30, '30 days'],
  [90, '90 days'],
  [365, '1 year'],
  [730, '2 years'],
  [2555, '7 years'],
];

/** Keeping rules and legal holds (LB7). Everyone who can open an item sees when it will
 * be deleted and whether it is held; admins set a folder's keeping rule and put items
 * or folders on legal hold. */
export function KeepingControl({ item }: { item: Item }) {
  const load = useCallback(() => request<KeepingInfo>(`/${item.id}/keeping`), [item.id]);
  const info = useResource(load);
  const mutation = useMutation();
  const [holding, setHolding] = useState(false);
  const data = info.data;
  if (!data) return null;
  const folder = item.kind === 'folder';
  const run = (path: string, body: unknown) =>
    void mutation.run(async () => {
      await request(`/${item.id}/${path}`, body);
      await info.refresh();
    });
  return (
    <div className="library-keeping">
      {data.hold ? (
        <p className="library-notice" role="status">
          On legal hold{data.hold.on_folder ? ' with its folder' : ''} since{' '}
          {dateLabel(data.hold.at)}: {data.hold.reason}. It can't be trashed or deleted, and
          every version is kept.
        </p>
      ) : (
        !folder &&
        data.deletes_at && (
          <p className="library-muted">
            Deleted for good on {dateLabel(data.deletes_at)} under its folder's keeping rule.
          </p>
        )
      )}
      {data.can_change && (
        <div className="apps-row">
          {folder && (
            <label className="library-keep-select">
              Keep items
              <select
                aria-label="Keep items"
                value={data.keep_days ?? ''}
                disabled={mutation.busy}
                onChange={(e) =>
                  run('keeping', { keep_days: e.target.value ? Number(e.target.value) : null })
                }
              >
                <option value="">Forever</option>
                {periods.map(([days, label]) => (
                  <option key={days} value={days}>
                    {label}, then delete for good
                  </option>
                ))}
              </select>
            </label>
          )}
          {data.hold && !data.hold.on_folder ? (
            <Button disabled={mutation.busy} onClick={() => run('hold', { reason: null })}>
              Lift legal hold
            </Button>
          ) : (
            !data.hold && (
              <Button disabled={mutation.busy} onClick={() => setHolding(true)}>
                Legal hold…
              </Button>
            )
          )}
        </div>
      )}
      {(mutation.status || info.error) && <p role="alert">{mutation.status || info.error}</p>}
      {holding && (
        <HoldDialog
          title={item.title}
          close={() => setHolding(false)}
          hold={async (reason) => {
            await request(`/${item.id}/hold`, { reason });
            await info.refresh();
          }}
        />
      )}
    </div>
  );
}

function HoldDialog({
  title,
  close,
  hold,
}: {
  title: string;
  close: () => void;
  hold: (reason: string) => Promise<void>;
}) {
  const mutation = useMutation();
  const [reason, setReason] = useState('');
  return (
    <Dialog
      title={`Put ${title} on legal hold`}
      close={close}
      busy={mutation.busy}
      status={mutation.status}
      footer={
        <>
          <Button disabled={mutation.busy} onClick={close}>
            Cancel
          </Button>
          <Button
            primary
            disabled={mutation.busy || !reason.trim()}
            onClick={() =>
              void mutation.run(async () => {
                await hold(reason.trim());
                close();
              })
            }
          >
            Put on hold
          </Button>
        </>
      }
    >
      <p>
        Nobody can trash it, keeping rules skip it, and every version is kept until an admin
        lifts the hold.
      </p>
      <label className="ui-field">
        Why
        <input
          aria-label="Why"
          maxLength={300}
          placeholder="Supplier dispute"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
      </label>
    </Dialog>
  );
}
