import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { workspaceUrl } from '../../workspace-context';
import { dismissNeed, type FixGroup } from './model';

const fixUrl = (fix: FixGroup['fix']) => fix.kind === 'needs' ? workspaceUrl('/?view=activity&tab=needs')
  : fix.kind === 'tasks' ? workspaceUrl('/?view=activity&tab=open')
  : fix.kind === 'agent' ? workspaceUrl(`/?view=agents&selected=${encodeURIComponent(fix.ref ?? '')}`)
  : fix.kind === 'settings' ? workspaceUrl('/?view=apps&manage=1') : undefined;

/** Needs fixing (people-and-agents-first, Phase 4 AC3): what's wrong across the
 * agents you can fix, grouped by cause, each with one fix. */
export function NeedsFixing({ groups, openRequest, changed }: { groups: FixGroup[]; openRequest: (reply: string) => void; changed: () => void }) {
  const [needs, setNeeds] = useState<FixGroup>();
  if (!groups.length) return null;
  return <section className="activity-fixing" aria-label="Needs fixing">
    <h2 className="activity-group">Needs fixing · {groups.length}</h2>
    <ul className="activity-list">{groups.map(group => {
      const url = fixUrl(group.fix);
      return <li key={`${group.cause}.${group.title}`} className="activity-fix-row">
        <span className="activity-fix-faces">{group.agents.slice(0, 4).map(agent =>
          <AgentFace key={agent.id} agent={{ id: agent.id, name: agent.name }} className="activity-face" />)}</span>
        <span className="activity-main"><strong>{group.title}</strong>
          <small>{group.agents.map(agent => agent.name).join(', ')}{group.items[0]?.detail && group.cause !== 'failing' ? ` · ${group.items[0].detail}` : ''}</small></span>
        {url ? <a className="ui-button" href={url}>{group.fix.label}</a>
          : group.fix.kind === 'need' ? <Button onClick={() => setNeeds(group)}>{group.fix.label}</Button>
          : <Button onClick={() => group.fix.ref && openRequest(group.fix.ref)}>{group.fix.label}</Button>}
      </li>;
    })}</ul>
    {needs && <NeedsDialog group={needs} openRequest={openRequest} close={() => setNeeds(undefined)} changed={changed} />}
  </section>;
}

function NeedsDialog({ group, openRequest, close, changed }: { group: FixGroup; openRequest: (reply: string) => void; close: () => void; changed: () => void }) {
  const m = useMutation();
  const [gone, setGone] = useState<string[]>([]);
  return <Dialog title={group.title} close={close} busy={m.busy} status={m.status}>
    <ul className="activity-list">{group.items.filter(item => !gone.includes(item.ref)).map(item => <li key={item.ref}>
      <span className="activity-main"><strong>{item.agent_name}: {item.detail}</strong>{item.fix && <small>Fix: {item.fix}</small>}</span>
      {item.request && <Button onClick={() => { close(); openRequest(item.request!); }}>Open request</Button>}
      <Button disabled={m.busy} onClick={() => void m.run(async () => { await dismissNeed(item.ref); setGone(list => [...list, item.ref]); changed(); m.setStatus('Dismissed.'); })}>Dismiss</Button>
    </li>)}</ul>
  </Dialog>;
}
