import { useCallback } from 'react';
import { useMutation, useResource } from '../requests';
import { appsRequest, builtPath, type BuiltApp } from './apps-model';

type Rules = {
  actions: { id: string; title: string; does: string; mode: 'allow' | 'ask' | 'block' }[];
  can_change: boolean;
};

/** What agents may do in a built app (AP10), for everyone who can see it; its builder
 * and admins allow or block each action the manifest offers. Agents act as the person
 * they work for and never beyond that person's roles. */
export function AgentRules({ app }: { app: BuiltApp }) {
  const load = useCallback(() => appsRequest<Rules>(builtPath(app.id, '/agent-rules')), [app.id]);
  const rules = useResource(load);
  const mutation = useMutation();
  if (!rules.data?.actions?.length) return null;
  const set = (action: string, mode: string) =>
    void mutation.run(async () => {
      await appsRequest(builtPath(app.id, '/agent-rules'), { action, mode });
      await rules.refresh();
    });
  return (
    <section aria-label="Agents can">
      <h3>Agents can</h3>
      <ul className="app-actions">
        {rules.data.actions.map((action) => (
          <li key={action.id}>
            <span>
              {action.title}
              <small>For the person they work for, within that person's access</small>
            </span>
            {rules.data?.can_change ? (
              <select
                aria-label={`Agents: ${action.title}`}
                value={action.mode}
                disabled={mutation.busy}
                onChange={(e) => set(action.id, e.target.value)}
              >
                <option value="allow">Allow</option>
                <option value="ask">Ask first</option>
                <option value="block">Block</option>
              </select>
            ) : (
              <span className={`app-mode ${action.mode}`}>
                {{ allow: 'Allow', ask: 'Ask first', block: 'Block' }[action.mode]}
              </span>
            )}
          </li>
        ))}
      </ul>
      {(mutation.status || rules.error) && <p role="alert">{mutation.status || rules.error}</p>}
    </section>
  );
}
