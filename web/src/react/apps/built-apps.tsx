import { useCallback, useRef, useState } from 'react';
import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { appColour, appsRequest, builtPath, type BuiltApp } from './apps-model';
import { CodeDrawer } from './built-app-code';
import { RolesDialog } from './built-app-roles';

export function BuiltIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="#fff"
      strokeWidth="1.9"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M8 8 4 12l4 4M16 8l4 4-4 4M13.5 5l-3 14" />
    </svg>
  );
}

export const runUrl = (id: string, preview = false) =>
  workspaceUrl(`/?view=apps&run=${encodeURIComponent(id)}${preview ? '&preview=1' : ''}`);

/** The side panel for an app built here: open it, or (builder and admins) its code. */
export function BuiltPanel({ app, changed }: { app: BuiltApp; changed: () => void }) {
  const [coding, setCoding] = useState(false);
  const [editing, setEditing] = useState(false);
  const [giving, setGiving] = useState(false);
  const mutation = useMutation();
  return (
    <aside className="app-panel" aria-label={app.name}>
      <span className="app-icon big" style={{ background: appColour(app.id) }}>
        <BuiltIcon />
      </span>
      <div>
        <h2>{app.name}</h2>
        <p className="app-line">
          Built here by {app.builder_name}
          {app.description ? ` · ${app.description}` : ''}
        </p>
      </div>
      {app.can_open && app.version ? (
        <a className="ui-button primary" href={runUrl(app.id)}>
          Open {app.name}
        </a>
      ) : (
        <p className="app-callout">
          {app.version
            ? "You can't open this app."
            : 'Not published yet. Only its builder and admins see it.'}
        </p>
      )}
      <section aria-label="Who can use it">
        <h3>Who can use it</h3>
        <p>
          {app.audience === 'everyone'
            ? 'Everyone in the workspace'
            : app.people
              ? app.people.length
                ? app.people.map((p) => p.name).join(', ')
                : 'Only its builder and admins'
              : 'Chosen people'}
        </p>
      </section>
      <section aria-label="How it runs">
        <h3>How it runs</h3>
        <p>
          Inside Cuckoo, walled off from the page around it, with no internet access of its own.
        </p>
      </section>
      {app.can_manage && (
        <>
          <section aria-label="Versions">
            <h3>Versions</h3>
            <p>
              {app.version ? `Version ${app.version} is live.` : 'Nothing published.'}{' '}
              {app.versions?.length ? `${app.versions.length} published so far.` : ''}
            </p>
          </section>
          <div className="apps-row">
            <Button onClick={() => setCoding(true)}>
              {app.is_builder ? 'Code and publish' : 'Code and versions'}
            </Button>
            <a className="ui-button" href={runUrl(app.id, true)}>
              Preview draft
            </a>
            <Button onClick={() => setGiving(true)}>Roles</Button>
            <Button onClick={() => setEditing(true)}>Settings</Button>
            <Button
              disabled={mutation.busy}
              onClick={() =>
                void mutation.run(async () => {
                  if (!confirm(`Remove ${app.name}? Nobody will be able to open it.`)) return;
                  await appsRequest(builtPath(app.id, '/remove'), {});
                  changed();
                })
              }
            >
              Remove
            </Button>
          </div>
        </>
      )}
      {mutation.status && <p role="alert">{mutation.status}</p>}
      {coding && <CodeDrawer app={app} close={() => setCoding(false)} changed={changed} />}
      {giving && <RolesDialog app={app} close={() => setGiving(false)} />}
      {editing && <BuiltForm app={app} close={() => setEditing(false)} saved={changed} />}
    </aside>
  );
}

type Person = { id: string; name: string };
/** Start an app from the starter files, or change its name and who can use it. */
export function BuiltForm({
  app,
  close,
  saved,
}: {
  app?: BuiltApp;
  close: () => void;
  saved: (id: string) => void;
}) {
  const loadPeople = useCallback(
    () => identityRequest<{ users: Person[] }>(`/workspaces/${workspaceId}/chat-directory`),
    [],
  );
  const people = useResource(loadPeople);
  const mutation = useMutation();
  const id = useRef(app?.id ?? crypto.randomUUID());
  const [name, setName] = useState(app?.name ?? ''),
    [description, setDescription] = useState(app?.description ?? '');
  const [audience, setAudience] = useState(app?.audience ?? 'everyone');
  const [chosen, setChosen] = useState<string[]>(app?.people?.map((p) => p.id) ?? []);
  const save = () =>
    void mutation.run(async () => {
      const body = {
        id: id.current,
        name,
        description,
        audience,
        people: audience === 'selected' ? chosen : [],
      };
      await appsRequest(app ? builtPath(app.id) : '/built', body);
      saved(id.current);
      close();
    });
  return (
    <Dialog
      drawer
      title={app ? `Settings for ${app.name}` : 'Build an app'}
      close={close}
      busy={mutation.busy}
      status={mutation.status || people.error}
      footer={
        <>
          <Button disabled={mutation.busy} onClick={close}>
            Cancel
          </Button>
          <Button primary disabled={mutation.busy || !name.trim()} onClick={save}>
            {app ? 'Save changes' : 'Start building'}
          </Button>
        </>
      }
    >
      {!app && (
        <p>
          A small app for your team, such as a leave request form. It starts from a ready page you
          change, and nobody else sees it until you publish it.
        </p>
      )}
      <label className="ui-field">
        Name
        <input
          aria-label="Name"
          maxLength={80}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <label className="ui-field">
        What it is for <small>Optional</small>
        <input
          aria-label="What it is for"
          maxLength={300}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </label>
      <label className="ui-field">
        Who can use it
        <select
          aria-label="Who can use it"
          value={audience}
          onChange={(e) => setAudience(e.target.value as BuiltApp['audience'])}
        >
          <option value="everyone">Everyone in the workspace</option>
          <option value="selected">Chosen people</option>
        </select>
      </label>
      {audience === 'selected' && (
        <fieldset className="apps-people">
          <legend>People</legend>
          {people.data?.users.map((person) => (
            <label key={person.id} className="ui-check">
              <input
                type="checkbox"
                checked={chosen.includes(person.id)}
                onChange={(e) =>
                  setChosen((old) =>
                    e.target.checked ? [...old, person.id] : old.filter((p) => p !== person.id),
                  )
                }
              />
              {person.name}
            </label>
          ))}
        </fieldset>
      )}
    </Dialog>
  );
}
