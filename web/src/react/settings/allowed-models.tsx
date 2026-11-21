import { useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { loadCatalog } from '../agents/catalog-model';
import { backupChoices, hours, loadSettings, providerName, providers, saveSettings, settingsInput, type ModelChoice, type SettingsInput } from './workspace-model';

/** Agent rules → Models and Approvals, for admins: which models agents may use, the
 * default for new agents, and when admins may decide an approval no owner answered.
 * Each change saves at once; removing a model asks first because agents on it stop.
 * With Cuckoo's model catalog, admins choose from its models and never name a provider (MG1). */
export function AllowedModels() {
  const resource = useResource(loadSettings);
  // Loaded alongside the settings; without it the model list keeps naming providers.
  const catalogResource = useResource(loadCatalog);
  const mutation = useMutation();
  const data = resource.data;
  const catalog = catalogResource.data?.catalog_on ? catalogResource.data : undefined;
  const [adding, setAdding] = useState<ModelChoice>({ provider: '', model: '' });
  if (resource.error) return <p role="alert">{resource.error}</p>;
  if (!data || !(catalogResource.data || catalogResource.error)) return <p role="status">Loading models…</p>;
  const models = data.models ?? [];
  const provider = catalog ? '' : adding.provider || providers[0][0];
  const catalogName = (choice: ModelChoice) => catalog?.models.find(m => m.id === choice.model && !choice.provider)?.name;
  const addable = catalog?.models.filter(m => m.status !== 'retiring' && !models.some(c => !c.provider && c.model === m.id)) ?? [];
  // `after` runs only once the change is saved, so a refused one keeps what was typed.
  const apply = (change: Partial<SettingsInput>, done: string, after?: () => void) => void mutation.run(async () => {
    resource.setData(await saveSettings(settingsInput(data, change)));
    after?.();
    mutation.setStatus(done);
  });
  const add = () => {
    const model = adding.model.trim();
    if (!model || models.some(m => m.provider === provider && m.model === model)) return;
    apply({ models: [...models, { provider, model }], default_model: data.default_model ?? model }, 'Model added.', () => setAdding({ provider, model: '' }));
  };
  const remove = (choice: ModelChoice) => {
    const name = catalogName(choice) ?? choice.model;
    if (!confirm(`Remove ${name}? Agents using it stop taking new requests until their owner picks an allowed model.`)) return;
    const rest = models.filter(m => m !== choice);
    apply({ models: rest, default_model: !rest.length ? null : data.default_model === choice.model ? rest[0].model : data.default_model }, `${name} removed.`);
  };
  return <>
    <section className="settings-models" aria-label="Models">
      <header className="settings-subsection-heading"><div><h3>Models</h3><p>The models agents may use. New agents start on the default.</p></div></header>
      {!models.length && <p className="ui-help">Any model is allowed. Add models to limit what agents can use.</p>}
      <fieldset className="ui-fieldset settings-rows" disabled={mutation.busy}>
        {models.map(choice => <div className="settings-row" key={`${choice.provider}/${choice.model}`}>
          <label className="settings-model"><input type="radio" name="default-model" checked={data.default_model === choice.model}
            onChange={() => apply({ default_model: choice.model }, `${catalogName(choice) ?? choice.model} is the default.`)} />
            <span><strong>{catalogName(choice) ?? choice.model}</strong>
              <small>{catalog ? catalogName(choice) ? choice.model : 'Not one of Cuckoo’s models · remove it' : providerName(choice.provider)}</small></span></label>
          {data.default_model === choice.model && <span className="ui-badge">Default</span>}
          <Button aria-label={`Remove ${choice.model}`} onClick={() => remove(choice)}>Remove</Button>
        </div>)}
        {catalog ? <div className="settings-model-add settings-model-add-catalog">
          <select aria-label="Model to add" value={adding.model} onChange={event => setAdding({ provider: '', model: event.target.value })}>
            <option value="">Choose a model…</option>
            {addable.map(m => <option key={m.id} value={m.id}>{m.name}</option>)}</select>
          <Button disabled={!adding.model} onClick={add}>Add model</Button>
        </div> : <div className="settings-model-add">
          <select aria-label="Provider" value={provider} onChange={event => setAdding(old => ({ ...old, provider: event.target.value }))}>
            {providers.map(([id, name]) => <option key={id} value={id}>{name}</option>)}</select>
          <input aria-label="Model identifier" placeholder="Model identifier, e.g. gpt-5-mini" maxLength={200} value={adding.model}
            onChange={event => setAdding(old => ({ ...old, model: event.target.value.replace(/\s/g, '') }))}
            onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); add(); } }} />
          <Button disabled={!adding.model.trim()} onClick={add}>Add model</Button>
        </div>}
      </fieldset>
    </section>
    <section className="settings-approvals" aria-label="Approvals"><h3>Approvals</h3>
      <fieldset className="ui-fieldset settings-rows" disabled={mutation.busy}>
        <div className="settings-row"><span><strong>If no owner of an agent decides</strong><small>Admins can step in after this long.</small></span>
          <select aria-label="Approval backup" value={data.approval_backup_hours} onChange={event => apply({ approval_backup_hours: Number(event.target.value) }, 'Saved.')}>
            {backupChoices.map(n => <option key={n} value={n}>After {hours(n)}</option>)}</select></div>
      </fieldset>
    </section>
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
  </>;
}
