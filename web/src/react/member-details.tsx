import { Dialog } from './ui';
import { useMutation } from './requests';
import { MemberAccess } from './member-access';
import { personName } from './remove-person-dialog';
import type { Member } from './people';

// Per-person agent grants. Each agent's "who can talk to it" replaces this in M2 (D233).
export function MemberDetails({ member, close }: { member: Member; close: () => void }) {
  const mutation = useMutation();
  return <Dialog title={`Agent access · ${personName(member)}`} drawer busy={mutation.busy} status={mutation.status} close={close}>
    <p className="ui-member-email">{member.email}</p>
    <MemberAccess user={member.id} mutation={mutation} />
  </Dialog>;
}
