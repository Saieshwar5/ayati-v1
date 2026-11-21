import { currentWorkspace } from '../../workspace-context';
import { PauseAgentsControl } from '../activity/agents-pause';
import { AllowedModels } from './allowed-models';
import { CompanyRules } from './company-rules';

/** Settings → Agent rules: what every agent in the workspace must follow, in one
 * place (2026-11-21). Everyone reads the company rules; admins also choose the
 * models, the approval backup and can pause every agent. */
export function AgentRules() {
  const admin = currentWorkspace()?.role === 'Admin';
  return <section className="settings-agent-rules">
    <h2>Agent rules</h2>
    <p>What every agent in this workspace must follow. Each agent’s own settings come on top.</p>
    <CompanyRules />
    {admin && <AllowedModels />}
    {admin && <section className="settings-pause" aria-label="Pause all agents">
      <div><h3>Pause all agents</h3><p>If an agent is doing something wrong, stop every agent at once. Resume when it’s fixed.</p></div>
      <PauseAgentsControl />
    </section>}
  </section>;
}
