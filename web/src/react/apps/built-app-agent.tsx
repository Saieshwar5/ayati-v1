import { useCallback, useRef, useState } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { openAgentChat } from '../chat/direct-conversation';
import type { Directory } from '../chat/model';
import { appsRequest, builtPath, type BuiltApp } from './apps-model';

/** The builder asks an agent to build the app (BA3): the agent may then write the draft
 * in this person's requests, and their chat with it opens with the request ready to
 * send. Publishing stays with the builder. */
export function AgentBuildDialog({ app, close }: { app: BuiltApp; close: () => void }) {
  const load = useCallback(
    () => identityRequest<Directory>(`/workspaces/${workspaceId}/chat-directory`),
    [],
  );
  const directory = useResource(load);
  const mutation = useMutation();
  const attempt = useRef<{ current?: string }>({});
  const [agent, setAgent] = useState('');
  const [request, setRequest] = useState(app.description);
  const chosen = directory.data?.agents.find((a) => a.id === agent);
  const start = () =>
    void mutation.run(async () => {
      if (!chosen) return;
      await appsRequest(builtPath(app.id, '/helpers'), { agent: chosen.id, helps: true });
      const opening = `Please build my app "${app.name}" (app ${app.id}). ${request.trim()}`.trim();
      await openAgentChat(chosen, opening, attempt.current);
    });
  return (
    <Dialog
      title={`Build ${app.name} with an agent`}
      close={close}
      busy={mutation.busy}
      status={mutation.status || directory.error}
      footer={
        <>
          <Button disabled={mutation.busy} onClick={close}>
            Cancel
          </Button>
          <Button primary disabled={mutation.busy || !chosen} onClick={start}>
            Open the chat
          </Button>
        </>
      }
    >
      <p>
        The agent can change this app's draft when you ask it, and only then. It can't publish: you
        preview the draft and publish it yourself.
      </p>
      <label className="ui-field">
        Agent
        <select aria-label="Agent" value={agent} onChange={(e) => setAgent(e.target.value)}>
          <option value="">Choose an agent</option>
          {directory.data?.agents.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </select>
      </label>
      <label className="ui-field">
        What should it build?
        <textarea
          aria-label="What should it build?"
          rows={4}
          maxLength={2000}
          value={request}
          onChange={(e) => setRequest(e.target.value)}
        />
      </label>
    </Dialog>
  );
}

/** The agents building the app, each with a way for the builder to stop it. */
export function AgentHelpers({ app, changed }: { app: BuiltApp; changed: () => void }) {
  const mutation = useMutation();
  if (!app.helpers?.length) return null;
  return (
    <section aria-label="Agents building it">
      <h3>Agents building it</h3>
      <ul className="built-helpers">
        {app.helpers.map((helper) => (
          <li key={helper.id}>
            <span>{helper.name}</span>
            {app.is_builder && (
              <Button
                disabled={mutation.busy}
                onClick={() =>
                  void mutation.run(async () => {
                    await appsRequest(builtPath(app.id, '/helpers'), {
                      agent: helper.id,
                      helps: false,
                    });
                    changed();
                  })
                }
              >
                Stop {helper.name}
              </Button>
            )}
          </li>
        ))}
      </ul>
      {mutation.status && <p role="alert">{mutation.status}</p>}
    </section>
  );
}
