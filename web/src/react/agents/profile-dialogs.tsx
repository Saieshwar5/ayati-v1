import { useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { currentWorkspace, identityRequest, workspaceSession } from '../../workspace-context';
import { base, saveAgent, saveAgentWithPicture, type Agent, type Audience } from './model';
import { NameStep } from './create/name-step';
import { loadMembers, PeopleStep } from './create/people-step';

type Props = { agent: Agent; close: () => void; saved: (message: string) => Promise<void> };

/** Name, job title and picture, reusing the create flow's first step. */
export function IdentityDialog({ agent, close, saved }: Props) {
  const [draft, setDraft] = useState(agent);
  const [file, setFile] = useState<File>();
  const mutation = useMutation();
  const first = useRef<HTMLInputElement>(null);
  const save = () => void mutation.run(async () => {
    await saveAgentWithPicture({ ...draft, people: undefined, managers: undefined }, file, next => setDraft(old => ({ ...old, revision: next.revision, settings: next.settings })));
    await saved(`${draft.name.trim()} saved.`);
  });
  return <Dialog title={`${agent.name} · Name and picture`} close={close} busy={mutation.busy} status={mutation.status} initialFocus={first} footer={<>
    <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    <Button primary disabled={mutation.busy || !draft.name.trim()} onClick={save}>Save</Button>
  </>}>
    <NameStep draft={draft} file={file} first={first} change={(key, value) => setDraft(old => ({ ...old, [key]: value }))}
      choose={robot => { setFile(undefined); setDraft(old => ({ ...old, settings: { ...old.settings, avatar: robot } })); }}
      upload={setFile} reject={mutation.setStatus} />
  </Dialog>;
}

/** Who can talk to it and its managers, reusing the create flow's people step. */
export function PeopleDialog({ agent, close, saved }: Props) {
  const me = workspaceSession?.user?.id ?? '';
  const members = useResource(loadMembers);
  const [audience, setAudience] = useState<Audience>(agent.audience);
  const [people, setPeople] = useState(agent.people ?? []);
  const [managers, setManagers] = useState(agent.managers ?? []);
  const mutation = useMutation();
  const save = () => void mutation.run(async () => {
    await saveAgent({ ...agent, audience, people: audience === 'selected' ? people : agent.people, managers });
    await saved('Who can talk and managers saved.');
  });
  return <Dialog title={`${agent.name} · People`} close={close} busy={mutation.busy} status={mutation.status} footer={<>
    <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    <Button primary disabled={mutation.busy || !managers.length} onClick={save}>Save</Button>
  </>}>
    <PeopleStep name={agent.name} workspace={currentWorkspace()?.name ?? 'this workspace'} me={me} owner={agent.owner_id ?? me}
      members={members.data?.users} audience={audience} people={people} managers={managers}
      setAudience={setAudience} setPeople={setPeople} setManagers={setManagers} />
  </Dialog>;
}

/** Hands the agent to another member, who becomes its owner and a manager. */
export function TransferDialog({ agent, close, saved }: Props) {
  const members = useResource(loadMembers);
  const [owner, setOwner] = useState('');
  const mutation = useMutation();
  const others = (members.data?.users ?? []).filter(member => member.id !== agent.owner_id);
  const save = () => void mutation.run(async () => {
    await identityRequest(`${base}/${agent.id}/owner`, { owner, revision: agent.revision });
    const name = others.find(member => member.id === owner)?.name ?? 'the new owner';
    await saved(`${name} now owns ${agent.name}.`);
  });
  return <Dialog title={`Transfer ${agent.name}`} close={close} busy={mutation.busy} status={mutation.status} footer={<>
    <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    <Button primary disabled={mutation.busy || !owner} onClick={save}>Transfer owner</Button>
  </>}>
    {!members.data && !members.error && <p role="status">Loading people…</p>}
    {members.error && <p role="alert">{members.error}</p>}
    <label className="ui-field">New owner<select value={owner} onChange={event => setOwner(event.target.value)}>
      <option value="">Choose a person</option>
      {others.map(member => <option key={member.id} value={member.id}>{member.name}</option>)}
    </select></label>
    <p className="ui-help">The new owner can change everything about {agent.name} and replaces {agent.owner_name ?? 'the current owner'} as a manager.
      {agent.audience === 'only_me' && ` ${agent.name} is set to Only me, so only the new owner will be able to talk to it.`}</p>
  </Dialog>;
}
