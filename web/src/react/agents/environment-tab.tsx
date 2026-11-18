import { useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { saveAgent, type Agent, type Directory } from './model';
import { CatalogModelPicker } from './catalog-model-picker';
import { UsageReportView } from '../usage/usage-report';
import { AgentBudgets } from '../usage/budgets';

/** Settings: the model it uses, its usage and budgets. Its computer is under
 * Connections. When the admin lists allowed models
 * (Settings → Workspace), the model is picked from that list; otherwise typed.
 * With Cuckoo's model catalog, people pick a catalog model and never a provider. */
export function EnvironmentTab({ agent, directory, saved }: { agent: Agent; directory: Directory; saved: () => Promise<void> }) {
  return <div className="agent-environment">
    <section aria-label="Model">
      <h3>Model</h3>
      {!agent.editable ? <p>{directory.catalog?.models.find(m => m.id === agent.model)?.name ?? agent.model}</p>
        : directory.catalog?.catalog_on ? <CatalogModelPicker agent={agent} directory={directory} catalog={directory.catalog} saved={saved} />
          : <ProviderModelForm agent={agent} directory={directory} saved={saved} />}
    </section>
    {agent.editable && <section aria-label="Usage"><UsageReportView key={agent.id} agent={agent.id} title="Usage" /><AgentBudgets key={`budgets-${agent.id}`} agent={agent.id} /></section>}
  </div>;
}

/** Without a catalog: the provider and model, typed or from the admin's list. */
function ProviderModelForm({ agent, directory, saved }: { agent: Agent; directory: Directory; saved: () => Promise<void> }) {
  const [provider, setProvider] = useState(agent.settings.provider);
  const [model, setModel] = useState(agent.model);
  const mutation = useMutation();
  const changed = provider !== agent.settings.provider || model.trim() !== agent.model;
  const allowed = directory.models ?? [];
  const current = allowed.some(item => item.model === agent.model && (!agent.settings.provider || item.provider === agent.settings.provider));
  const save = () => void mutation.run(async () => {
    await saveAgent({ ...agent, model: model.trim(), people: undefined, settings: { ...agent.settings, provider } });
    mutation.setStatus('Model saved. New requests use it.');
    await saved();
  });
  return <div className="ui-form">
    {allowed.length ? <label className="ui-field">Model<select value={`${provider}/${model}`} onChange={event => {
      const choice = allowed.find(item => `${item.provider}/${item.model}` === event.target.value);
      if (choice) { setProvider(choice.provider); setModel(choice.model); }
    }}>
      {!current && <option value={`${provider}/${model}`} disabled>{model} · no longer allowed</option>}
      {allowed.map(item => <option key={`${item.provider}/${item.model}`} value={`${item.provider}/${item.model}`}>
        {item.model} · {directory.providers.find(p => p.id === item.provider)?.name ?? item.provider}{item.model === directory.default_model ? ' · workspace default' : ''}</option>)}
    </select></label> : <>
    <label className="ui-field">Model provider<select value={provider} onChange={event => setProvider(event.target.value)}>
      <option value="">Workspace default</option>
      {directory.providers.map(item => <option key={item.id} value={item.id} disabled={!item.configured}>{item.name}{item.configured ? '' : ' · not connected'}</option>)}
    </select></label>
    <label className="ui-field">Model<input required maxLength={200} value={model} onChange={event => setModel(event.target.value)} /></label></>}
    {allowed.length > 0 && !current && <p className="ui-help" role="status">{agent.name}’s model is no longer allowed in this workspace, so it can’t take new requests. Choose another.</p>}
    <div className="agent-save-row">
      <Button primary disabled={mutation.busy || !changed || !model.trim()} onClick={save}>Save model</Button>
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </div>
    <p className="ui-help">Changes apply to new requests. The workspace default is {directory.default_model}.</p>
  </div>;
}
