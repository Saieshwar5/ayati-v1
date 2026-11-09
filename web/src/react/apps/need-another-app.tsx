import { lazy, Suspense, useState } from 'react';
import { Button } from '../ui';
import { currentWorkspace } from '../../workspace-context';
import { PlusIcon } from './app-marks';
import './need-another-app.css';

// The dialogs load on first use, keeping agent pages' startup lighter.
const NeedAppDialogs = lazy(() => import('./need-app-dialogs').then(module => ({ default: module.NeedAppDialogs })));

/** "Need another app?" where agents get their tools (designs/connector-screens 5).
 * Members ask an admin through the existing app requests. Admins connect one here:
 * the catalog and connect dialogs open over the page, and sign-in opens in a new
 * tab, so a draft agent is kept. The client role only chooses what to show; the
 * server decides who may connect. */
export function NeedAnotherApp({ draft = false, connected }: { draft?: boolean; connected: () => void }) {
  const admin = currentWorkspace()?.role === 'Admin';
  const [open, setOpen] = useState(false);
  const [sent, setSent] = useState(false);
  return <div className="need-app">
    <span className="need-app-plus" aria-hidden="true"><PlusIcon /></span>
    <span className="need-app-text"><strong>Need another app?</strong>
      <small>{admin ? `Connect one here.${draft ? ' This draft is kept.' : ''}` : sent ? 'Sent. Your admin decides on the Apps page.' : 'Your admin gets the request on the Apps page.'}</small></span>
    <Button onClick={() => setOpen(true)}>{admin ? 'Connect an app' : 'Ask an admin'}</Button>
    {open && <Suspense fallback={<span className="ui-status" role="status">Opening…</span>}>
      <NeedAppDialogs admin={admin} close={() => setOpen(false)} sent={() => setSent(true)} connected={connected} /></Suspense>}
  </div>;
}
