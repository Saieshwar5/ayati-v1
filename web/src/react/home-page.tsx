import { currentWorkspace } from '../workspace-context';
import { lazy, Suspense, useEffect, useState } from 'react';
import { AttentionBoundary } from './attention/boundary';
import { WorkRecords } from './work-records';
import { useAttention } from './attention/use-attention';
import '../home.css';

const WorkspacePanels = lazy(() => import('./home-panels/panels').then(module => ({ default: module.WorkspacePanels })));
const HomeHints = lazy(() => import('./home-panels/home-hints').then(module => ({ default: ({ page }: { page?: import('./attention/model').AttentionPage | null }) => <><module.NeedsYouLine page={page} /><module.PanelSuggestions /></> })));
const HomeAttention = lazy(() => import('./attention/home-attention').then(module => ({ default: module.HomeAttention })));

/** Device time until the workspace has a shared business location setting. */
function BusinessClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 30000);
    return () => window.clearInterval(timer);
  }, []);
  // The workspace's time zone (Settings → Workspace), or this device's until one is chosen.
  const chosen = currentWorkspace()?.timezone ?? undefined;
  const timezone = chosen ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
  return <section className="home-clock-panel" aria-label="Business clock"><h2>Business clock</h2>
    <time dateTime={now.toISOString()}>
      <span className="business-clock-value">{now.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit', timeZone: timezone })}</span>
      <span className="business-clock-date">{now.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', timeZone: timezone })}</span>
    </time>
    <p className="home-location">{chosen ? 'Workspace time' : 'Device time'} · {timezone.replaceAll('_', ' ')}</p>
  </section>;
}

export function HomePage() {
  // One read of what needs you, shared by Notifications and every panel's "needs you".
  const attention = useAttention('needs_you', null);
  return <main className="home" aria-label="Home">
    <Suspense fallback={null}><HomeHints page={attention.data} /></Suspense>
    <div className="home-standard-panels">
      <BusinessClock />
      <div className="home-notifications-host"><AttentionBoundary>
        <Suspense fallback={<p role="status">Checking saved work…</p>}><HomeAttention shared={attention} /></Suspense>
      </AttentionBoundary></div>
    </div>
    <AttentionBoundary><Suspense fallback={<p role="status">Opening saved panels…</p>}><WorkspacePanels needsYou={attention.data} /></Suspense></AttentionBoundary>
    <WorkRecords />
  </main>;
}
