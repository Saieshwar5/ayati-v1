import { storageScope, workspaceId } from './workspace-context';
import './style.css';
import { currentView } from './view-lifetime';
import { type AgentEvent, type Artifact, type Snapshot } from './types';
import { element, renderConversation } from './view';
import { applyDelta, mergeSnapshot } from './conversation-state';
import { PendingSubmission } from './submission';
import { workPanels } from './work-panels';
import { installBrowser } from './browser';
import { installMemory } from './memory';
import { installAgentDetails } from './agent-details';
import './agent-workspace.css';
import './agent-studio.css';
import { installStudioTools } from './agent-studio';
import { installArtifacts } from './agent-artifacts';
import { installAgentCreations } from './agent-creations';
import { installConversationIntent } from './conversation-intent';

export function installChat(): (profile: import('./agents').Profile) => void {
  const view = currentView();
  const { api, post, apiUrl } = view;
  const selectedAgent = view.agent;
  const app = view.querySelector<HTMLDivElement>('#app')!;
  app.innerHTML = `
    <div class="header" role="toolbar" aria-label="Agent tools">
      <div class="identity"><span class="avatar" aria-hidden="true">R</span>
        <div><strong>Relay</strong><span class="connection" id="connection">Connecting…</span></div>
      </div>
      <button class="files-button" id="agent-sidebar-toggle" aria-label="Hide agent list" aria-expanded="true"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M9 4v16"/></svg></button>
      <button class="files-button agent-list-toggle" id="agent-list-toggle" aria-label="Choose agent"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M9 4v16"/></svg></button>
      <button class="files-button" id="browser-button" hidden>Browser</button>
      <button class="files-button" id="files-button" aria-expanded="false" aria-controls="files-panel">Artifacts <span id="file-count">0</span></button>
      <div class="chat-tool-links"></div>
    </div>
    <main class="main">
      <section class="chat" aria-label="Conversation with Relay">
        <div class="notice" id="notice" role="status" hidden></div>
        <div class="conversation" id="conversation" role="log" aria-label="Messages"></div>
        <div class="composer-area">
          <div class="attachments" id="attachments"></div>
          <form id="composer" class="composer">
            <button type="button" class="icon-button attach" id="attach" aria-label="Attach a file">＋</button>
            <textarea id="input" rows="1" placeholder="Ask Relay anything…" aria-label="Message Relay" maxlength="16000"></textarea>
            <button type="button" class="stop-button" id="stop" hidden>Stop</button>
            <button type="submit" class="send-button" id="send" aria-label="Send message">↑</button>
          </form>
        </div>
      </section>
      <dialog class="files-panel" id="files-panel" aria-label="Artifacts" hidden>
        <div class="panel-header"><h2>Artifacts</h2><button class="icon-button" id="close-files" aria-label="Close files">×</button></div>
        <p class="panel-description">Created, updated and attached here. Text files can be read; other formats are stored for download.</p>
        <div id="agent-creations-slot"></div><div id="artifact-preview" hidden></div><div id="files-list" class="files-list"></div>
      </dialog>
    </main>
    <dialog id="browser-panel" class="browser-panel" aria-label="Agent browser" hidden>
      <div class="panel-header"><h2>Agent browser</h2><button id="browser-close" class="icon-button" aria-label="Close browser panel">×</button></div>
      <p id="browser-message" role="status"></p>
      <div class="browser-controls"><button id="browser-take" class="text-button">Take control</button><button id="browser-resume" class="text-button" disabled>Hand back to agent</button><button id="browser-finish" class="text-button" disabled>Close browser session</button></div>
      <p>Closing the browser session discards unfinished forms. Saved logins remain.</p>
      <canvas id="browser-canvas" tabindex="0" aria-label="Live agent browser. Take control to interact." width="1280" height="720"></canvas>
      <div id="browser-owner-input" hidden>
        <form id="browser-navigate"><label>Website <input id="browser-address" type="url" placeholder="https://example.com" required></label><button>Open</button></form>
        <form id="browser-text"><label>Type into the selected browser field <input id="browser-private-text" type="password" autocomplete="off"></label><button>Send text</button></form>
        <p>Use the browser view to select a field first. This text is sent only to your browser session.</p>
      </div>
    </dialog>
    <input id="file-input" type="file" hidden />`;
  view.querySelector('.chat')!.prepend(view.querySelector('.header')!);
  installBrowser();
  installMemory();

  const conversation = view.querySelector<HTMLDivElement>('#conversation')!;
  const input = view.querySelector<HTMLTextAreaElement>('#input')!;
  const notice = view.querySelector<HTMLDivElement>('#notice')!;
  const connection = view.querySelector<HTMLSpanElement>('#connection')!;
  const send = view.querySelector<HTMLButtonElement>('#send')!;
  const stop = view.querySelector<HTMLButtonElement>('#stop')!;
  const filesButton = view.querySelector<HTMLButtonElement>('#files-button')!;
  const filesPanel = view.querySelector<HTMLDialogElement>('#files-panel')!;
  const fileInput = view.querySelector<HTMLInputElement>('#file-input')!;
  let state: Snapshot = { messages: [], tasks: [], artifacts: [] };
  let modelReady = false;
  let connected = false;
  let sending = false;
  let syncing = false;
  let syncAgain = false;
  let attachments: { name: string; id: string }[] = [];
  let uploading = false;
  let browserStorage: Storage | undefined;
  try { browserStorage = window.sessionStorage; } catch { /* Private browser modes may restrict it. */ }
  const pendingSubmission = new PendingSubmission(browserStorage, storageScope(selectedAgent));
  const readingKey = storageScope(`ayati.reading.${selectedAgent}`);
  let restoreReading = true;
  view.cleanup(() => {
    try { browserStorage?.setItem(readingKey, String(conversation.scrollTop)); } catch { /* Optional reading position. */ }
  });
  const draftKey = storageScope(`ayati.draft.${selectedAgent}`);
  try { input.value = browserStorage?.getItem(draftKey) ?? pendingSubmission.draft(); }
  catch { input.value = pendingSubmission.draft(); }
  try {
    const saved: unknown = JSON.parse(browserStorage?.getItem(storageScope(`ayati.attachments.${selectedAgent}`)) ?? '[]');
    if (Array.isArray(saved)) attachments = saved.filter(f => f && typeof f.id === 'string' && typeof f.name === 'string').slice(0, 8);
  } catch { attachments = []; }
  const updateDetails = installAgentDetails();
  const showProfile = (profile: import('./agents').Profile): void => {
    if (!view.active || profile.id !== selectedAgent) return;
    const name = profile.id === 'local-agent' && profile.name === 'Ayati' ? 'Relay' : profile.name;
    updateDetails(profile);
    view.querySelector('.identity strong')!.textContent = name;
    view.querySelector('.identity .avatar')!.textContent = name.split(/\s+/).map(part => part[0]).slice(0, 2).join('').toUpperCase();
    input.setAttribute('aria-label', `Message ${name}`); input.placeholder = `Message ${name}…`;
    view.querySelector('.chat')!.setAttribute('aria-label', `Conversation with ${name}`);
    document.title = name === 'Relay' ? 'Chat · Relay' : `${name} · Relay`;
  };
  const sidebarButton = view.querySelector<HTMLButtonElement>('#agent-sidebar-toggle')!;
  sidebarButton.setAttribute('aria-controls', 'agents-sidebar');
  const contactsHidden = !!view.root.closest('.contacts-collapsed');
  sidebarButton.setAttribute('aria-expanded', String(!contactsHidden));
  sidebarButton.setAttribute('aria-label', contactsHidden ? 'Show agent list' : 'Hide agent list');
  view.querySelector('#agent-list-toggle')!.setAttribute('aria-haspopup', 'dialog');
  const renderArtifacts = installArtifacts();
  installStudioTools();


  function tell(text: string): void { notice.textContent = text; notice.hidden = !text; }

  function draw(): void {
    if (!view.active) return;
    renderConversation(conversation, state, view.querySelector('.identity strong')?.textContent ?? 'Relay');
    renderArtifacts(state.artifacts);
    view.querySelector('#file-count')!.textContent = String(state.artifacts.filter((f) => f.kind !== 'log').length);
    const active = state.tasks.find(task => task.status === 'running');
    stop.hidden = !active;
    if (connected) connection.textContent = state.permissions?.length ? 'Waiting for your approval' : active?.detail || (active ? 'Working…' : 'Ready');
    send.disabled = !modelReady || sending || uploading;
    view.querySelector<HTMLButtonElement>('#attach')!.disabled = uploading || sending;
  }

  async function sync(): Promise<void> {
    if (syncing) { syncAgain = true; return; }
    syncing = true;
    try {
      const snapshot = await api<Snapshot>('/state');
      state = mergeSnapshot(state, snapshot);
      draw();
      if (restoreReading) {
        restoreReading = false;
        try { const saved = browserStorage?.getItem(readingKey); if (saved !== null && saved !== undefined) conversation.scrollTop = Number(saved) || 0; } catch { /* Optional reading position. */ }
      }
    } catch { connection.textContent = 'Reconnecting…'; }
    finally {
      syncing = false;
      if (syncAgain) { syncAgain = false; void sync(); }
    }
  }

  function showAttachments(): void {
    try { browserStorage?.setItem(storageScope(`ayati.attachments.${selectedAgent}`), JSON.stringify(attachments)); } catch { /* Optional storage. */ }
    const container = view.querySelector<HTMLDivElement>('#attachments')!;
    container.replaceChildren(...attachments.map((attachment, index) => {
      const chip = element('button', 'attachment-chip', `${attachment.name} ×`);
      chip.type = 'button';
      chip.title = 'Remove from this message';
      chip.onclick = () => { attachments.splice(index, 1); showAttachments(); };
      return chip;
    }));
  }

  view.querySelector('#composer')!.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (uploading || sending || !modelReady || (!input.value.trim() && !attachments.length)) return;
    sending = true;
    draw();
    const content = creationContent(input.value.trim());
    const submission = pendingSubmission.prepare(content, attachments.map(f => f.id));
    try {
      await post('/messages', submission);
      pendingSubmission.acknowledge(submission.request_id);
      input.value = '';
      try { browserStorage?.removeItem(draftKey); } catch { /* Storage is optional. */ }
      input.style.height = '';
      attachments = [];
      showAttachments();
      tell('');
      await sync();
    } catch (error) {
      tell(error instanceof TypeError
        ? 'The connection was lost while sending. Your draft is saved; retry the same message safely.'
        : (error as Error).message);
    }
    finally { sending = false; draw(); input.focus(); }
  });

  input.addEventListener('keydown', (event) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      view.querySelector<HTMLFormElement>('#composer')!.requestSubmit();
    }
  });
  function resizeComposer(): void {
    input.style.height = 'auto';
    input.style.height = `${Math.min(input.scrollHeight, 140)}px`;
  }
  input.addEventListener('input', () => {
    try { browserStorage?.setItem(draftKey, input.value); } catch { /* Storage is optional. */ }
    resizeComposer();
  });
  view.onWindow('resize', resizeComposer);
  const resizeFrame = requestAnimationFrame(resizeComposer);
  view.cleanup(() => cancelAnimationFrame(resizeFrame));
  const creationContent = installConversationIntent(input);
  installAgentCreations();

  async function stopTask(id: string): Promise<void> {
    try { await post(`/tasks/${id}/stop`); await sync(); }
    catch (error) { tell((error as Error).message); }
  }
  stop.onclick = () => {
    const active = state.tasks.find((task) => task.status === 'running');
    if (active) void stopTask(active.id);
  };
  conversation.addEventListener('click', (event) => {
    const decision = (event.target as HTMLElement).closest<HTMLButtonElement>('[data-permission]');
    if (decision?.dataset.permission) {
      decision.disabled = true;
      void post(`/permissions/${decision.dataset.permission}`, { allow: decision.dataset.decision === 'allow' })
        .then(() => sync()).catch((error: Error) => { tell(error.message); decision.disabled = false; });
      return;
    }
    const target = (event.target as HTMLElement).closest<HTMLElement>('[data-stop-task]');
    if (target?.dataset.stopTask) void stopTask(target.dataset.stopTask);
  });

  const filesWork = workPanels.register(filesPanel, filesButton);
  view.querySelector<HTMLButtonElement>('#close-files')!.onclick = () => filesWork.close();
  view.querySelector<HTMLButtonElement>('#attach')!.onclick = () => fileInput.click();
  fileInput.onchange = async () => {
    if (attachments.length >= 8) { tell('Send up to 8 files per message.'); return; }
    const file = fileInput.files?.[0];
    if (!file) return;
    if (file.size > 25 * 1024 * 1024) { tell('Please choose a file smaller than 25 MB.'); return; }
    const body = new FormData();
    body.append('file', file);
    uploading = true; draw();
    tell('Adding your file…');
    try {
      const data = await api<{ artifact: Artifact; workspace_path: string }>('/files', { method: 'POST', body });
      attachments.push({ name: file.name, id: data.artifact.id });
      showAttachments();
      tell('');
      await sync();
      input.focus();
    } catch (error) { tell((error as Error).message); }
    finally { uploading = false; fileInput.value = ''; draw(); }
  };

  showAttachments();
  const events = new EventSource(apiUrl('/events'));
  view.cleanup(() => events.close());
  events.onopen = () => { connected = true; draw(); void sync(); };
  events.onerror = () => { connected = false; connection.textContent = 'Reconnecting…'; };
  events.onmessage = (event) => {
    if (!view.active) return;
    const update = JSON.parse(event.data) as AgentEvent;
    if (update.kind === 'delta' && update.message_id && update.text) {
      if (!applyDelta(state, update)) { void sync(); return; }
      renderConversation(conversation, state, view.querySelector('.identity strong')?.textContent ?? 'Relay');
    } else { void sync(); }
  };

  void api<{ model_ready: boolean; file_uploads_supported?: boolean }>('/health').then((health) => {
    modelReady = health.model_ready;
    view.querySelector<HTMLButtonElement>('#attach')!.hidden = health.file_uploads_supported === false;
    if (!modelReady) tell(workspaceId ? 'This workspace needs its own agent environment. Create an agent from the agent list to get started; existing local environments are kept separate.' : 'Relay needs its AI connection. See the project README to configure it on this computer.');
    else if (pendingSubmission.draft()) tell('Your last message is saved here. You can retry the same message safely.');
    draw();
  }).catch(() => tell('Relay is not reachable. Start the local service and reload.'));
  void sync();
  return showProfile;

}
