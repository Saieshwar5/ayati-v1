import { useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { loadAgents } from '../agents/model';
import { budgetSentence, loadBudgets, saveBudget, warnChoices, type Budget, type BudgetInput, type Budgets } from './budget-model';
import { dollars } from './usage-model';

/** Monthly model budgets (MG3): everyone sees how much is left; admins set the
 * workspace's budget and budgets for single agents. A failed save keeps the draft. */
export function BudgetsView() {
  const resource = useResource(loadBudgets);
  const data = resource.data;
  return <section className="usage-budgets" aria-label="Monthly budgets">
    <h3>Monthly budgets</h3>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!data && !resource.error && <p role="status">Loading budgets…</p>}
    {data && <>
      {!(data.budgets ?? []).length && <p className="ui-help">No budget is set{data.can_change ? '. Set one to stop model charges at a monthly limit.' : '.'}</p>}
      <ul className="usage-budget-list">{(data.budgets ?? []).map(budget => <li key={budget.agent || 'workspace'}>
        <BudgetLine budget={budget} />
        {data.can_change && <RemoveBudget budget={budget} saved={resource.setData} />}
      </li>)}</ul>
      {data.can_change && <BudgetForm budgets={data} saved={resource.setData} />}
    </>}
  </section>;
}

export function BudgetLine({ budget }: { budget: Budget }) {
  return <div className={`usage-budget usage-budget-${budget.state}`}>
    <div className="usage-budget-head"><strong>{budget.name}</strong>
      <span>{dollars(budget.used_usd)} of {dollars(budget.monthly_usd)} this month</span></div>
    <meter min={0} max={100} low={budget.warn_percent} high={99} optimum={0} value={Math.min(budget.percent, 100)}
      aria-label={`${budget.name}: ${budget.percent}% of the monthly budget used`} />
    <small role={budget.state === 'ok' ? undefined : 'status'}>{budgetSentence(budget)}</small>
  </div>;
}

function RemoveBudget({ budget, saved }: { budget: Budget; saved: (next: Budgets) => void }) {
  const mutation = useMutation();
  return <div className="usage-budget-remove">
    <Button disabled={mutation.busy} aria-label={`Remove the budget for ${budget.name}`}
      onClick={() => void mutation.run(async () => saved(await saveBudget({ agent: budget.agent, monthly_usd: null, warn_percent: budget.warn_percent })))}>Remove</Button>
    {mutation.status && <p role="alert">{mutation.status}</p>}
  </div>;
}

function BudgetForm({ budgets, saved }: { budgets: Budgets; saved: (next: Budgets) => void }) {
  const agents = useResource(loadAgents);
  const [agent, setAgent] = useState('');
  const [amount, setAmount] = useState('');
  const [warn, setWarn] = useState(80);
  const mutation = useMutation();
  const existing = budgets.budgets.find(b => b.agent === agent);
  const value = Number(amount);
  const valid = amount.trim() !== '' && Number.isFinite(value) && value >= 0.01;
  const submit = () => void mutation.run(async () => {
    const input: BudgetInput = { agent, monthly_usd: value, warn_percent: warn };
    saved(await saveBudget(input));
    setAmount('');
    mutation.setStatus('Budget saved.');
  });
  return <form className="ui-form usage-budget-form" aria-label="Set a monthly budget" onSubmit={event => { event.preventDefault(); if (valid) submit(); }}>
    <fieldset className="ui-fieldset" disabled={mutation.busy}>
      <label className="ui-field">For<select value={agent} onChange={event => setAgent(event.target.value)}>
        <option value="">The whole workspace</option>
        {(agents.data?.agents ?? []).map(a => <option key={a.id} value={a.id}>{a.name}</option>)}
      </select></label>
      <label className="ui-field">Monthly budget in US dollars<input inputMode="decimal" value={amount} placeholder={existing ? String(existing.monthly_usd) : 'e.g. 50'}
        onChange={event => setAmount(event.target.value.replace(/[^0-9.]/g, ''))} /></label>
      <label className="ui-field">Warn at<select value={warn} onChange={event => setWarn(Number(event.target.value))}>
        {warnChoices.map(n => <option key={n} value={n}>{n}% used</option>)}</select></label>
      <Button type="submit" primary disabled={mutation.busy || !valid}>{mutation.busy ? 'Saving…' : existing ? 'Change budget' : 'Set budget'}</Button>
    </fieldset>
    <p role="status" className="ui-status">{mutation.status}</p>
    <p className="ui-help">At the limit, agents take no new requests and model calls stop until the month ends or the budget is raised.</p>
  </form>;
}

/** On an agent's page: the budgets that limit it, its own and the workspace's. */
export function AgentBudgets({ agent }: { agent: string }) {
  const resource = useResource(loadBudgets);
  const limits = (resource.data?.budgets ?? []).filter(b => b.agent === agent || b.agent === '');
  if (!limits.length) return null;
  return <div className="usage-budgets" aria-label="Monthly budgets"><h3>Monthly budgets</h3>
    <ul className="usage-budget-list">{limits.map(b => <li key={b.agent || 'workspace'}><BudgetLine budget={b} /></li>)}</ul>
    <p className="ui-help">Admins change budgets in Settings → Usage.</p>
  </div>;
}
