import { currentWorkspace } from '../../workspace-context';
import { UsageReportView } from '../usage/usage-report';
import { BudgetsView } from '../usage/budgets';

/** Settings → Usage (MG2): admins see the workspace's model use and charges; members
 * see the use made for their own requests. The server decides which. */
export function UsagePage() {
  const admin = currentWorkspace()?.role === 'Admin';
  return <section className="settings-usage"><header className="settings-section-heading"><div><h2>Usage</h2>
    <p>{admin ? 'Model use and charges across the workspace, by agent, model, person and day.' : 'Model use and charges for your requests.'}</p></div></header>
    <UsageReportView title={admin ? 'Workspace usage' : 'Your usage'} />
    <BudgetsView />
  </section>;
}
