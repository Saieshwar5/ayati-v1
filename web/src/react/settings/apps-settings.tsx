import { AllowedApps } from './allowed-apps';
import { useConnections } from './app-connections';

/** Settings → Apps (the page was Connections until 2026-11-21): one list of the apps
 * the workspace offers, each with the admins' decision and its connections under it.
 * Switching an app off ends its connections, so they are read again. */
export function ConnectionsSettings() {
  const connections = useConnections();
  return <AllowedApps connections={connections} changed={() => void connections.refresh().catch(() => {})} />;
}
