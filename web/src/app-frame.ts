import { workspaceUrl } from './workspace-context';
import { api } from './types';

export type AppSession = { session: string; document_id: string; document_url: string; revision: number; preview: boolean; definition: { name: string; description: string; html: string; css: string; javascript: string; collections: Record<string, Record<string, string>> } };

export function mountAppFrame(container: HTMLElement, app: string, session: AppSession, status: HTMLElement): { dispose(): void; ready: Promise<void> } {
  const frame = document.createElement('iframe'); frame.title = session.definition.name;
  frame.setAttribute('sandbox', 'allow-scripts allow-forms'); frame.referrerPolicy = 'no-referrer'; frame.className = 'generated-app-frame';
  let disposed = false, port: MessagePort | undefined, loads = 0, count = 0, windowStart = Date.now();
  const pending = new Set<string>(); const abort = new AbortController();
  let acceptReady: () => void;
  const ready = new Promise<void>(resolve => { acceptReady = resolve; });
  const timeout = setTimeout(() => { if (!port) status.textContent = 'The app did not start. Inspect the source or ask its maintaining agent to fix the preview.'; }, 10000);
  async function records(command: unknown): Promise<unknown> {
    const options: RequestInit = { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Ayati-App-Session': session.session }, body: JSON.stringify(command), signal: abort.signal };
    try { return await api(`/apps/${app}/records`, options); }
    catch (error) {
      // A record mutation has its own durable UUID receipt. A single retry of
      // exactly the same body can recover a transport loss without a duplicate.
      if (!disposed && (error instanceof TypeError || error instanceof SyntaxError)) return api(`/apps/${app}/records`, options);
      throw error;
    }
  }
  const receive = (event: MessageEvent): void => {
    if (disposed || port || event.source !== frame.contentWindow || event.data?.type !== 'ayati-app-ready' || event.data.document_id !== session.document_id || event.ports.length !== 1) return;
    port = event.ports[0]; clearTimeout(timeout);
    port.onmessage = async event => {
      const message = event.data;
      try {
        if (!message || typeof message.id !== 'string' || message.id.length > 100 || !message.command || !['list', 'save', 'delete'].includes(message.command.action) || JSON.stringify(message.command).length > 8000 || pending.size >= 8 || pending.has(message.id)) throw new Error('Invalid or excessive app record request.');
        if (Date.now() - windowStart > 60000) { count = 0; windowStart = Date.now(); }
        if (++count > 100) throw new Error('App record request rate exceeded. Wait a minute before retrying.');
        pending.add(message.id);
        const result = await records(message.command);
        if (!disposed) port?.postMessage({ id: message.id, result });
      } catch (error) { if (!disposed) port?.postMessage({ id: message?.id, error: (error as Error).message }); }
      finally { pending.delete(message?.id); }
    };
    port.start(); status.textContent = session.preview ? 'Isolated preview records · changes here do not affect the active app.' : 'Active app · records are saved without model calls.'; acceptReady();
  };
  const dispose = (): void => { disposed = true; clearTimeout(timeout); abort.abort(); port?.close(); window.removeEventListener('message', receive); frame.remove(); };
  window.addEventListener('message', receive);
  frame.onload = () => { if (++loads > 1) { status.textContent = 'The app navigated away from its reviewed document. Reopen the app to restore scoped record access.'; dispose(); } };
  frame.src = workspaceUrl(session.document_url); container.append(frame);
  return { dispose, ready };
}
