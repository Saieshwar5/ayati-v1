import { lazy, Suspense, useCallback, useEffect, useState } from 'react';
import { useResource } from '../react/requests';
import { control, NotOperator, SignedOut, type Providers, type Session } from './api';
import { CommandPalette } from './palette';
import { usePolling } from './shared';

const Overview = lazy(() => import('./overview').then(m => ({ default: m.OverviewScreen })));
const Workspaces = lazy(() => import('./workspaces').then(m => ({ default: m.WorkspacesScreen })));
const ProvidersScreen = lazy(() => import('./providers').then(m => ({ default: m.ProvidersScreen })));
const Routing = lazy(() => import('./routing').then(m => ({ default: m.RoutingScreen })));
const SpendScreen = lazy(() => import('./spend').then(m => ({ default: m.SpendScreen })));
const Codes = lazy(() => import('./codes').then(m => ({ default: m.CodesScreen })));
const Configuration = lazy(() => import('./configuration').then(m => ({ default: m.ConfigurationScreen })));
const ChangeLog = lazy(() => import('./change-log').then(m => ({ default: m.ChangeLogScreen })));
const Reports = lazy(() => import('./reports').then(m => ({ default: m.ReportsScreen })));

export type Section = 'overview' | 'workspaces' | 'providers' | 'routing' | 'spend' | 'codes' | 'config' | 'log' | 'reports';
export type Go = (section: Section, params?: Record<string, string>) => void;

const GROUPS: { name: string; items: [Section, string][] }[] = [
  { name: 'Monitor', items: [['overview', 'Overview'], ['spend', 'Spend'], ['reports', 'Problem reports']] },
  { name: 'Customers', items: [['workspaces', 'Workspaces'], ['codes', 'Invite codes']] },
  { name: 'Models', items: [['providers', 'Providers'], ['routing', 'Routing']] },
  { name: 'Platform', items: [['config', 'Configuration'], ['log', 'Change log']] },
];
const TITLES: Record<Section, [string, string]> = {
  overview: ['Overview', 'Live · refreshes every 15 seconds'], spend: ['Spend', 'Model cost from token use and catalog prices'],
  reports: ['Problem reports', 'Sent from Settings → Help in any workspace'], workspaces: ['Workspaces', 'Every company using Cuckoo'],
  codes: ['Invite codes', 'Each code creates one workspace'], providers: ['Providers', 'Model services Cuckoo calls, their keys and models'],
  routing: ['Routing', 'Which provider serves each model agents ask for, and the backup'],
  config: ['Configuration', 'Platform settings and where each comes from'], log: ['Change log', 'Every operator change, from the web or the CLI'],
};

function readSection(): { section: Section; params: URLSearchParams } {
  const params = new URLSearchParams(location.search);
  const section = params.get('section') as Section | null;
  return { section: section && section in TITLES ? section : 'overview', params };
}

const loadProviders = () => control<Providers>('/providers');

export function ControlApp() {
  const [state, setState] = useState<{ session?: Session; failure?: Error }>({});
  useEffect(() => {
    let live = true;
    control<Session>('/session').then(
      session => { if (live) setState({ session }); },
      failure => { if (live) setState({ failure: failure instanceof Error ? failure : new Error('Request could not be completed.') }); },
    );
    return () => { live = false; };
  }, []);
  const { session, failure } = state;
  if (failure instanceof NotOperator) return <Gate title="Cuckoo Control is for Cuckoo operators" body="Your account is not on the operator list. Ask whoever runs the deployment to add your email to AYATI_PLATFORM_ADMINS." />;
  if (failure instanceof SignedOut) return <Gate title="Sign in to open Cuckoo Control" body="Sign in with your operator account, then open this page again." signIn />;
  if (failure) return <Gate title="Cuckoo Control is unavailable" body={failure.message} />;
  if (!session) return <main className="cc-gate" role="status">Opening Cuckoo Control…</main>;
  return <Console session={session} />;
}

function Gate({ title, body, signIn = false }: { title: string; body: string; signIn?: boolean }) {
  return <main className="cc-gate"><div className="cc-card cc-gate-box">
    <h1>{title}</h1><p className="cc-dim">{body}</p>
    {signIn && <a className="cc-btn primary" href="/auth/login">Sign in</a>}
  </div></main>;
}

function Console({ session }: { session: Session }) {
  const [{ section, params }, setPlace] = useState(readSection);
  const [palette, setPalette] = useState(false);
  const providers = useResource(loadProviders);
  usePolling(providers.refresh, 15_000);
  const go: Go = useCallback((next, extra = {}) => {
    const query = new URLSearchParams({ section: next, ...extra });
    window.history.pushState(null, '', `?${query}`);
    setPlace({ section: next, params: query });
    setPalette(false);
  }, []);
  useEffect(() => {
    const back = () => setPlace(readSection());
    const keys = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') { event.preventDefault(); setPalette(open => !open); }
    };
    window.addEventListener('popstate', back);
    window.addEventListener('keydown', keys);
    return () => { window.removeEventListener('popstate', back); window.removeEventListener('keydown', keys); };
  }, []);
  const production = session.environment === 'production';
  const [title, subtitle] = TITLES[section];
  return <div className={`cc-root env-${production ? 'production' : session.environment === 'staging' ? 'staging' : 'other'}`}>
    <div className="cc-band" aria-hidden="true" />
    <header className="cc-top">
      <div className="cc-brand"><span className="cc-mark" aria-hidden="true">c</span>cuckoo <span className="cc-dim">control</span></div>
      <span className="cc-env" title="Which deployment you are changing">{session.environment.toUpperCase()}</span>
      <button type="button" className="cc-search" onClick={() => setPalette(true)}>
        <span>Find a workspace, or run a command</span><kbd className="cc-kbd">⌘K</kbd>
      </button>
      <span className="cc-spacer" />
      <span className="cc-dim cc-who" title={session.email}>{session.name}</span>
    </header>
    <StatusStrip providers={providers.data} error={providers.error} go={go} />
    <div className="cc-body">
      <nav className="cc-side" aria-label="Sections">
        {GROUPS.map(group => <div key={group.name} className="cc-side-group">
          <div className="cc-cap">{group.name}</div>
          {group.items.map(([id, label]) => <a key={id} href={`?section=${id}`} aria-current={id === section ? 'page' : undefined}
            onClick={event => { event.preventDefault(); go(id); }}>{label}</a>)}
        </div>)}
      </nav>
      <main className="cc-main">
        <div className="cc-head"><div><div className="cc-dim cc-crumb">{GROUPS.find(g => g.items.some(i => i[0] === section))?.name}</div><h1>{title}</h1></div>
          <div className="cc-dim">{subtitle}</div></div>
        <Suspense fallback={<p className="cc-dim" role="status">Loading…</p>}>
          {section === 'overview' && <Overview go={go} />}
          {section === 'workspaces' && <Workspaces key={params.get('q') ?? ''} go={go} initialQuery={params.get('q') ?? ''} selected={params.get('id')} />}
          {section === 'providers' && <ProvidersScreen providers={providers} canStoreKeys={session.can_store_keys} selected={params.get('id')} go={go} />}
          {section === 'routing' && <Routing providers={providers.data} selected={params.get('id')} go={go} />}
          {section === 'spend' && <SpendScreen go={go} />}
          {section === 'codes' && <Codes />}
          {section === 'config' && <Configuration />}
          {section === 'log' && <ChangeLog />}
          {section === 'reports' && <Reports go={go} />}
        </Suspense>
      </main>
    </div>
    {palette && <CommandPalette go={go} close={() => setPalette(false)} />}
  </div>;
}

/** Every provider's health on every screen; a click opens it. */
function StatusStrip({ providers, error, go }: { providers?: Providers; error: string; go: Go }) {
  return <div className="cc-strip" aria-label="Live provider status">
    {error && <span className="cc-alert">{error}</span>}
    {providers?.providers.filter(p => p.source !== 'missing').map(p => {
      const rate = p.health.calls ? (100 * p.health.errors / p.health.calls) : 0;
      const tone = !p.enabled ? 'bad' : rate >= 5 ? 'warn' : 'ok';
      return <button key={p.name} type="button" onClick={() => go('providers', { id: p.name })}>
        <span className="cc-strip-name"><span className={`cc-dot ${tone}`} aria-hidden="true" />{p.name}
          <span className={`cc-strip-state ${tone}`}>{!p.enabled ? 'OFF' : tone === 'warn' ? 'ERRORS' : 'OK'}</span></span>
        <span className="cc-num cc-dim">{p.enabled ? `${p.health.calls} calls/h · ${rate.toFixed(1)}% err${p.health.p95_ms ? ` · p95 ${(p.health.p95_ms / 1000).toFixed(1)} s` : ''}` : 'no traffic'}</span>
      </button>;
    })}
    {providers && !providers.providers.length && <span className="cc-dim cc-strip-empty">No model providers yet. Add one in Providers.</span>}
  </div>;
}
