import { useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { backupChoices, colours, hours, providerName, providers, saveSettings, timeZones, type ModelChoice, type WorkspaceSettings } from './workspace-model';
import type { Catalog } from '../agents/catalog-model';

/** Colour, time zone, approval backup and models in one short form
 * (designs/settings 4). A failed save keeps the draft. With Cuckoo's model catalog,
 * admins choose from its models and never name a provider (MG1). */
export function WorkspaceRules({ initial, catalog, saved }: { initial: WorkspaceSettings; catalog?: Catalog; saved: (next: WorkspaceSettings) => void }) {
  const [zones] = useState(timeZones);
  const [timezone, setTimezone] = useState(initial.timezone ?? zones[0]);
  const [colour, setColour] = useState(initial.colour);
  const [backup, setBackup] = useState(initial.approval_backup_hours);
  const [models, setModels] = useState<ModelChoice[]>(initial.models);
  const [preferred, setPreferred] = useState(initial.default_model);
  const [adding, setAdding] = useState<ModelChoice>({ provider: catalog ? '' : providers[0][0], model: '' });
  const catalogName = (choice: ModelChoice) => catalog?.models.find(m => m.id === choice.model && !choice.provider)?.name;
  const addable = catalog?.models.filter(m => m.status !== 'retiring' && !models.some(c => !c.provider && c.model === m.id)) ?? [];
  const mutation = useMutation();
  const add = () => {
    const model = adding.model.trim();
    if (!model || models.some(m => m.provider === adding.provider && m.model === model)) return;
    setModels(old => [...old, { provider: adding.provider, model }]);
    if (!preferred || !models.length) setPreferred(model);
    setAdding(old => ({ ...old, model: '' }));
  };
  const remove = (choice: ModelChoice) => {
    const rest = models.filter(m => m !== choice);
    setModels(rest);
    if (preferred === choice.model) setPreferred(rest[0]?.model ?? null);
  };
  const submit = () => void mutation.run(async () => {
    const next = await saveSettings({ timezone, colour, approval_backup_hours: backup, models, default_model: models.length ? preferred : null });
    saved(next);
    mutation.setStatus('Workspace settings saved.');
  });
  return <form className="ui-form settings-rules" aria-label="Workspace rules" onSubmit={event => { event.preventDefault(); submit(); }}>
    <fieldset className="ui-fieldset" disabled={mutation.busy}>
      <fieldset className="settings-colours"><legend>Colour</legend>
        {colours.map(([id, hex]) => <label key={id} className="settings-colour" style={{ background: hex }}>
          <input type="radio" name="colour" value={id} checked={colour === id} onChange={() => setColour(id)} aria-label={id} />
        </label>)}
      </fieldset>
      <label className="ui-field">Time zone <small>used by schedules and Home</small>
        <select value={timezone} onChange={event => setTimezone(event.target.value)}>
          {zones.map(zone => <option key={zone} value={zone}>{zone.replaceAll('_', ' ')}</option>)}</select></label>
      <label className="ui-field">Approval backup <small>when no agent manager has decided</small>
        <select value={backup} onChange={event => setBackup(Number(event.target.value))}>
          {backupChoices.map(n => <option key={n} value={n}>Admins can decide after {hours(n)}</option>)}</select></label>
      <fieldset className="settings-models"><legend>Models <small>which models agents may use</small></legend>
        {!models.length && <p className="ui-help">Any model is allowed. Add models to limit what agents can use.</p>}
        <ul>{models.map(choice => <li key={`${choice.provider}/${choice.model}`}>
          <label><input type="radio" name="default-model" checked={preferred === choice.model} onChange={() => setPreferred(choice.model)} />
            <span><strong>{catalogName(choice) ?? choice.model}</strong><small>{catalog ? catalogName(choice) ? choice.model : 'Not one of Cuckoo’s models · remove it' : providerName(choice.provider)}{preferred === choice.model ? ' · default for new agents' : ''}</small></span></label>
          <Button aria-label={`Remove ${choice.model}`} onClick={() => remove(choice)}>Remove</Button>
        </li>)}</ul>
        {catalog ? <div className="settings-model-add settings-model-add-catalog">
          <select aria-label="Model to add" value={adding.model} onChange={event => setAdding({ provider: '', model: event.target.value })}>
            <option value="">Choose a model…</option>
            {addable.map(m => <option key={m.id} value={m.id}>{m.name}</option>)}</select>
          <Button disabled={!adding.model} onClick={add}>Add model</Button>
        </div> : <div className="settings-model-add">
          <select aria-label="Provider" value={adding.provider} onChange={event => setAdding(old => ({ ...old, provider: event.target.value }))}>
            {providers.map(([id, name]) => <option key={id} value={id}>{name}</option>)}</select>
          <input aria-label="Model identifier" placeholder="Model identifier, e.g. gpt-5-mini" maxLength={200} value={adding.model}
            onChange={event => setAdding(old => ({ ...old, model: event.target.value.replace(/\s/g, '') }))}
            onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); add(); } }} />
          <Button disabled={!adding.model.trim()} onClick={add}>Add model</Button>
        </div>}
        <p className="ui-help">Agents already using a model you remove stop taking new requests until their owner picks an allowed one.</p>
      </fieldset>
      <Button type="submit" primary disabled={mutation.busy}>{mutation.busy ? 'Saving…' : 'Save changes'}</Button>
    </fieldset>
    <p role="status" className="ui-status">{mutation.status}</p>
  </form>;
}
