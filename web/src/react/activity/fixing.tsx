import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { workspaceUrl } from '../../workspace-context';
import { dismissNeed, type FixGroup } from './model';
import { RequestDetails } from './request-details';

const fixUrl = (fix: FixGroup['fix']) => fix.kind === 'tasks' ? workspaceUrl('/?view=activity&tab=open')
  : fix.kind === 'agent' ? workspaceUrl(`/?view=agents&selected=${encodeURIComponent(fix.ref ?? '')}`)
  : fix.kind === 'settings' ? workspaceUrl('/?view=settings&section=connections') : undefined;
export const fixKey = (group: FixGroup) => `fix.${group.cause}.${group.title}`;
/** What needs fixing and belongs in Needs you: every group except the one that only
 * points back at Needs you itself (requests waiting on a person). */
export const fixGroups = (groups: FixGroup[] | undefined) => (groups ?? []).filter(group => group.fix.kind !== 'needs');

/** One thing that needs fixing across the agents you can fix (people-and-agents-first,
 * Phase 4 AC3): the cause, which agents it stops, each case, and its one fix. It
 * shows beside the Needs you list, or as a drawer in a narrow window. */
export function FixDetails({ group, close, changed }: { group: FixGroup; close: () => void; changed: () => void }) {
  const m = useMutation();
  const [gone, setGone] = useState<string[]>([]);
  const [request, setRequest] = useState<string>();
  const url = fixUrl(group.fix);
  const items = group.items.filter(item => !gone.includes(item.ref));
  return <Dialog drawer title={group.title} close={close} busy={m.busy} status={m.status}>
    <section className="activity-details" aria-label="What needs fixing">
      <span className="activity-status problem">Needs fixing</span>
      <p className="activity-fix-agents"><span className="activity-fix-faces">{group.agents.slice(0, 4).map(agent =>
        <AgentFace key={agent.id} agent={{ id: agent.id, name: agent.name }} className="activity-face" />)}</span>
        {group.agents.map(agent => agent.name).join(', ')}</p>
      <ul className="activity-list">{items.map(item => <li key={item.ref}>
        <span className="activity-main"><strong>{item.agent_name}{item.detail ? `: ${item.detail}` : ''}</strong>{item.fix && <small>Fix: {item.fix}</small>}</span>
        {item.request && <Button onClick={() => setRequest(item.request)}>Open request</Button>}
        {group.fix.kind === 'need' && <Button disabled={m.busy} onClick={() => void m.run(async () => {
          await dismissNeed(item.ref); setGone(list => [...list, item.ref]); changed(); m.setStatus('Dismissed.'); })}>Dismiss</Button>}
      </li>)}</ul>
      {!items.length && <p className="activity-empty">Nothing left here.</p>}
      <div className="activity-detail-actions">
        {url ? <a className="ui-button primary" href={url}>{group.fix.label}</a>
          : group.fix.kind === 'request' && group.fix.ref ? <Button primary onClick={() => setRequest(group.fix.ref)}>{group.fix.label}</Button> : null}
      </div>
    </section>
    {request && <RequestDetails reply={request} close={() => setRequest(undefined)} />}
  </Dialog>;
}
