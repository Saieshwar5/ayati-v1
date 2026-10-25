import type { ReactNode } from 'react';
import { Button } from '../ui';

export function AgentWorkspaceHeader({ name, role, avatar, status, owner, back, children }: {
  name: string; role: string; avatar: ReactNode; status: string; owner?: string;
  back: () => void; children: ReactNode;
}) {
  return <header className="agent-workspace-header">
    <Button className="agent-roster-back" onClick={back}>‹ Agents</Button>
    <div className="agent-identity-row">{avatar}<div className="agent-identity-title"><h2>{name}</h2>
      <p>{role}</p></div><span className="agent-availability">{status}</span><div className="agent-header-actions">{children}</div></div>
    {owner && <p className="agent-responsible">Responsible person: <strong>{owner}</strong></p>}
  </header>;
}
