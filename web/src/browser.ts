import { api, post, selectedAgent } from './types';

type Result = { isError?: boolean; content?: { type: string; data?: string; mimeType?: string }[] };
type Status = { configured: boolean; owner: boolean; vm?: { state?: string }; guest?: { interaction_pending: boolean } };

export function installBrowser(): void {
  const button = document.querySelector<HTMLButtonElement>('#browser-button')!;
  const focusLauncher = (): void => { (button.getClientRects().length ? button : document.querySelector<HTMLButtonElement>('#details-button'))?.focus(); };
  const panel = document.querySelector<HTMLElement>('#browser-panel')!;
  const canvas = document.querySelector<HTMLCanvasElement>('#browser-canvas')!;
  const message = document.querySelector<HTMLElement>('#browser-message')!;
  const take = document.querySelector<HTMLButtonElement>('#browser-take')!;
  const resume = document.querySelector<HTMLButtonElement>('#browser-resume')!;
  const finish = document.querySelector<HTMLButtonElement>('#browser-finish')!;
  const address = document.querySelector<HTMLInputElement>('#browser-address')!;
  const privateText = document.querySelector<HTMLInputElement>('#browser-private-text')!;
  const storageKey = selectedAgent === 'local-agent' ? 'ayati-browser-controller' : `ayati-browser-controller.${selectedAgent}`;
  let token: string | undefined;
  try { token = sessionStorage.getItem(storageKey) ?? undefined; } catch { /* In-memory fallback. */ }
  let owns = false;
  let busyFrame = false;
  let frameEpoch = 0;
  let handingBack = false;
  let inputQueue = Promise.resolve();
  const say = (text: string): void => { message.textContent = text; };
  const control = <T = Result>(op: string, extra: Record<string, unknown> = {}): Promise<T> =>
    post('/browser/control', { op, token, ...extra }) as Promise<T>;

  function input(tool: string, args: Record<string, unknown>): void {
    if (!owns || handingBack) return;
    inputQueue = inputQueue.then(async () => {
      if (!owns) return;
      const result = await control('input', { tool: `agent_browser_${tool}`, arguments: args });
      if (result.isError) throw new Error('Browser input failed. Inspect the page before trying again.');
    }).catch((error: Error) => { owns = false; take.disabled = false; say(`${error.message} Control remains paused; reconnect with Take control.`); });
  }

  async function frame(): Promise<void> {
    if (panel.hidden || busyFrame || handingBack) return;
    busyFrame = true;
    const epoch = frameEpoch;
    try {
      const result = await control('frame');
      const picture = result.content?.find((item) => item.type === 'image' && item.data);
      if (result.isError || !picture) return;
      const img = new Image();
      img.src = `data:${picture.mimeType};base64,${picture.data}`;
      await img.decode();
      if (panel.hidden || frameEpoch !== epoch) return;
      canvas.width = img.width; canvas.height = img.height;
      canvas.getContext('2d')!.drawImage(img, 0, 0);
    } catch (error) { say((error as Error).message); }
    finally { busyFrame = false; }
  }

  async function status(): Promise<void> {
    try {
      const state = await api<Status>('/browser/status');
      button.hidden = !state.configured;
      if (!state.owner) owns = false;
      resume.disabled = !state.owner || !token;
      finish.disabled = !owns;
      take.disabled = owns;
      document.querySelector<HTMLElement>('#browser-owner-input')!.hidden = !owns;
      canvas.setAttribute('aria-disabled', String(!owns));
      if (!owns) say(state.owner ? 'Owner control is paused. Reconnect from the controlling tab; closing this panel does not resume the agent.' : state.guest?.interaction_pending ? 'The browser is keeping unfinished interaction open. Take control to continue, or close the browser session when finished.' : 'Watch the agent browser or take control to complete login.');
    } catch { /* Chat presents daemon connection state. */ }
  }

  button.onclick = () => { panel.hidden = !panel.hidden; if (!panel.hidden) { void status(); void frame(); } };
  document.querySelector<HTMLButtonElement>('#browser-close')!.onclick = () => {
    panel.hidden = true;
    frameEpoch++;
    canvas.getContext('2d')!.clearRect(0, 0, canvas.width, canvas.height);
    privateText.value = ''; focusLauncher();
  };
  take.onclick = async () => {
    if (!token) { token = crypto.randomUUID(); try { sessionStorage.setItem(storageKey, token); } catch { /* Memory token remains valid. */ } }
    take.disabled = true; say('Waiting for the agent browser to pause…');
    try {
      await control('take'); owns = true;
      say('You control the browser. Agent captures and actions are paused. Hand back explicitly when finished.');
      document.querySelector<HTMLElement>('#browser-owner-input')!.hidden = false;
      resume.disabled = false; finish.disabled = false; await frame(); canvas.focus();
    } catch (error) { take.disabled = false; say((error as Error).message); }
  };
  finish.onclick = async () => {
    handingBack = true; frameEpoch++; finish.disabled = true;
    try {
      await inputQueue; owns = false; privateText.value = '';
      await control('finish');
      panel.hidden = true;
      canvas.getContext('2d')!.clearRect(0, 0, canvas.width, canvas.height);
      await status(); focusLauncher();
    } catch (error) { say((error as Error).message); }
    finally { handingBack = false; }
  };
  resume.onclick = async () => {
    resume.disabled = true;
    handingBack = true; frameEpoch++;
    try {
      await inputQueue; owns = false; privateText.value = '';
      frameEpoch++;
      await control('resume');
      canvas.getContext('2d')!.clearRect(0, 0, canvas.width, canvas.height);
      say('Control handed back. Send a message to continue any interrupted task.');
      await status();
    } catch (error) { resume.disabled = false; say((error as Error).message); }
    finally { handingBack = false; }
  };
  const point = (event: PointerEvent): { x: number; y: number } => {
    const rect = canvas.getBoundingClientRect();
    return { x: Math.round((event.clientX - rect.left) * canvas.width / rect.width), y: Math.round((event.clientY - rect.top) * canvas.height / rect.height) };
  };
  canvas.onpointerdown = (event) => {
    if (!owns || event.button !== 0) return;
    event.preventDefault(); canvas.focus(); canvas.setPointerCapture(event.pointerId);
    input('mouse_move', point(event)); input('mouse_down', { button: 'left' });
  };
  canvas.onpointerup = (event) => { if (owns) { input('mouse_move', point(event)); input('mouse_up', { button: 'left' }); } };
  canvas.onpointercancel = () => { if (owns) input('mouse_up', { button: 'left' }); };
  canvas.onwheel = (event) => { if (owns) { event.preventDefault(); input('mouse_wheel', { dy: Math.max(-1200, Math.min(1200, Math.round(event.deltaY))) }); } };
  canvas.onkeydown = (event) => {
    if (!owns || event.isComposing) return;
    if (['Shift', 'Control', 'Alt', 'Meta', 'CapsLock'].includes(event.key)) return;
    event.preventDefault();
    if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) input('keyboard_insert_text', { text: event.key });
    else input('press', { key: `${event.ctrlKey ? 'Control+' : ''}${event.metaKey ? 'Meta+' : ''}${event.altKey ? 'Alt+' : ''}${event.shiftKey ? 'Shift+' : ''}${event.key}` });
  };
  document.querySelector<HTMLFormElement>('#browser-navigate')!.onsubmit = (event) => { event.preventDefault(); input('open', { url: address.value }); };
  document.querySelector<HTMLFormElement>('#browser-text')!.onsubmit = (event) => {
    event.preventDefault(); const text = privateText.value; privateText.value = '';
    if (text) input('keyboard_insert_text', { text });
  };
  window.setInterval(() => { void frame(); }, 1000);
  window.setInterval(() => { if (!panel.hidden) void status(); }, 3000);
  void status();
}
