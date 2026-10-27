import { useEffect, useId, useRef, useState } from 'react';
import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { errorMessage, useMutation } from '../requests';
import type { Conversation, Directory } from '../chat/model';
import { readPanels, type ChangeTarget } from './model';
import { subscribeChanges } from '../chat/live';
import { PanelTypeChoice, panelPreference, type PanelChoice } from './type-choice';
export function AskPanel({
  close,
  target,
  sourceAvailable = true,
}: {
  close: () => void;
  target?: ChangeTarget;
  sourceAvailable?: boolean;
}) {
  const fieldId = useId();
  const [directory, setDirectory] = useState<Directory>();
  const [chats, setChats] = useState<Conversation[]>([]);
  const [agent, setAgent] = useState(target?.agent ?? '');
  const [chat, setChat] = useState(target?.chat ?? '');
  const [text, setText] = useState('');
  const [choice, setChoice] = useState<PanelChoice>({ family: '', variant: 'line' });
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const m = useMutation();
  const attempt = useRef<{ key: string; id: string }>(undefined);
  useEffect(() => {
    const abort = new AbortController();
    setDirectory(undefined);
    setChats([]);
    setError('');
    void Promise.all([
      identityRequest<Directory>(`/workspaces/${workspaceId}/chat-directory`, undefined, abort.signal),
      identityRequest<{ chats: Conversation[] }>(`/workspaces/${workspaceId}/chats`, undefined, abort.signal),
    ])
      .then(([d, c]) => {
        if (!abort.signal.aborted) {
          setDirectory(d);
          setChats(c.chats);
        }
      })
      .catch((failure) => {
        if (!abort.signal.aborted) setError(errorMessage(failure));
      });
    return () => abort.abort();
  }, [retry]);
  useEffect(
    () =>
      subscribeChanges((change) => {
        if (['access', 'reset'].includes(change.kind)) {
          setDirectory(undefined);
          setChats([]);
          setRetry((value) => value + 1);
        }
      }),
    [],
  );
  const agents = directory?.agents.filter((a) => a.scope === 'workspace') ?? [];
  const conversations = chats.filter((c) => c.agents.includes(agent) && !c.archived);
  const permitted = agents.some((a) => a.id === agent) && conversations.some((c) => c.id === chat);
  return (
    <Dialog title={target ? 'Ask for changes' : 'Ask for a Home panel'} close={close}>
      <p>
        {target
          ? 'This request stays with the original agent and conversation. Your approved view stays in place while you review a new draft.'
          : 'Ask a permitted workspace agent in an existing conversation. Review its saved draft on Home before keeping it.'}
      </p>
      {target && !sourceAvailable && (
        <p role="status">
          The selected panel source is being checked or is unavailable. Your change request is kept here.
        </p>
      )}
      {error && (
        <p role="alert">
          {error} <Button onClick={() => setRetry((v) => v + 1)}>Retry options</Button>
        </p>
      )}
      {!directory && !error && <p role="status">Loading your agents and conversations…</p>}
      {directory && !agents.length && (
        <p>No permitted workspace agents are available. Ask the workspace owner for access.</p>
      )}
      <form
        className="workspace-panel-form"
        onSubmit={(event) => {
          event.preventDefault();
          void m.run(async () => {
            if (!agent || !chat || !text.trim())
              throw new Error('Choose an agent, conversation and panel request.');
            if (!permitted)
              throw new Error(
                'Current permission to request this agent in the original conversation is required.',
              );
            if (target) {
              const current = (await readPanels()).panels.find((panel) => panel.id === target.id);
              if (
                !current ||
                current.revision !== target.revision ||
                current.agent !== target.agent ||
                !(current.active?.available || current.draft?.available)
              )
                throw new Error(
                  'The selected panel or source changed. Reopen Ask for changes from the current saved view; no new task was sent.',
                );
            }
            const instruction = target
              ? `[Personal Home panel change]\nRevise personal panel ${target.id} using expected_revision=${target.revision} and save_home_panel. Keep its original person, agent and conversation. A new draft requires my review; create no schedule.`
              : '[Personal Home panel request]\nPrepare one personal Home draft using save_home_panel. Use a fixed template and current authorized sources. It requires my review on Home; do not publish it or create a schedule.';
            const content = `${instruction}${panelPreference(choice)}\n${text.trim()}`;
            const key = JSON.stringify({ agent, chat, content });
            if (attempt.current?.key !== key) attempt.current = { key, id: crypto.randomUUID() };
            const id = attempt.current.id;
            await identityRequest(`/workspaces/${workspaceId}/chats/${chat}/messages`, {
              id,
              agent,
              content,
            });
            location.assign(
              workspaceUrl(
                `/?view=chat&conversation=${encodeURIComponent(chat)}&message=${encodeURIComponent(id)}`,
              ),
            );
          });
        }}
      >
        <div className="workspace-panel-field">
          <label htmlFor={`${fieldId}-agent`}>Agent</label>
          <select
            id={`${fieldId}-agent`}
            value={agent}
            disabled={m.busy || Boolean(target)}
            onChange={(e) => {
              setAgent(e.target.value);
              setChat('');
            }}
            required
          >
            <option value="">Choose an agent</option>
            {agents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </div>
        <div className="workspace-panel-field">
          <label htmlFor={`${fieldId}-chat`}>Conversation</label>
          <select
            id={`${fieldId}-chat`}
            value={chat}
            disabled={m.busy || !agent || Boolean(target)}
            onChange={(e) => setChat(e.target.value)}
            required
          >
            <option value="">Choose an existing conversation</option>
            {conversations.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </div>
        {agent && !conversations.length && (
          <p className="ui-help">
            This agent needs an accessible, unarchived conversation. Add it to a conversation in Chat first.
          </p>
        )}
        <PanelTypeChoice id={fieldId} value={choice} change={setChoice} disabled={m.busy} />
        <div className="workspace-panel-field">
          <label htmlFor={`${fieldId}-text`}>What should the panel show?</label>
          <textarea
            id={`${fieldId}-text`}
            value={text}
            onChange={(e) => setText(e.target.value)}
            maxLength={3000}
            required
            rows={5}
            disabled={m.busy}
          />
        </div>
        <p className="ui-help">
          Submitting starts an ordinary agent task. Opening this form starts no agent work.
        </p>
        {m.status && <p role="alert">{m.status}</p>}
        <Button
          primary
          type="submit"
          disabled={
            m.busy || !agent || !chat || !text.trim() || !permitted || (Boolean(target) && !sourceAvailable)
          }
        >
          {m.busy ? 'Sending request…' : 'Ask agent'}
        </Button>
      </form>
    </Dialog>
  );
}
