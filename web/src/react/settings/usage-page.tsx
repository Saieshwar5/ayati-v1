import { currentWorkspace } from '../../workspace-context';
import { UsageReportView } from '../usage/usage-report';
import { BudgetsView } from '../usage/budgets';

/** Settings → Usage (MG2): the monthly budgets first, since they are what you act
 * on, then the charges. Admins see the workspace's model use; members see the use
 * made for their own requests. The server decides which. */
export function UsagePage() {
  const admin = currentWorkspace()?.role === 'Admin';
  return <section className="settings-usage"><header className="settings-section-heading"><div><h2>Usage</h2>
    <p>{admin ? 'What agents cost across the workspace.' : 'Model use and charges for your requests.'}</p></div></header>
    <BudgetsView />
    <UsageReportView title={admin ? 'Workspace usage' : 'Your usage'} />
  </section>;
}
