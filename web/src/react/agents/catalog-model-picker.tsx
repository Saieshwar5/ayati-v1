import { useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { saveAgent, type Agent, type Directory } from './model';
import { abilities, allowedModels, priceLine, statusLabel, type Catalog, type CatalogModel } from './catalog-model';

/** Choosing an agent's model from Cuckoo's catalog (MG1). People pick the model;
 * Cuckoo decides which provider serves it, so the saved provider is always empty. */
export function CatalogModelPicker({ agent, directory, catalog, saved }: { agent: Agent; directory: Directory; catalog: Catalog; saved: () => Promise<void> }) {
  const [model, setModel] = useState(agent.model);
  const mutation = useMutation();
  const offered = allowedModels(catalog, directory.models);
  const current = catalog.models.find(m => m.id === agent.model);
  const choosable = (id: string) => offered.some(m => m.id === id && m.status !== 'retiring');
  const replacement = current?.replacement ? catalog.models.find(m => m.id === current.replacement) : undefined;
  const changed = model !== agent.model || agent.settings.provider !== '';
  const save = () => void mutation.run(async () => {
    await saveAgent({ ...agent, model, people: undefined, managers: undefined, settings: { ...agent.settings, provider: '' } });
    mutation.setStatus('Model saved. New requests use it.');
    await saved();
  });
  return <div className="ui-form">
    <fieldset className="catalog-models" disabled={mutation.busy}>
      <legend className="visually-hidden">Model</legend>
      <ModelNotice agent={agent} current={current} replacement={replacement} allowed={choosable(agent.model)} />
      {!offered.length && <p className="ui-help">No models are on offer right now.</p>}
      <CatalogModelChoices models={offered} name={`model-${agent.id}`} value={model} saved={agent.model}
        workspaceDefault={directory.default_model} choose={setModel} />
    </fieldset>
    <div className="agent-save-row">
      <Button primary disabled={mutation.busy || !changed || !choosable(model)} onClick={save}>Save model</Button>
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </div>
    <p className="ui-help">Changes apply to new requests. Cuckoo chooses who runs the model and switches if one is down.</p>
  </div>;
}

/** Why the agent's saved model can't take new requests, when it can't. */
function ModelNotice({ agent, current, replacement, allowed }: { agent: Agent; current?: CatalogModel; replacement?: CatalogModel; allowed: boolean }) {
  if (!current) return <p className="ui-help" role="status">{agent.model} isn’t offered any more, so {agent.name} can’t take new requests. Choose a model.</p>;
  if (current.status === 'retiring') return <p className="ui-help" role="status">{current.name} is being retired. Choose {replacement?.name ?? 'another model'} to keep {agent.name} working.</p>;
  if (!allowed) return <p className="ui-help" role="status">{current.name} is no longer allowed in this workspace, so {agent.name} can’t take new requests. Choose another.</p>;
  return null;
}

/** The catalog models to choose from, shared by the Environment tab and Create. A
 * retiring model can only stay selected when it is the one already saved. */
export function CatalogModelChoices({ models, name, value, saved, workspaceDefault, choose }: {
  models: CatalogModel[]; name: string; value: string; saved?: string; workspaceDefault: string; choose: (id: string) => void;
}) {
  return <ul>{models.map(item => <li key={item.id}>
    <ModelOption item={item} name={name} checked={value === item.id} workspaceDefault={item.id === workspaceDefault}
      disabled={item.status === 'retiring' && item.id !== saved} choose={() => choose(item.id)} />
  </li>)}</ul>;
}

function ModelOption({ item, name, checked, disabled, workspaceDefault, choose }: {
  item: CatalogModel; name: string; checked: boolean; disabled: boolean; workspaceDefault: boolean; choose: () => void;
}) {
  const status = statusLabel(item);
  const details = [...abilities(item), priceLine(item)].filter(Boolean).join(' · ');
  return <label>
    <input type="radio" name={name} value={item.id} checked={checked} disabled={disabled} onChange={choose} />
    <span>
      <strong>{item.name}</strong>
      {(status || workspaceDefault || item.tags.length > 0) && <span className="catalog-model-tags">
        {status && <span className="ui-badge">{status}</span>}
        {workspaceDefault && <span className="ui-badge">Workspace default</span>}
        {item.tags.map(tag => <span key={tag} className="ui-badge">{tag}</span>)}
      </span>}
      {item.description && <small>{item.description}</small>}
      {details && <small>{details}</small>}
    </span>
  </label>;
}
