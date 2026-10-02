import './style.css';
import { api, post, type AgentEvent, type Artifact, type Snapshot } from './types';
import { element, renderConversation, renderFiles } from './view';
import { applyDelta, mergeSnapshot } from './conversation-state';

const app = document.querySelector<HTMLDivElement>('#app')!;
app.innerHTML = `
  <header class="header">
    <div class="identity"><span class="avatar" aria-hidden="true">a</span>
      <div><strong>Ayati</strong><span class="connection" id="connection">Connecting…</span></div>
    </div>
    <button class="files-button" id="files-button" aria-expanded="false" aria-controls="files-panel">Files <span id="file-count">0</span></button>
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
        <p class="composer-note">Your assistant, one conversation.</p>
      </div>
    </section>
    <aside class="files-panel" id="files-panel" aria-label="Your files" hidden>
      <div class="panel-header"><h2>Your files</h2><button class="icon-button" id="close-files" aria-label="Close files">×</button></div>
      <p class="panel-description">Originals and new versions, ready to download.</p>
      <div id="files-list" class="files-list"></div>
    </aside>
  </main>
  <input id="file-input" type="file" hidden />`;

const conversation = document.querySelector<HTMLDivElement>('#conversation')!;
const input = document.querySelector<HTMLTextAreaElement>('#input')!;
const notice = document.querySelector<HTMLDivElement>('#notice')!;
const connection = document.querySelector<HTMLSpanElement>('#connection')!;
const send = document.querySelector<HTMLButtonElement>('#send')!;
const stop = document.querySelector<HTMLButtonElement>('#stop')!;
const filesButton = document.querySelector<HTMLButtonElement>('#files-button')!;
const filesPanel = document.querySelector<HTMLElement>('#files-panel')!;
const fileInput = document.querySelector<HTMLInputElement>('#file-input')!;
let state: Snapshot = { messages: [], tasks: [], artifacts: [] };
let modelReady = false;
let sending = false;
let syncing = false;
let syncAgain = false;
let attachments: { name: string; path: string }[] = [];

function tell(text: string): void { notice.textContent = text; notice.hidden = !text; }

function draw(): void {
  renderConversation(conversation, state);
  renderFiles(document.querySelector('#files-list')!, state.artifacts);
  document.querySelector('#file-count')!.textContent = String(state.artifacts.filter((f) => f.kind !== 'log').length);
  stop.hidden = !state.tasks.some((task) => task.status === 'running');
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
  const content = input.value.trim() + attachments.map((f) => `\n\nAttached file: ${f.path}`).join('');
  try {
    await post('/messages', { content });
    input.value = '';
    input.style.height = '';
    attachments = [];
    showAttachments();
    tell('');
    await sync();
  } catch (error) { tell((error as Error).message); }
  finally { sending = false; draw(); input.focus(); }
});

input.addEventListener('keydown', (event) => {
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    document.querySelector<HTMLFormElement>('#composer')!.requestSubmit();
  }
});
input.addEventListener('input', () => {
  input.style.height = 'auto';
  input.style.height = `${Math.min(input.scrollHeight, 140)}px`;
});

async function stopTask(id: string): Promise<void> {
  try { await post(`/tasks/${id}/stop`); await sync(); }
  catch (error) { tell((error as Error).message); }
}
stop.onclick = () => {
  const active = state.tasks.find((task) => task.status === 'running');
  if (active) void stopTask(active.id);
};
conversation.addEventListener('click', (event) => {
  const target = (event.target as HTMLElement).closest<HTMLElement>('[data-stop-task]');
  if (target?.dataset.stopTask) void stopTask(target.dataset.stopTask);
});

function toggleFiles(open: boolean): void {
  filesPanel.hidden = !open;
  filesButton.setAttribute('aria-expanded', String(open));
  if (open) document.querySelector<HTMLButtonElement>('#close-files')!.focus();
  else filesButton.focus();
}
filesButton.onclick = () => toggleFiles(Boolean(filesPanel.hidden));
document.querySelector<HTMLButtonElement>('#close-files')!.onclick = () => toggleFiles(false);
document.addEventListener('keydown', (event) => { if (event.key === 'Escape' && !filesPanel.hidden) toggleFiles(false); });
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

const events = new EventSource('/api/events');
events.onopen = () => { connection.textContent = 'Here to help'; void sync(); };
events.onerror = () => { connection.textContent = 'Reconnecting…'; };
events.onmessage = (event) => {
  const update = JSON.parse(event.data) as AgentEvent;
  if (update.kind === 'delta' && update.message_id && update.text) {
    if (!applyDelta(state, update)) { void sync(); return; }
    renderConversation(conversation, state);
  } else { void sync(); }
};

void api<{ model_ready: boolean }>('/health').then((health) => {
  modelReady = health.model_ready;
  if (!modelReady) tell('Ayati needs its AI connection. See the project README to configure it on this computer.');
  draw();
}).catch(() => tell('Ayati is not reachable. Start the local service and reload.'));
void sync();
