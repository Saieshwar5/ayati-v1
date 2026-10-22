import { lazy, Suspense, useCallback, useEffect, useState } from 'react';
import { navigate } from '../../navigation';
import { currentWorkspace } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { AskDialog, HistoryDialog, ShareDialog } from './dialogs';
import { PageEditor } from './editor';
import { usePageSave } from './autosave';
import {
  bytesLabel,
  dateLabel,
  downloadUrl,
  draftKey,
  itemUrl,
  request,
  visibilityName,
  type Item,
} from './model';
const PdfPreview = lazy(() =>
  import('./pdf-preview').then((module) => ({ default: module.PdfPreview })),
);
export function LibraryItem({
  id,
  sourceRevision,
  changed,
}: {
  id: string;
  sourceRevision?: string | null;
  changed: () => void;
}) {
  const [generation, setGeneration] = useState(0);
  const load = useCallback(async () => {
    try {
      return await request<Item>(`/${id}`);
    } catch (error) {
      if (
        error &&
        typeof error === 'object' &&
        'status' in error &&
        [400, 401, 403, 404].includes(Number(error.status))
      ) {
        try {
          localStorage.removeItem(draftKey(id));
        } catch {
          /* Optional. */
        }
      }
      throw error;
    }
  }, [id]);
  const resource = useResource(load);
  useEffect(() => {
    const refresh = () => void resource.refresh().catch(() => {});
    const timer = window.setInterval(refresh, 15000);
    window.addEventListener('focus', refresh);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('focus', refresh);
    };
  }, [resource.refresh]);
  const refresh = () => {
    void resource.refresh().catch(() => {});
    changed();
  };
  const reload = () => {
    void resource
      .refresh()
      .then(() => setGeneration((n) => n + 1))
      .catch(() => {});
    changed();
  };
  const item = resource.data;
  if (!item)
    return (
      <section className="library-detail">
        <Button onClick={() => navigate(itemUrl())}>‹ Library</Button>
        <p role={resource.error ? 'alert' : 'status'}>{resource.error || 'Opening item…'}</p>
        <Button onClick={() => void resource.refresh().catch(() => {})}>Retry</Button>
      </section>
    );
  return (
    <ReadyItem
      key={`${id}:${generation}`}
      item={item}
      sourceRevision={sourceRevision}
      refresh={refresh}
      reload={reload}
      changed={changed}
      error={resource.error}
      acknowledged={(saved) =>
        resource.setData((current) =>
          current && current.id === id && current.revision <= saved.revision
            ? { ...current, ...saved }
            : current,
        )
      }
    />
  );
}
function ReadyItem({
  item,
  sourceRevision,
  refresh,
  reload,
  changed,
  error,
  acknowledged,
}: {
  item: Item;
  sourceRevision?: string | null;
  refresh: () => void;
  reload: () => void;
  changed: () => void;
  error: string;
  acknowledged: (saved: Pick<Item, 'title' | 'document' | 'revision'>) => void;
}) {
  const id = item.id;
  const mutation = useMutation();
  const [dialog, setDialog] = useState<'share' | 'ask' | 'history'>();
  const saving = usePageSave(item, acknowledged, item.kind === 'page');
  const stale = item.kind === 'page' && item.revision !== saving.revision;
  return (
    <section className="library-detail">
      <header className="library-detail-header">
        <div>
          <Button onClick={() => navigate(itemUrl())}>‹ Library</Button>
          <span>
            {item.kind === 'page' ? 'Page' : 'File'} · {visibilityName(item.visibility)}
          </span>
        </div>
        <div>
          <Button
            disabled={mutation.busy}
            aria-label={item.starred ? 'Remove star' : 'Star item'}
            onClick={() =>
              void mutation.run(async () => {
                await request(`/${id}/action`, {
                  request: crypto.randomUUID(),
                  revision: item.revision,
                  action: item.starred ? 'unstar' : 'star',
                });
                refresh();
              })
            }
          >
            {item.starred ? '★' : '☆'}
          </Button>
          <Button
            disabled={
              saving.dirty || saving.busy || stale || !['ready', 'partial'].includes(item.readiness)
            }
            title={
              saving.dirty || saving.busy || stale
                ? 'Save or reload the current page before asking'
                : undefined
            }
            onClick={() => setDialog('ask')}
          >
            Ask an agent
          </Button>
          {item.steward === null &&
            ((item.visibility === 'workspace' && currentWorkspace()?.role === 'Owner') ||
              (item.visibility === 'selected' && item.editable)) && (
              <Button
                disabled={mutation.busy}
                onClick={() =>
                  void mutation.run(async () => {
                    await request(`/${id}/action`, {
                      request: crypto.randomUUID(),
                      revision: item.revision,
                      action: 'claim',
                    });
                    refresh();
                  })
                }
              >
                Manage this shared item
              </Button>
            )}
          {item.manageable && (
            <>
              <Button onClick={() => setDialog('share')}>Share</Button>
              <Button
                disabled={mutation.busy}
                onClick={() =>
                  void mutation.run(async () => {
                    if (
                      !confirm(
                        'Move this item to Trash? People and agents will lose access until you restore it.',
                      )
                    )
                      return;
                    await request(`/${id}/action`, {
                      request: crypto.randomUUID(),
                      revision: item.revision,
                      action: 'trash',
                    });
                    changed();
                    navigate(itemUrl());
                  })
                }
              >
                Trash
              </Button>
            </>
          )}
        </div>
      </header>
      {(error || mutation.status) && (
        <p className="library-notice" role="alert">
          {error || mutation.status}
        </p>
      )}
      {stale && !saving.busy && (
        <p className="library-notice" role="status">
          A newer saved revision is available. Your current draft stays in this browser.{' '}
          <Button onClick={reload}>Load saved version</Button>
        </p>
      )}
      {sourceRevision && Number(sourceRevision) !== item.revision && (
        <p className="library-notice">
          The agent used revision {sourceRevision}. You are viewing the current revision{' '}
          {item.revision}; editors can inspect saved history.
        </p>
      )}
      {item.kind === 'page' ? (
        <PageEditor
          item={item}
          saving={saving}
          history={() => setDialog('history')}
          saved={reload}
        />
      ) : (
        <FilePreview item={item} />
      )}
      {dialog === 'share' && item.manageable && (
        <ShareDialog item={item} close={() => setDialog(undefined)} changed={refresh} />
      )}
      {dialog === 'ask' && <AskDialog item={item} close={() => setDialog(undefined)} />}
      {dialog === 'history' && item.editable && (
        <HistoryDialog item={item} close={() => setDialog(undefined)} changed={reload} />
      )}
    </section>
  );
}
function FilePreview({ item }: { item: Item }) {
  return (
    <div className="library-file">
      <div className="library-file-main">
        <h1>{item.title}</h1>
        {item.mime.startsWith('image/') ? (
          <img className="library-image" src={downloadUrl(item.id, true)} alt={item.title} />
        ) : item.mime === 'application/pdf' ? (
          <Suspense fallback={<p role="status">Opening PDF viewer…</p>}>
            <PdfPreview id={item.id} />
          </Suspense>
        ) : item.mime === 'text/plain' ? (
          <iframe
            className="library-text-preview"
            src={downloadUrl(item.id, true)}
            title={`Preview of ${item.title}`}
            sandbox=""
          />
        ) : (
          <div className="library-preview-empty">
            <span>▤</span>
            <p>Preview is unavailable for this file type.</p>
            <a className="ui-button" href={downloadUrl(item.id)}>
              Download original
            </a>
          </div>
        )}
        <p className="library-muted">
          If the preview cannot be displayed, download the original file.
        </p>
      </div>
      <aside className="library-file-info">
        <h2>File details</h2>
        <dl>
          <dt>Type</dt>
          <dd>{item.mime}</dd>
          <dt>Size</dt>
          <dd>{bytesLabel(item.size)}</dd>
          <dt>Updated</dt>
          <dd>{dateLabel(item.updated)}</dd>
          <dt>People access</dt>
          <dd>{visibilityName(item.visibility)}</dd>
          <dt>Agent reading</dt>
          <dd>
            {item.readiness === 'ready'
              ? 'Text ready'
              : item.readiness === 'partial'
                ? 'Limited excerpt ready'
                : 'Text extraction unavailable'}
          </dd>
        </dl>
        {item.readiness === 'partial' && (
          <p>
            Agent context is limited. PDFs include text from at most the first 50 pages; each
            question uses up to 12,000 characters.
          </p>
        )}
        <a className="ui-button primary" href={downloadUrl(item.id)}>
          Download original
        </a>
        <p className="library-muted">
          Files stay private until the creator shares them. Sharing and agent access are separate.
        </p>
      </aside>
    </div>
  );
}
