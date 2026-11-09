import { useId, useState, type ReactNode } from 'react';
import { Button } from '../../ui';
import type { Agent, Directory } from '../model';
import { CatalogModelChoices } from '../catalog-model-picker';
import { abilities, allowedModels, priceLine, type Catalog } from '../catalog-model';

/** The model a new agent starts with, shown on the last create step. It starts on
 * the workspace default and stays collapsed; "Change" opens the models the admin
 * allows. Nothing here is required: the Environment tab changes it later too. */
export function CreateModelChoice({ agent, directory, needsTools, change }: {
  agent: Agent; directory: Directory; needsTools: boolean; change: (model: string, provider: string) => void;
}) {
  const catalog = directory.catalog?.catalog_on ? directory.catalog : undefined;
  return catalog
    ? <CatalogChoice agent={agent} directory={directory} catalog={catalog} needsTools={needsTools} change={model => change(model, '')} />
    : <ListedChoice agent={agent} directory={directory} change={change} />;
}

/** With Cuckoo's catalog: catalog models only, never a provider (MG1). */
function CatalogChoice({ agent, directory, catalog, needsTools, change }: {
  agent: Agent; directory: Directory; catalog: Catalog; needsTools: boolean; change: (model: string) => void;
}) {
  const offered = allowedModels(catalog, directory.models).filter(m => m.status !== 'retiring');
  const current = catalog.models.find(m => m.id === agent.model);
  const usesTools = offered.filter(m => m.supports.tools);
  const better = usesTools.find(m => m.id === directory.default_model) ?? usesTools[0];
  const display = agent.name.trim() || 'This agent';
  const warning = needsTools && current && !current.supports.tools && <p className="agent-create-model-warning" role="status">
    {current.name} can’t use tools, so {display} won’t be able to use the apps or tools you chose.
    {better && <> <Button onClick={() => change(better.id)}>Use {better.name} instead</Button></>}
  </p>;
  return <ModelBox name={current?.name ?? agent.model} isDefault={agent.model === directory.default_model}
    details={current ? [...abilities(current), priceLine(current)].filter(Boolean).join(' · ') : ''}
    choosable={offered.length > 1} warning={warning}>
    <CatalogModelChoices models={offered} name={`create-model-${agent.id}`} value={agent.model}
      workspaceDefault={directory.default_model} choose={change} />
  </ModelBox>;
}

/** Without a catalog: the admin's list of provider and model pairs. When the admin
 * listed none, any model is allowed and it is typed on the Environment tab later. */
function ListedChoice({ agent, directory, change }: { agent: Agent; directory: Directory; change: (model: string, provider: string) => void }) {
  const allowed = directory.models ?? [];
  const providerName = (id: string) => directory.providers.find(p => p.id === id)?.name ?? id;
  const key = (provider: string, model: string) => `${provider}/${model}`;
  return <ModelBox name={agent.model} isDefault={agent.model === directory.default_model}
    details={agent.settings.provider ? providerName(agent.settings.provider) : ''} choosable={allowed.length > 1}>
    <ul>{allowed.map(item => <li key={key(item.provider, item.model)}><label>
      <input type="radio" name={`create-model-${agent.id}`} value={key(item.provider, item.model)}
        checked={item.model === agent.model && item.provider === agent.settings.provider} onChange={() => change(item.model, item.provider)} />
      <span><strong>{item.model}</strong>
        <small>{providerName(item.provider)}{item.model === directory.default_model ? ' · workspace default' : ''}</small></span>
    </label></li>)}</ul>
  </ModelBox>;
}

/** The collapsed line and, once opened, the choices below it. */
function ModelBox({ name, isDefault, details, choosable, warning, children }: {
  name: string; isDefault: boolean; details: string; choosable: boolean; warning?: ReactNode; children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return <section className="agent-create-model" aria-labelledby={`${id}-title`}>
    <div className="agent-create-model-line">
      <span>
        <strong id={`${id}-title`}>Model</strong> · {name}
        {isDefault && <span className="ui-badge">Workspace default</span>}
        {details && <small>{details}</small>}
      </span>
      {choosable && <Button aria-expanded={open} aria-controls={`${id}-choices`} onClick={() => setOpen(!open)}>{open ? 'Done' : 'Change'}</Button>}
    </div>
    {open && <fieldset id={`${id}-choices`} className="catalog-models">
      <legend className="ui-help">Models your admin allows</legend>
      {children}
    </fieldset>}
    {warning}
    <p className="ui-help">You can change the model later on the Environment tab.</p>
  </section>;
}
