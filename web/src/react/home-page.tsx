import { lazy, Suspense, useEffect, useState } from 'react';
import { AttentionBoundary } from './attention/boundary';
import { WorkRecords } from './work-records';
import '../home.css';

const WorkspacePanels = lazy(() => import('./home-panels/panels').then(module => ({ default: module.WorkspacePanels })));
const HomeAttention = lazy(() => import('./attention/home-attention').then(module => ({ default: module.HomeAttention })));

/** Device time until the workspace has a shared business location setting. */
function BusinessClock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 30000);
    return () => window.clearInterval(timer);
  }, []);
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  return <section className="home-clock-panel" aria-label="Business clock"><h2>Business clock</h2>
    <time dateTime={now.toISOString()}>
      <span className="business-clock-value">{now.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}</span>
      <span className="business-clock-date">{now.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' })}</span>
    </time>
    <p className="home-location">Device time · {timezone.replaceAll('_', ' ')}</p>
  </section>;
}

export function HomePage() {
  return <main className="home" aria-label="Home">
    <div className="home-standard-panels">
      <BusinessClock />
      <div className="home-notifications-host"><AttentionBoundary>
        <Suspense fallback={<p role="status">Checking saved work…</p>}><HomeAttention /></Suspense>
      </AttentionBoundary></div>
    </div>
    <AttentionBoundary><Suspense fallback={<p role="status">Opening saved panels…</p>}><WorkspacePanels /></Suspense></AttentionBoundary>
    <WorkRecords />
  </main>;
}
