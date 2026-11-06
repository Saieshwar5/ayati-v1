import { useCallback, useRef, useState } from 'react';
import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { request as libraryRequest, type Listing } from '../library/model';
import type { Message } from './model';

/** Save an answer from Chat as a Library page (LB5), so agents given the folder answer
 * the question themselves next time. The text starts as the message and can be edited;
 * nothing is saved until the person chooses Save. */
export function SaveAnswer({ message, close }: { message: Message; close: () => void }) {
  const load = useCallback(() => libraryRequest<Listing>('?view=all&q=&offset=0'), []);
  const listing = useResource(load);
  const mutation = useMutation();
  const attempt = useRef({ id: crypto.randomUUID(), request: crypto.randomUUID() });
  const [title, setTitle] = useState('');
  const [text, setText] = useState(message.content);
  const [folder, setFolder] = useState('');
  const [saved, setSaved] = useState<string>();
  const folders = listing.data?.items.filter((i) => i.kind === 'folder' && i.editable) ?? [];
  const save = () =>
    void mutation.run(async () => {
      await identityRequest(`/workspaces/${workspaceId}/library-answers`, {
        ...attempt.current,
        folder: folder || null,
        title,
        text,
        message: message.id,
      });
      setSaved(attempt.current.id);
    });
  return (
    <Dialog
      title="Save to Library"
      close={close}
      busy={mutation.busy}
      status={mutation.status || listing.error}
      footer={
        saved ? (
          <>
            <a className="ui-button" href={workspaceUrl(`/?view=library&item=${saved}`)}>
              Open the page
            </a>
            <Button primary onClick={close}>
              Done
            </Button>
          </>
        ) : (
          <>
            <Button disabled={mutation.busy} onClick={close}>
              Cancel
            </Button>
            <Button
              primary
              disabled={mutation.busy || !title.trim() || !text.trim()}
              onClick={save}
            >
              Save
            </Button>
          </>
        )
      }
    >
      {saved ? (
        <p role="status">Saved. Agents given this folder can answer it from now on.</p>
      ) : (
        <>
          <p>
            Keep this answer as a Library page. Agents given the folder use it to answer the same
            question next time.
          </p>
          <label className="ui-field">
            Question or title
            <input
              aria-label="Question or title"
              maxLength={180}
              placeholder="Where do visitors park?"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </label>
          <label className="ui-field">
            Answer
            <textarea
              aria-label="Answer"
              rows={6}
              maxLength={60000}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </label>
          <label className="ui-field">
            Folder
            <select aria-label="Folder" value={folder} onChange={(e) => setFolder(e.target.value)}>
              <option value="">No folder (only you)</option>
              {folders.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.title}
                </option>
              ))}
            </select>
          </label>
        </>
      )}
    </Dialog>
  );
}
