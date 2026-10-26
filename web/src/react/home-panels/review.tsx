import { useRef } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { keepPanel, type PersonalPanel } from './model';
import { PanelContent, SnapshotDetails } from './snapshot';
export function PanelReview({
  panel,
  close,
  refresh,
  readOnly = false,
}: {
  panel: PersonalPanel;
  close: () => void;
  refresh: () => Promise<void>;
  readOnly?: boolean;
}) {
  const m = useMutation();
  const request = useRef(crypto.randomUUID());
  const snapshot = readOnly ? (panel.active ?? panel.draft) : panel.draft;
  return (
    <Dialog title={readOnly ? 'Saved panel details' : 'Review Home panel'} close={close}>
      <p>
        {readOnly
          ? 'This is a saved view. Opening it does not refresh its sources.'
          : 'Only your Home changes. Keeping this snapshot grants no tool, source or recurring-update permission.'}
      </p>
      {snapshot?.available && snapshot.definition ? (
        <>
          <PanelContent definition={snapshot.definition} />
          <SnapshotDetails snapshot={snapshot} />
          {!readOnly && (
            <Button
              primary
              disabled={m.busy}
              onClick={() =>
                void m.run(async () => {
                  await keepPanel(panel.id, snapshot.revision, request.current);
                  await refresh();
                  close();
                })
              }
            >
              {m.busy ? 'Saving review…' : 'Keep on Home'}
            </Button>
          )}
        </>
      ) : (
        <p>Panel source unavailable. Refresh Home to check current access.</p>
      )}
      {m.status && <p role="alert">{m.status}</p>}
    </Dialog>
  );
}
