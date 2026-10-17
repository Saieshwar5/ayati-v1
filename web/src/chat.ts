import './style.css';
import { installAgents } from './agents';
import { api, post, apiUrl, selectedAgent, type AgentEvent, type Artifact, type Snapshot } from './types';
import { element, renderConversation, renderFiles } from './view';
import { applyDelta, mergeSnapshot } from './conversation-state';
import { PendingSubmission } from './submission';
import { workPanels } from './work-panels';
import { installBrowser } from './browser';
import { installMemory } from './memory';
import { installAgentDetails } from './agent-details';
import './agent-workspace.css';
import { installAgentCreations } from './agent-creations';
import { installConversationIntent } from './conversation-intent';

const app = document.querySelector<HTMLDivElement>('#app')!;
app.innerHTML = `
  <header class="header">
    <div class="identity"><span class="avatar" aria-hidden="true">a</span>
      <div><strong>Ayati</strong><span class="connection" id="connection">Connecting…</span></div>
    </div>
    <button class="files-button" id="agent-sidebar-toggle" aria-label="Hide agent list" aria-expanded="true"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M9 4v16"/></svg></button>
    <button class="files-button agent-list-toggle" id="agent-list-toggle" aria-label="Choose agent"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M9 4v16"/></svg></button>
    <button class="files-button" id="browser-button" hidden>Browser</button>
    <div class="chat-tool-links">
    <button class="files-button" id="files-button" aria-expanded="false" aria-controls="files-panel">Files <span id="file-count">0</span></button>
    </div>
  </header>
  <main class="main">
    <section class="chat" aria-label="Conversation with Ayati">
      <div class="notice" id="notice" role="status" hidden></div>
      <div class="conversation" id="conversation" role="log" aria-label="Messages"></div>
      <div class="composer-area">
        <div class="attachments" id="attachments"></div>
        <form id="composer" class="composer">
          <button type="button" class="icon-button attach" id="attach" aria-label="Attach a file">＋</button>
          <textarea id="input" rows="1" placeholder="Ask Ayati anything…" aria-label="Message Ayati" maxlength="16000"></textarea>
          <button type="button" class="stop-button" id="stop" hidden>Stop</button>
          <button type="submit" class="send-button" id="send" aria-label="Send message">↑</button>
        </form>
      </div>
    </section>
    <dialog class="files-panel" id="files-panel" aria-label="Your files" hidden>
      <div class="panel-header"><h2>Your files</h2><button class="icon-button" id="close-files" aria-label="Close files">×</button></div>
      <p class="panel-description">Originals and new versions, ready to download.</p>
      <div id="files-list" class="files-list"></div>
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
document.querySelector('.chat')!.prepend(document.querySelector('.header')!);
installBrowser();
installMemory();

const conversation = document.querySelector<HTMLDivElement>('#conversation')!;
const input = document.querySelector<HTMLTextAreaElement>('#input')!;
const notice = document.querySelector<HTMLDivElement>('#notice')!;
const connection = document.querySelector<HTMLSpanElement>('#connection')!;
const send = document.querySelector<HTMLButtonElement>('#send')!;
const stop = document.querySelector<HTMLButtonElement>('#stop')!;
const filesButton = document.querySelector<HTMLButtonElement>('#files-button')!;
const filesPanel = document.querySelector<HTMLDialogElement>('#files-panel')!;
const fileInput = document.querySelector<HTMLInputElement>('#file-input')!;
let state: Snapshot = { messages: [], tasks: [], artifacts: [] };
let modelReady = false;
let connected = false;
let sending = false;
let syncing = false;
let syncAgain = false;
let attachments: { name: string; path: string }[] = [];
let browserStorage: Storage | undefined;
try { browserStorage = window.sessionStorage; } catch { /* Private browser modes may restrict it. */ }
const pendingSubmission = new PendingSubmission(browserStorage, selectedAgent);
const draftKey = `ayati.draft.${selectedAgent}`;
try { input.value = browserStorage?.getItem(draftKey) ?? pendingSubmission.draft(); }
catch { input.value = pendingSubmission.draft(); }
installAgents(installAgentDetails());
document.querySelector<HTMLButtonElement>('#agent-sidebar-toggle')!.onclick = event => {
  const hidden = document.querySelector('.main')!.classList.toggle('agent-list-collapsed');
  const button = event.currentTarget as HTMLButtonElement;
  button.setAttribute('aria-expanded', String(!hidden));
  button.setAttribute('aria-label', hidden ? 'Show agent list' : 'Hide agent list');
};

function tell(text: string): void { notice.textContent = text; notice.hidden = !text; }

function draw(): void {
  renderConversation(conversation, state, document.querySelector('.identity strong')?.textContent ?? 'Ayati');
  renderFiles(document.querySelector('#files-list')!, state.artifacts, selectedAgent);
  document.querySelector('#file-count')!.textContent = String(state.artifacts.filter((f) => f.kind !== 'log').length);
  const active = state.tasks.find(task => task.status === 'running');
  stop.hidden = !active;
  if (connected) connection.textContent = state.permissions?.length ? 'Waiting for your approval' : active?.detail || (active ? 'Working…' : 'Ready');
  send.disabled = !modelReady || sending;
}

async function sync(): Promise<void> {
  if (syncing) { syncAgain = true; return; }
  syncing = true;
  try {
    const snapshot = await api<Snapshot>('/state');
    state = mergeSnapshot(state, snapshot);
    draw();
  } catch { connection.textContent = 'Reconnecting…'; }
  finally {
    syncing = false;
    if (syncAgain) { syncAgain = false; void sync(); }
  }
}

function showAttachments(): void {
  const container = document.querySelector<HTMLDivElement>('#attachments')!;
  container.replaceChildren(...attachments.map((attachment, index) => {
    const chip = element('button', 'attachment-chip', `${attachment.name} ×`);
    chip.type = 'button';
    chip.title = 'Remove from this message';
    chip.onclick = () => { attachments.splice(index, 1); showAttachments(); };
    return chip;
  }));
}

document.querySelector('#composer')!.addEventListener('submit', async (event) => {
  event.preventDefault();
  if (sending || !modelReady || (!input.value.trim() && !attachments.length)) return;
  sending = true;
  draw();
  const content = creationContent(input.value.trim()) + attachments.map((f) => `\n\nAttached file: ${f.path}`).join('');
  const submission = pendingSubmission.prepare(content);
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
    document.querySelector<HTMLFormElement>('#composer')!.requestSubmit();
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
window.addEventListener('resize', resizeComposer);
requestAnimationFrame(resizeComposer);
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
document.querySelector<HTMLButtonElement>('#close-files')!.onclick = () => filesWork.close();
document.querySelector<HTMLButtonElement>('#attach')!.onclick = () => fileInput.click();
fileInput.onchange = async () => {
  const file = fileInput.files?.[0];
  if (!file) return;
  if (file.size > 25 * 1024 * 1024) { tell('Please choose a file smaller than 25 MB.'); return; }
  const body = new FormData();
  body.append('file', file);
  tell('Adding your file…');
  try {
    const data = await api<{ artifact: Artifact; workspace_path: string }>('/files', { method: 'POST', body });
    attachments.push({ name: file.name, path: data.workspace_path });
    showAttachments();
    tell('');
    await sync();
    input.focus();
  } catch (error) { tell((error as Error).message); }
  finally { fileInput.value = ''; }
};

const events = new EventSource(apiUrl('/events'));
events.onopen = () => { connected = true; draw(); void sync(); };
events.onerror = () => { connected = false; connection.textContent = 'Reconnecting…'; };
events.onmessage = (event) => {
  const update = JSON.parse(event.data) as AgentEvent;
  if (update.kind === 'delta' && update.message_id && update.text) {
    if (!applyDelta(state, update)) { void sync(); return; }
    renderConversation(conversation, state, document.querySelector('.identity strong')?.textContent ?? 'Ayati');
  } else { void sync(); }
};

void api<{ model_ready: boolean; file_uploads_supported?: boolean }>('/health').then((health) => {
  modelReady = health.model_ready;
  document.querySelector<HTMLButtonElement>('#attach')!.hidden = health.file_uploads_supported === false;
  if (!modelReady) tell('Ayati needs its AI connection. See the project README to configure it on this computer.');
  else if (pendingSubmission.draft()) tell('Your last message is saved here. You can retry the same message safely.');
  draw();
}).catch(() => tell('Ayati is not reachable. Start the local service and reload.'));
void sync();
