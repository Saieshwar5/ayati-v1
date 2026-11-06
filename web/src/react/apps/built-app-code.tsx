import { useCallback, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import {
  appsRequest,
  builtPath,
  downloadUrl,
  type AppCheck,
  type AppFiles,
  type BuiltApp,
} from './apps-model';

const dateLabel = (seconds: number) =>
  new Date(seconds * 1000).toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  });

/** An app's draft files, the checks that block publishing, and its versions (BA1).
 * Only the builder changes the draft and publishes; admins can roll back. */
export function CodeDrawer({
  app,
  close,
  changed,
}: {
  app: BuiltApp;
  close: () => void;
  changed: () => void;
}) {
  const load = useCallback(
    () => appsRequest<AppFiles>(builtPath(app.id, '/files?version=0')),
    [app.id],
  );
  const files = useResource(load);
  const mutation = useMutation();
  const [selected, setSelected] = useState('index.html');
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [adding, setAdding] = useState('');
  const [note, setNote] = useState('');
  const [failed, setFailed] = useState<AppCheck[]>();
  const live = app.version;
  const list = files.data?.files ?? [];
  const current = list.find((f) => f.path === selected) ?? list[0];
  const text = current ? (drafts[current.path] ?? current.content) : '';
  const edited = Boolean(
    current && drafts[current.path] !== undefined && drafts[current.path] !== current.content,
  );
  const unsaved = Object.entries(drafts).filter(
    ([path, content]) => list.find((f) => f.path === path)?.content !== content,
  ).length;
  const checks = failed ?? files.data?.checks ?? [];
  const write = (path: string, content: string | null) =>
    void mutation.run(async () => {
      await appsRequest(builtPath(app.id, '/files'), {
        path,
        content,
        revision: files.data?.draft_revision ?? 0,
      });
      setDrafts((old) => {
        const next = { ...old };
        delete next[path];
        return next;
      });
      setFailed(undefined);
      await files.refresh();
      if (content === null) setSelected('index.html');
      mutation.setStatus(content === null ? `Deleted ${path}.` : `Saved ${path}.`);
    });
  const publish = () =>
    void mutation.run(async () => {
      const result = await appsRequest<{ state: string; version?: number; checks?: AppCheck[] }>(
        builtPath(app.id, '/publish'),
        { note, revision: files.data?.draft_revision ?? 0 },
      );
      if (result.state === 'checks_failed') {
        setFailed(result.checks ?? []);
        mutation.setStatus('Fix what the checks found, then publish.');
        return;
      }
      changed();
      setNote('');
      mutation.setStatus(`Published version ${result.version}. People you chose can open it now.`);
    });
  const switchTo = (version: number) =>
    void mutation.run(async () => {
      if (
        version === 0 &&
        !confirm(
          `Take ${app.name} offline? Nobody will be able to open it until it is published again.`,
        )
      )
        return;
      await appsRequest(builtPath(app.id, '/rollback'), { note: '', revision: version });
      changed();
      mutation.setStatus(version ? `Version ${version} is live.` : 'The app is offline.');
    });
  return (
    <Dialog
      drawer
      title={`${app.name}: code`}
      close={close}
      busy={mutation.busy}
      status={mutation.status || files.error}
    >
      {!files.data && !files.error && <p role="status">Loading files…</p>}
      {files.data && (
        <div className="built-code">
          <nav aria-label="Files" className="built-files">
            <ul>
              {list.map((file) => (
                <li key={file.path}>
                  <button
                    type="button"
                    aria-current={current?.path === file.path ? 'true' : undefined}
                    onClick={() => setSelected(file.path)}
                  >
                    {file.path}
                    {drafts[file.path] !== undefined && drafts[file.path] !== file.content && (
                      <span aria-label="unsaved"> •</span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
            {app.is_builder && (
              <div className="built-new">
                <input
                  aria-label="New file name"
                  placeholder="new-file.js"
                  value={adding}
                  onChange={(e) => setAdding(e.target.value)}
                />
                <Button
                  disabled={
                    mutation.busy || !adding.trim() || list.some((f) => f.path === adding.trim())
                  }
                  onClick={() => {
                    write(adding.trim(), '');
                    setSelected(adding.trim());
                    setAdding('');
                  }}
                >
                  Add file
                </Button>
              </div>
            )}
          </nav>
          {current && (
            <div className="built-editor">
              <textarea
                aria-label={`Contents of ${current.path}`}
                spellCheck={false}
                wrap="off"
                readOnly={!app.is_builder}
                value={text}
                onChange={(e) => setDrafts((old) => ({ ...old, [current.path]: e.target.value }))}
              />
              {app.is_builder && (
                <div className="apps-row">
                  <Button
                    primary
                    disabled={!edited || mutation.busy}
                    onClick={() => write(current.path, text)}
                  >
                    Save file
                  </Button>
                  {edited && (
                    <Button
                      disabled={mutation.busy}
                      onClick={() =>
                        setDrafts((old) => {
                          const next = { ...old };
                          delete next[current.path];
                          return next;
                        })
                      }
                    >
                      Discard
                    </Button>
                  )}
                  {!['index.html', 'cuckoo-app.json'].includes(current.path) && (
                    <Button
                      disabled={mutation.busy}
                      onClick={() => {
                        if (confirm(`Delete ${current.path}?`)) write(current.path, null);
                      }}
                    >
                      Delete file
                    </Button>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      )}
      {files.data && (
        <section className="built-checks" aria-label="Checks">
          <h3>Checks</h3>
          {checks.length ? (
            <ul>
              {checks.map((check, i) => (
                <li key={i} className="built-check-failed">
                  <strong>{check.check}</strong> {check.message}
                </li>
              ))}
            </ul>
          ) : (
            <p>
              Every check passes: manifest, screens, size, no keys in the code, nothing loaded from
              the internet.
            </p>
          )}
        </section>
      )}
      {app.is_builder && files.data && (
        <section className="built-publish" aria-label="Publish">
          <h3>Publish</h3>
          <p className="app-line">
            {unsaved
              ? `Save ${unsaved} changed file${unsaved === 1 ? '' : 's'} first; publishing uses the saved draft.`
              : 'Publishing makes the saved draft the version people open.'}
          </p>
          <label className="ui-field">
            What changed <small>Optional</small>
            <input
              aria-label="What changed"
              maxLength={300}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <Button primary disabled={mutation.busy || unsaved > 0} onClick={publish}>
            Publish
          </Button>
        </section>
      )}
      <section className="built-versions" aria-label="Versions">
        <h3>Versions</h3>
        {!app.versions?.length && <p className="app-line">Nothing published yet.</p>}
        <ul>
          {app.versions?.map((v) => (
            <li key={v.version}>
              <span>
                <strong>Version {v.version}</strong>
                {live === v.version && ' · live'}
                <small>
                  {dateLabel(v.at)}
                  {v.note && ` · ${v.note}`}
                </small>
              </span>
              <span className="apps-row">
                {live !== v.version && (
                  <Button disabled={mutation.busy} onClick={() => switchTo(v.version)}>
                    Make live
                  </Button>
                )}
                <a className="ui-button" href={downloadUrl(app.id, v.version)} download>
                  Download
                </a>
              </span>
            </li>
          ))}
        </ul>
        <div className="apps-row">
          <a className="ui-button" href={downloadUrl(app.id, 0)} download>
            Download draft
          </a>
          {live && (
            <Button disabled={mutation.busy} onClick={() => switchTo(0)}>
              Take offline
            </Button>
          )}
        </div>
      </section>
    </Dialog>
  );
}
