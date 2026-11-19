import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { AgentFace, defaultRobot } from '../faces';
import { useMutation, useResource } from '../requests';
import { currentWorkspace, workspaceUrl } from '../../workspace-context';
import { loadAgents, saveAgentWithPicture, type Agent } from '../agents/model';
import { newAgent } from '../agents/create/create-flow';
import { assignSkill, createSkill } from '../agents/skills/model';
import { readAssistantAnswer, skillText } from '../agents/skills/writer-model';
import { agentSectionUrl } from '../agents/workspace-tabs';
import './agent-draft-card.css';

/** A new agent your assistant drafted in your chat with it (schema 139), under
 * its reply and for you alone. The whole draft is here to read. Nothing exists
 * until you press Create agent: the agent and its skills are then saved through
 * the ordinary routes, with you as its owner. Its ids come with the draft, so
 * creating again after a failure updates the same agent and saves each skill once. */
export function AgentDraftCard({ reply, ready, assistant }: { reply: string; ready: boolean; assistant: string }) {
  // Read again once the reply is complete: until then there is nothing to show.
  const load = useCallback(() => readAssistantAnswer(reply), [reply, ready]);
  const { data, setData } = useResource(load);
  const mutation = useMutation();
  const [saved, setSaved] = useState<Agent>();
  const [done, setDone] = useState(false);
  if (!data || data.state !== 'complete' || data.from !== 'chat' || !data.agent_id) return null;
  const id = data.agent_id, name = data.name ?? '', title = data.job_title ?? '', rules = data.ground_rules ?? '';
  const skills = data.skills ?? [], library = data.reuse ?? [], apps = data.missing_apps ?? [], used = data.used ?? [];
  const face = <AgentFace agent={{ id, name, avatar: defaultRobot(id) }} className="agent-draft-face" />;
  if (data.replaced && !data.created && !done) return <p className="agent-draft-replaced" role={mutation.status ? 'alert' : undefined}>{face}{name || 'This draft'} · earlier draft, replaced by the one below</p>;
  const created = done || Boolean(data.created);
  const create = () => mutation.run(async () => {
    // A later reply may have carried the draft on since this card was read.
    const now = await readAssistantAnswer(reply);
    if (now.replaced || now.created) { setData(now); if (now.replaced) throw new Error('A newer draft is below. Create that one.'); return; }
    const fresh = newAgent(await loadAgents());
    const start: Agent = saved ?? { ...fresh, id, name, role: title, audience: 'everyone',
      settings: { ...fresh.settings, avatar: defaultRobot(id), instructions: rules } };
    // A saved agent keeps its revision, so creating again updates it.
    await saveAgentWithPicture({ ...start, people: [] }, undefined, next => setSaved({ ...start, revision: next.revision, settings: next.settings }));
    for (const skill of skills) {
      const made = await createSkill(skill.id ?? crypto.randomUUID(), false, [{ path: 'SKILL.md', text: skillText(skill.name, skill.description, skill.steps) }]);
      await assignSkill(id, made.id, true);
    }
    for (const pick of library) await assignSkill(id, pick.id, true);
    setDone(true);
  });
  return <section className="agent-draft-card" aria-label={`Agent draft: ${name || 'new agent'}`}>
    <header>{face}
      <span><strong>{name || 'New agent'}</strong>{title && <small>{title}</small>}</span>
      <span className={`agent-draft-badge${created ? ' created' : ''}`}>{created ? 'Created' : 'Draft · not created'}</span>
    </header>
    {created
      ? <div className="agent-draft-part">
        <p>{name} is created and you are its owner.{apps.length > 0 && ` It can’t use ${apps.join(' or ')} until you connect it.`}</p>
        <div className="agent-draft-actions">
          <a className="ui-button primary" href={agentSectionUrl(id, 'Connections')}>Connect its apps</a>
          <a className="ui-button" href={agentSectionUrl(id, 'Overview')}>Open {name}’s page</a>
        </div>
      </div>
      : <>
        <div className="agent-draft-part">
          <h4>Ground rules <small>Always on</small></h4>
          {rules ? <pre>{rules}</pre> : <p className="ui-help">None yet. Ask {assistant} to write them.</p>}
        </div>
        {(skills.length > 0 || library.length > 0) && <div className="agent-draft-part">
          <h4>Skills <small>Opened only when a request needs them</small></h4>
          {skills.map(skill => <details key={skill.name}>
            <summary><strong>{skill.name}</strong><span>{skill.description}</span></summary>
            <pre>{skill.steps}</pre>
          </details>)}
          {library.map(pick => <p key={pick.id}><strong>{pick.name}</strong> <span>{pick.description} · from the library</span></p>)}
        </div>}
        {apps.length > 0 && <div className="agent-draft-part">
          <h4>Apps it will need</h4>
          <p>{apps.join(', ')}. You connect them on its Connections tab after creating; {assistant} can’t.</p>
        </div>}
        {used.length > 0 && <div className="agent-draft-part">
          <h4>What {assistant} used</h4>
          <ul>{used.map(line => <li key={line}>{line}</li>)}</ul>
        </div>}
        {(data.notes ?? []).length > 0 && <ul className="agent-draft-notes">{data.notes!.map(note => <li key={note}>{note}</li>)}</ul>}
        <footer>
          <div className="agent-draft-actions">
            <Button primary disabled={mutation.busy || !name} onClick={() => void create()}>{mutation.busy ? 'Creating…' : 'Create agent'}</Button>
            <a className="ui-button" href={workspaceUrl(`/?view=agents&setup=${encodeURIComponent(reply)}`)}>Review in New agent</a>
          </div>
          <small>{name ? `Everyone in ${currentWorkspace()?.name ?? 'this workspace'} will be able to talk to it, and you will be its owner. To change something first, tell ${assistant} or review it in New agent.`
            : `It has no name yet. Ask ${assistant} to name it, or review it in New agent.`}</small>
        </footer>
      </>}
    {mutation.status && <p className="agent-draft-error" role="alert">{mutation.status}</p>}
  </section>;
}
