import { useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { request, type Item } from './model';

/** One line saying what an item is, in the words people would look for. Agents and
 * people find the item by it, so no model has to read every document. */
export function DescriptionControl({ item, changed }: { item: Item; changed: () => void }) {
  const mutation = useMutation();
  const [draft, setDraft] = useState(item.description ?? '');
  if (!item.editable)
    return item.description ? <p className="library-muted">{item.description}</p> : null;
  const dirty = draft.trim() !== (item.description ?? '');
  return (
    <div className="library-keeping">
      <label className="library-field">
        What this is
        <input
          aria-label="What this is"
          maxLength={300}
          value={draft}
          placeholder="One line, in the words people would search for"
          onChange={(event) => setDraft(event.target.value)}
        />
      </label>
      {dirty && (
        <Button
          disabled={mutation.busy}
          onClick={() =>
            void mutation.run(async () => {
              await request(`/${item.id}/description`, { text: draft });
              changed();
            })
          }
        >
          Save description
        </Button>
      )}
      {mutation.status && (
        <p className="library-notice" role="alert">
          {mutation.status}
        </p>
      )}
    </div>
  );
}
