import { useRef } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation } from '../requests';
import type { Item } from './model';

const uploadBase = `/workspaces/${workspaceId}/library-uploads`;
type Attempt = { checksum: string; revision: number; id: string; request: string };

/** Replaces a file with a new version, from the revision this person is looking at. */
export function ReplaceFile({ item, changed }: { item: Item; changed: () => void }) {
  const mutation = useMutation();
  const picker = useRef<HTMLInputElement>(null);
  // Choosing the same file again after a failure is the same upload, not a second one.
  const attempt = useRef<Attempt | undefined>(undefined);
  const replace = (file: File) =>
    mutation.run(async () => {
      if (!file.size || file.size > 25 * 1024 * 1024)
        throw new Error('Choose a file between 1 byte and 25 MiB.');
      const digest = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
      const checksum = Array.from(new Uint8Array(digest), (b) =>
        b.toString(16).padStart(2, '0'),
      ).join('');
      const previous = attempt.current;
      const current =
        previous && previous.checksum === checksum && previous.revision === item.revision
          ? previous
          : {
              checksum,
              revision: item.revision,
              id: crypto.randomUUID(),
              request: crypto.randomUUID(),
            };
      attempt.current = current;
      await identityRequest(uploadBase, {
        id: current.id,
        request: current.request,
        title: file.name.slice(0, 180),
        size: file.size,
        checksum,
        replaces: { item: item.id, revision: item.revision },
      });
      const response = await fetch(`/api/identity${uploadBase}/${current.id}/bytes`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/octet-stream', 'X-Ayati-Request': 'identity' },
        body: file,
      });
      if (!response.ok) {
        const result = await response.json().catch(() => ({}));
        throw new Error(result.error ?? 'The new version was not confirmed. Choose the file again.');
      }
      attempt.current = undefined;
      changed();
    });
  return (
    <>
      <Button disabled={mutation.busy} onClick={() => picker.current?.click()}>
        {mutation.busy ? 'Uploading…' : 'Upload new version'}
      </Button>
      <input
        ref={picker}
        type="file"
        hidden
        aria-label="New version of this file"
        onChange={(event) => {
          const file = event.target.files?.[0];
          event.target.value = '';
          if (file) void replace(file);
        }}
      />
      {mutation.status && (
        <p className="library-notice" role="alert">
          {mutation.status}
        </p>
      )}
    </>
  );
}
