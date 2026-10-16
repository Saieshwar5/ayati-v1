import { api, post } from './types';
import { element } from './view';

type Setup = { provider: string; model: string; execution: string; supported: boolean; can_test: boolean;
  checks: { label: string; ok: boolean; help: string }[] };

export function installSetup(): void {
  const root = document.querySelector('#setup-facts')!;
  const status = document.querySelector('#setup-status')!;
  const refresh = element('button', 'files-button', 'Refresh setup');
  const details = element('div');
  const consent = document.createElement('input'); consent.type = 'checkbox';
  const label = element('label', 'setup-consent');
  label.append(consent, document.createTextNode('Send “Reply with OK.” to the configured provider and model. This may incur normal API charges. No conversations, files or tools are sent.'));
  const test = element('button', 'files-button', 'Test provider connection'); test.disabled = true;
  const result = element('p'); result.setAttribute('role', 'status');
  const scope = element('p', 'page-subtitle', 'These checks describe the default agent. A connection test does not run Goose, boot a VM or validate agent tools. Edit names and responsibilities from Agent details. Model and tool settings remain read-only.');
  root.append(details, refresh, scope, label, test, result);
  let ready = false; let busy = false;
  consent.onchange = () => { test.disabled = busy || !ready || !consent.checked; };
  async function load(): Promise<void> {
    refresh.disabled = true; ready = false; test.disabled = true;
    status.textContent = 'Checking local setup…';
    try {
      const setup = await api<Setup>('/setup');
      details.replaceChildren();
      for (const [name, value] of [['Provider', setup.provider], ['Model', setup.model], ['Execution', setup.execution]]) {
        const row = element('div', 'settings-row'); row.append(element('span', '', name), element('span', '', value)); details.append(row);
      }
      for (const check of setup.checks) {
        const row = element('div', 'setup-check');
        row.append(element('strong', '', `${check.ok ? '✓' : 'Needs attention:'} ${check.label}`));
        if (!check.ok) row.append(element('p', 'page-subtitle', check.help));
        details.append(row);
      }
      ready = setup.can_test;
      status.textContent = !setup.supported ? 'Switch to the Goose runtime to use this setup guide.' : setup.checks.every(check => check.ok)
        ? 'Local checks passed. Provider availability has not been tested.' : 'Complete the items below, restart Ayati after configuration changes, then refresh.';
    } catch { status.textContent = 'Setup could not be read. Check that Ayati is running, then refresh.'; }
    finally { refresh.disabled = busy; test.disabled = busy || !ready || !consent.checked; }
  }
  refresh.onclick = () => { result.textContent = ''; consent.checked = false; void load(); };
  test.onclick = async () => {
    if (!ready || busy || !consent.checked) return;
    busy = true; test.disabled = true; refresh.disabled = true; consent.disabled = true;
    result.textContent = 'Testing once… This can take up to 30 seconds.';
    try {
      const response = await post<{ message: string }>('/setup/test', { confirm: true });
      result.textContent = `${response.message} Checked at ${new Date().toLocaleTimeString()}.`;
    } catch (error) { result.textContent = `${error instanceof Error ? error.message : 'Result unavailable.'} No automatic retry will be sent.`; }
    finally { busy = false; consent.checked = false; consent.disabled = false; refresh.disabled = false; test.disabled = true; }
  };
  void load();
}
