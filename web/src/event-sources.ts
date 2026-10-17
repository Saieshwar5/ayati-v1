import { api, post } from './types';
import { element } from './view';

type Field = { type?: string; required?: boolean; default?: unknown; description?: string; options?: string[]; enum?: string[] };
type EventType = { slug: string; name: string; description: string; version: string; config: Record<string, Field> };

export async function installEventSourceForm(container: HTMLElement, connection: string, saved: () => Promise<void>): Promise<void> {
  const data = await api<{ types: EventType[] }>(`/connections/${connection}/event-types`);
  container.replaceChildren(element('h3', '', 'Watch an event'));
  if (!data.types.length) { container.append(element('p', '', 'No supported events were returned for this connection.')); return; }
  const form = document.createElement('form');
  const select = document.createElement('select'); select.setAttribute('aria-label', 'Event to watch');
  for (const type of data.types) { const option = document.createElement('option'); option.value = type.slug; option.textContent = type.name || type.slug; select.append(option); }
  const description = element('p');
  const fields = element('div');
  const status = element('p'); status.setAttribute('role', 'status');
  const submit = element('button', 'files-button', 'Start watching'); submit.type = 'submit';
  form.append(select, description, fields, element('p', '', 'Composio watches this account and delivers events to Ayati. A trigger rule below decides what this agent should do. Pausing one agent’s rule does not stop a shared event source.'), status, submit);
  container.append(form);
  function draw(): void {
    fields.replaceChildren(); submit.disabled = false;
    const type = data.types.find(t => t.slug === select.value)!; description.textContent = type.description;
    const schema = type.config as unknown as { type?: string; properties?: Record<string, Field> };
    const config = schema.type === 'object' ? schema.properties : type.config;
    for (const [key, field] of Object.entries(config ?? {})) {
      if (!['string', 'number', 'integer', 'boolean', 'enum'].includes(field.type ?? 'string')) {
        fields.append(element('p', '', `This event needs a ${key} setting that is not supported by this editor yet.`)); submit.disabled = true; continue;
      }
      const label = document.createElement('label'); label.textContent = key.replaceAll('_', ' ');
      const options = field.options ?? field.enum;
      const input = options ? document.createElement('select') : document.createElement('input');
      input.name = key; input.required = !!field.required;
      if (input instanceof HTMLSelectElement) for (const value of options!) { const option = document.createElement('option'); option.value = value; option.textContent = value; input.append(option); }
      else { input.type = field.type === 'boolean' ? 'checkbox' : ['number', 'integer'].includes(field.type ?? '') ? 'number' : 'text'; input.maxLength = 1000; if (field.type === 'number') input.step = 'any'; }
      input.dataset.valueType = field.type ?? 'string';
      if (field.default !== undefined) { if (input instanceof HTMLInputElement && input.type === 'checkbox') input.checked = field.default === true; else input.value = String(field.default); }
      label.append(input); fields.append(label); if (field.description) fields.append(element('small', '', field.description));
    }
  }
  select.onchange = draw; draw();
  form.onsubmit = async event => {
    event.preventDefault(); if (submit.disabled) return;
    const type = data.types.find(t => t.slug === select.value)!;
    const config: Record<string, unknown> = {};
    for (const field of fields.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input,select')) {
      if (field.dataset.valueType === 'boolean') config[field.name] = (field as HTMLInputElement).checked;
      else if (field.value) config[field.name] = ['number', 'integer'].includes(field.dataset.valueType ?? '') ? Number(field.value) : field.value;
    }
    submit.disabled = true; select.disabled = true; status.textContent = 'Registering event source…';
    try {
      await post(`/connections/${connection}/event-sources/create`, { slug: type.slug, version: type.version, config });
      status.textContent = 'Event source registered. Choose it in the trigger form.'; await saved();
    } catch (error) { status.textContent = `${(error as Error).message} Refresh subscriptions before trying again; the source may already exist.`; }
    finally { submit.disabled = false; select.disabled = false; }
  };
}
