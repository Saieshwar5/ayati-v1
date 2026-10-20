import { currentView } from './view-lifetime';
import { workPanels } from './work-panels';
import { installSchedules } from './schedules';
import { installConnections } from './connections';
import { installProfileEditor } from './agent-profile-editor';
import { element } from './view';
import type { Profile } from './agents';

export function installAgentDetails(): (profile: Profile) => void {
  const view = currentView();
  const panel = document.createElement('dialog');
  panel.className = 'agent-details'; panel.id = 'agent-details';
  panel.setAttribute('aria-labelledby', 'details-title');
  panel.innerHTML = `<div class="panel-header"><h2 id="details-title">Agent details</h2><button class="icon-button" aria-label="Close agent details">×</button></div>
    <section><h3>Responsibilities</h3><p id="details-role"></p></section>
    <section><h3>Model</h3><p id="details-model"></p></section>
    <section><h3>Authority</h3><p>Tool actions follow the runtime’s approval checks. An approval applies only to its original request.</p></section>
    <section><h3>Tools and knowledge</h3><p id="details-tools"></p><div id="details-links"></div></section>
    <section id="agent-connections"></section>
    <p class="details-note">Settings can be edited when this agent is idle. Browser access requires a compatible VM.</p>`;
  view.querySelector('.main')!.append(panel);
  const toggle = element('button', 'files-button', 'Details'); toggle.id = 'details-button';
  toggle.setAttribute('aria-controls', panel.id); toggle.setAttribute('aria-expanded', 'false');
  view.querySelector('.header')!.append(toggle);
  const work = workPanels.register(panel, toggle);
  const close = (): void => work.close();
  panel.querySelector('button')!.onclick = close;
  const updateEditor = installProfileEditor(panel);
  const updateSchedules = installSchedules(panel);
  installConnections(panel.querySelector('#agent-connections')!, true);
  let previous = '';
  return profile => {
    for (const [tool, enabled] of [['memory', profile.memory], ['browser', profile.browser]] as const) {
      const button = view.querySelector<HTMLButtonElement>(`#${tool}-button`);
      if (button) button.hidden = !enabled;
      if (!enabled) {
        const toolPanel = view.querySelector<HTMLElement>(`#${tool}-panel`);
        if (toolPanel) workPanels.close(toolPanel.id);
        button?.setAttribute('aria-expanded', 'false');
      }
    }
    const fingerprint = JSON.stringify(profile);
    if (previous === fingerprint) return;
    previous = fingerprint;
    view.querySelectorAll('.work-panel-owner').forEach(node => { node.textContent = profile.name; });
    updateEditor(profile);
    updateSchedules(profile);
    panel.querySelector('#details-role')!.textContent = profile.role || 'A general assistant for your everyday work.';
    panel.querySelector('#details-model')!.textContent = profile.model;
    panel.querySelector('#details-tools')!.textContent = `${profile.memory ? 'Sourced memory enabled' : 'Sourced memory disabled'} · ${profile.browser ? 'Browser enabled' : 'Browser disabled'}`;
    const links = panel.querySelector('#details-links')!; links.replaceChildren();
    for (const [label, id, enabled] of [['Memory', 'memory-button', profile.memory], ['Browser', 'browser-button', profile.browser], ['Files', 'files-button', true]] as const) {
      if (!enabled) continue;
      const button = element('button', 'files-button', label);
      button.onclick = () => { close(); view.querySelector<HTMLButtonElement>(`#${id}`)?.click(); };
      links.append(button);
    }
  };
}
