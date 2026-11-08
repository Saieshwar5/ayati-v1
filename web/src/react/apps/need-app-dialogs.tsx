import { useState } from 'react';
import { RequestDialog } from './company-apps';
import { CatalogDialog } from './catalog-dialog';
import { ConnectDialog } from './connect-dialog';
import type { CatalogApp } from './apps-model';

/** What "Need another app?" opens: a member's request, or the admin's catalog and
 * then Connect, with sign-in in a new tab so the page underneath is kept. */
export function NeedAppDialogs({ admin, close, sent, connected }: { admin: boolean; close: () => void; sent: () => void; connected: () => void }) {
  const [connecting, setConnecting] = useState<CatalogApp>();
  if (!admin) return <RequestDialog close={close} sent={sent} />;
  return connecting ? <ConnectDialog app={connecting} keepPage close={close} connected={() => { close(); connected(); }} />
    : <CatalogDialog canManage close={close} connect={setConnecting} />;
}
