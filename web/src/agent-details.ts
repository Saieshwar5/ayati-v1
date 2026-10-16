import { installProfileEditor } from './agent-profile-editor';
import { element } from './view';
import type { Profile } from './agents';

export function installAgentDetails(): (profile: Profile) => void {
  const panel = document.createElement('dialog');
  panel.className = 'agent-details'; panel.id = 'agent-details';
  panel.setAttribute('aria-labelledby', 'details-title');
  panel.innerHTML = `<div class="panel-header"><h2 id="details-title">Agent details</h2><button class="icon-button" aria-label="Close agent details">×</button></div>
    <section><h3>Responsibilities</h3><p id="details-role"></p></section>
    <section><h3>Model</h3><p id="details-model"></p></section>
    <section><h3>Authority</h3><p>Tool actions follow the runtime’s approval checks. An approval applies only to its original request.</p></section>
    <section><h3>Tools and knowledge</h3><p id="details-tools"></p><div id="details-links"></div></section>
    <section><h3>Connections</h3><p>Business account connections and per-agent grants are still being built.</p></section>
    <p class="details-note">Model and enabled tools remain fixed. Name and responsibilities can be edited when this agent is idle.</p>`;
  document.querySelector('.main')!.append(panel);
  const toggle = element('button', 'files-button', 'Details'); toggle.id = 'details-button';
  toggle.setAttribute('aria-controls', panel.id); toggle.setAttribute('aria-expanded', 'false');
  document.querySelector('.header')!.append(toggle);
  const mobile = matchMedia('(max-width: 1000px)');
  function close(): void { panel.close(); toggle.setAttribute('aria-expanded', 'false'); toggle.focus(); }
  toggle.onclick = () => {
    if (panel.open) { close(); return; }
    if (mobile.matches) panel.showModal(); else panel.show();
    toggle.setAttribute('aria-expanded', 'true'); panel.querySelector('button')!.focus();
  };
  panel.querySelector('button')!.onclick = close;
  panel.addEventListener('close', () => { toggle.setAttribute('aria-expanded', 'false'); toggle.focus(); });
  panel.addEventListener('keydown', event => { if (event.key === 'Escape') { event.preventDefault(); close(); } });
  mobile.addEventListener('change', () => { if (panel.open) close(); });
  const updateEditor = installProfileEditor(panel);
  let previous = '';
  return profile => {
    const fingerprint = JSON.stringify(profile);
    if (previous === fingerprint) return;
    previous = fingerprint;
    updateEditor(profile);
    panel.querySelector('#details-role')!.textContent = profile.role || 'A general assistant for your everyday work.';
    panel.querySelector('#details-model')!.textContent = profile.model;
    panel.querySelector('#details-tools')!.textContent = `${profile.memory ? 'Sourced memory enabled' : 'Sourced memory disabled'} · ${profile.browser ? 'Browser enabled' : 'Browser disabled'}`;
    const links = panel.querySelector('#details-links')!; links.replaceChildren();
    for (const [label, id, enabled] of [['Memory', 'memory-button', profile.memory], ['Browser', 'browser-button', profile.browser], ['Files', 'files-button', true]] as const) {
      if (!enabled) continue;
      const button = element('button', 'files-button', label);
      button.onclick = () => { close(); document.querySelector<HTMLButtonElement>(`#${id}`)?.click(); };
      links.append(button);
    }
  };
}
