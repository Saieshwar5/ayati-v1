import { useCallback } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { request, type Item } from './model';

type MeaningInfo = {
  on: boolean;
  from_folder_above: boolean;
  available: boolean;
  can_change: boolean;
  proposals: { agent: string; reason: string }[];
};

/** A folder's meaning search. It is off until the folder's manager turns it on, because
 * the folder's text is then sent to a model provider; agents may propose it. */
export function MeaningControl({ item }: { item: Item }) {
  const load = useCallback(() => request<MeaningInfo>(`/${item.id}/meaning`), [item.id]);
  const info = useResource(load);
  const mutation = useMutation();
  const data = info.data;
  if (!data) return null;
  const set = (on: boolean) =>
    void mutation.run(async () => {
      if (
        on &&
        !confirm(
          `Turn on meaning search for "${item.title}"? The text of everything in this folder is sent to the model provider so agents can find it by meaning. Search by words stays on your server.`,
        )
      )
        return;
      await request(`/${item.id}/meaning`, { on });
      await info.refresh();
    });
  const covered = data.on || data.from_folder_above;
  if (!data.can_change && !covered) return null;
  return (
    <div className="library-keeping">
      <p className="library-muted">
        {data.on
          ? 'Meaning search is on: agents also find what is here by meaning, and its text is sent to the model provider.'
          : data.from_folder_above
            ? 'Meaning search is on for a folder above this one.'
            : 'Agents find what is here by its words. Meaning search is off.'}
        {data.can_change && !data.available && !covered
          ? ' No meaning-search model is set up on this Cuckoo.'
          : ''}
      </p>
      {data.proposals.map((proposal) => (
        <p className="library-notice" role="status" key={proposal.agent}>
          {proposal.agent} suggests turning on meaning search: {proposal.reason}
        </p>
      ))}
      {data.can_change && !data.from_folder_above && (
        <div className="apps-row">
          <Button disabled={mutation.busy} onClick={() => set(!data.on)}>
            {data.on ? 'Turn off meaning search' : 'Turn on meaning search'}
          </Button>
          {!data.on && data.proposals.length > 0 && (
            <Button disabled={mutation.busy} onClick={() => set(false)}>
              Dismiss
            </Button>
          )}
        </div>
      )}
      {(info.error || mutation.status) && (
        <p className="library-notice" role="alert">
          {info.error || mutation.status}
        </p>
      )}
    </div>
  );
}
