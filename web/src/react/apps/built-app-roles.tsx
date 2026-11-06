import { useCallback, useState } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { appsRequest, builtPath, type BuiltApp } from './apps-model';

type Roles = {
  roles: string[];
  held: { role: string; id: string; name: string }[];
  from_version: number;
};
type Person = { id: string; name: string };

/** Who holds each of a built app's roles (BA2). Everyone who can open the app holds
 * its first role; the builder and admins give the others. */
export function RolesDialog({ app, close }: { app: BuiltApp; close: () => void }) {
  const load = useCallback(() => appsRequest<Roles>(builtPath(app.id, '/roles')), [app.id]);
  const roles = useResource(load);
  const loadPeople = useCallback(
    () => identityRequest<{ users: Person[] }>(`/workspaces/${workspaceId}/chat-directory`),
    [],
  );
  const people = useResource(loadPeople);
  const mutation = useMutation();
  const [drafts, setDrafts] = useState<Record<string, string[]>>({});
  const holders = (role: string) =>
    drafts[role] ?? roles.data?.held.filter((h) => h.role === role).map((h) => h.id) ?? [];
  const given = roles.data?.roles.slice(1) ?? [];
  const save = (role: string) =>
    void mutation.run(async () => {
      await appsRequest(builtPath(app.id, '/roles'), { role, people: holders(role) });
      await roles.refresh();
      setDrafts((old) => {
        const next = { ...old };
        delete next[role];
        return next;
      });
      mutation.setStatus(`Saved who holds "${role}".`);
    });
  return (
    <Dialog
      drawer
      title={`Roles in ${app.name}`}
      close={close}
      busy={mutation.busy}
      status={mutation.status || roles.error || people.error}
    >
      {roles.data && (
        <p>
          {roles.data.roles.length
            ? `Everyone who can open it holds "${roles.data.roles[0]}". The app's manifest says what each role may do.`
            : 'Publish the app first; its manifest names its roles.'}
          {roles.data.from_version === 0 && roles.data.roles.length
            ? ' These are the draft’s roles.'
            : ''}
        </p>
      )}
      {roles.data && roles.data.roles.length > 0 && !given.length && (
        <p className="ui-empty">This app has no other roles to give.</p>
      )}
      {given.map((role) => (
        <fieldset key={role} className="apps-people">
          <legend>{role}</legend>
          {people.data?.users.map((person) => (
            <label key={person.id} className="ui-check">
              <input
                type="checkbox"
                checked={holders(role).includes(person.id)}
                onChange={(e) =>
                  setDrafts((old) => ({
                    ...old,
                    [role]: e.target.checked
                      ? [...holders(role), person.id]
                      : holders(role).filter((p) => p !== person.id),
                  }))
                }
              />
              {person.name}
            </label>
          ))}
          <div className="apps-row">
            <Button primary disabled={mutation.busy || !drafts[role]} onClick={() => save(role)}>
              Save {role}
            </Button>
          </div>
        </fieldset>
      ))}
    </Dialog>
  );
}
