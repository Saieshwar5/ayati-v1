import { useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { currentWorkspace, workspaceSession } from '../../workspace-context';
import { saveAgent, saveAgentWithPicture, type Agent, type Audience } from './model';
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
    await saveAgentWithPicture({ ...draft, people: undefined }, file, next => setDraft(old => ({ ...old, revision: next.revision, settings: next.settings })));
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

/** Who can talk to it, reusing the create flow's people step. */
export function PeopleDialog({ agent, close, saved }: Props) {
  const me = workspaceSession?.user?.id ?? '';
  const members = useResource(loadMembers);
  const [audience, setAudience] = useState<Audience>(agent.audience);
  const [people, setPeople] = useState(agent.people ?? []);
  const mutation = useMutation();
  const save = () => void mutation.run(async () => {
    await saveAgent({ ...agent, audience, people: audience === 'selected' ? people : agent.people });
    await saved('Who can talk saved.');
  });
  return <Dialog title={`${agent.name} · People`} close={close} busy={mutation.busy} status={mutation.status} footer={<>
    <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    <Button primary disabled={mutation.busy} onClick={save}>Save</Button>
  </>}>
    <PeopleStep name={agent.name} workspace={currentWorkspace()?.name ?? 'this workspace'} me={me} owners={agent.owners ?? []}
      members={members.data?.users} audience={audience} people={people}
      setAudience={setAudience} setPeople={setPeople} />
  </Dialog>;
}
