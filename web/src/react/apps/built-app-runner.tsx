import { workspaceUrl } from '../../workspace-context';
import { Button } from '../ui';
import { useResource } from '../requests';
import { frameUrl, loadApps } from './apps-model';

/** Runs a built app (BA1). The frame loads the app's document from Cuckoo, which
 * serves it with a sandboxing content policy: without allow-same-origin it gets a unique
 * origin, so it cannot read Cuckoo's cookies, storage or page, and every network request
 * it makes is blocked. */
export function AppRunner({ id, preview }: { id: string; preview: boolean }) {
  const apps = useResource(loadApps);
  const app = apps.data?.built?.find((built) => built.id === id);
  const runs = app && (preview ? app.can_manage : app.can_open && app.version);
  return (
    <main className="apps-runner" aria-label={app?.name ?? 'App'}>
      <header className="apps-runner-bar">
        <a href={workspaceUrl('/?view=apps')}>← Apps</a>
        <h1>{app?.name ?? (apps.data ? 'App unavailable' : 'Opening app…')}</h1>
        {runs && (
          <span className="app-line">{preview ? 'Draft preview' : `Version ${app.version}`}</span>
        )}
      </header>
      {apps.error && (
        <div className="apps-runner-error">
          <p role="alert">{apps.error}</p>
          <Button onClick={() => void apps.refresh().catch(() => {})}>Retry</Button>
        </div>
      )}
      {!apps.data && !apps.error && <p role="status">Opening app…</p>}
      {apps.data && !runs && (
        <div className="apps-runner-error">
          <p role="alert">
            {preview
              ? 'Only its builder and admins preview a draft.'
              : "This app isn't published for you."}
          </p>
        </div>
      )}
      {app && runs && (
        <iframe
          title={app.name}
          className="apps-runner-frame"
          sandbox="allow-scripts allow-forms"
          referrerPolicy="no-referrer"
          src={frameUrl(app.id, preview)}
        />
      )}
    </main>
  );
}
