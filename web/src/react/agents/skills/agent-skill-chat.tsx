import { useRef, useState } from 'react';
import { Button } from '../../ui';
import { useMutation } from '../../requests';
import type { Agent } from '../model';
import { assignSkill, createSkill, loadAgentSkills, loadSkill, type SkillFile } from './model';
import { SetupChat, useSkillWriting } from './setup-chat';
import { lineDiff, saveSkillVersion, skillText, stepsOf, type LibraryPick, type WriterAnswer } from './writer-model';

/** A skill the agent has, with every file so a new version keeps them. */
type Saved = { id: string; name: string; description: string; steps: string; version: number; files: SkillFile[] };
type Proposal = { key: string } & (
  | { kind: 'rules'; after: string }
  | { kind: 'change'; skill: Saved; description: string; steps: string }
  | { kind: 'new'; name: string; description: string; steps: string }
  | { kind: 'remove'; skill: Saved }
  | { kind: 'reuse'; pick: LibraryPick });

const main = (files: SkillFile[]) => files.find(file => file.path === 'SKILL.md')?.text ?? '';

/** Change with chat on the Instructions tab (designs/agent-built-skills 5). The
 * owner or an admin says what changed; the writer proposes each change as a
 * diff, and nothing changes until they apply it. Ground rules go into the
 * instructions box, to be saved there. */
export function AgentSkillChat({ agent, rules, takeRules, changed }: {
  agent: Agent; rules: string; takeRules: (text: string) => void; changed: () => void;
}) {
  const writing = useSkillWriting();
  const saved = useRef<Saved[] | undefined>(undefined);
  const [proposals, setProposals] = useState<Proposal[]>([]);
  const [done, setDone] = useState<Record<string, string>>({});
  const applying = useMutation();
  const load = async () => {
    if (saved.current) return saved.current;
    const { skills } = await loadAgentSkills(agent.id);
    const details = await Promise.all(skills.map(skill => loadSkill(skill.id)));
    saved.current = details.map(detail => ({ id: detail.id, name: detail.name, description: detail.description,
      steps: stepsOf(main(detail.files)), version: detail.version, files: detail.files }));
    return saved.current;
  };
  const propose = (answer: WriterAnswer) => {
    const current = saved.current ?? [];
    const key = () => crypto.randomUUID();
    const next: Proposal[] = [];
    if (answer.ground_rules && answer.ground_rules.trim() !== rules.trim()) next.push({ key: key(), kind: 'rules', after: answer.ground_rules });
    for (const { name, description, steps } of answer.skills) {
      const skill = current.find(item => item.name === name);
      next.push(skill ? { key: key(), kind: 'change', skill, description, steps } : { key: key(), kind: 'new', name, description, steps });
    }
    for (const name of answer.remove) {
      const skill = current.find(item => item.name === name);
      if (skill) next.push({ key: key(), kind: 'remove', skill });
    }
    for (const pick of answer.reuse) if (!current.some(item => item.id === pick.id)) next.push({ key: key(), kind: 'reuse', pick });
    setProposals(next); setDone({});
  };
  const submit = () => writing.send(async () => ({ name: agent.name, role: agent.role, agent: agent.id, ground_rules: rules,
    skills: (await load()).map(({ name, description, steps }) => ({ name, description, steps, saved: true })) }), propose);
  const apply = (proposal: Proposal) => void applying.run(async () => {
    let result = '';
    if (proposal.kind === 'rules') {
      takeRules(proposal.after);
      result = 'Now in the instructions box. Save changes to keep it.';
    } else if (proposal.kind === 'change') {
      const { skill } = proposal;
      const text = skillText(skill.name, proposal.description, proposal.steps);
      const files = [...skill.files.filter(file => file.path !== 'SKILL.md'), { path: 'SKILL.md', text }];
      const next = await saveSkillVersion(skill.id, skill.version, files);
      result = `Saved as version ${next.version}.`;
    } else if (proposal.kind === 'new') {
      // The proposal's key saves it once, even when retried.
      const created = await createSkill(proposal.key, false, [{ path: 'SKILL.md', text: skillText(proposal.name, proposal.description, proposal.steps) }]);
      await assignSkill(agent.id, created.id, true);
      result = `Created and added to ${agent.name}.`;
    } else if (proposal.kind === 'remove') {
      await assignSkill(agent.id, proposal.skill.id, false);
      result = `Removed from ${agent.name}.`;
    } else {
      await assignSkill(agent.id, proposal.pick.id, true);
      result = `Added to ${agent.name}.`;
    }
    setDone(old => ({ ...old, [proposal.key]: result }));
    if (proposal.kind !== 'rules') { saved.current = undefined; changed(); }
  });
  const discard = (key: string) => setProposals(old => old.filter(item => item.key !== key));
  const list = proposals.length > 0 && <section className="skill-proposals" aria-label="Proposed changes">
    <h4>Proposed changes</h4>
    {proposals.map(proposal => <article key={proposal.key} className="skill-proposal">
      <ProposalBody proposal={proposal} rules={rules} />
      {done[proposal.key]
        ? <p className="ui-help" role="status">{done[proposal.key]}</p>
        : <div className="draft-actions">
          <Button primary disabled={applying.busy} onClick={() => apply(proposal)}>{action(proposal, agent.name)}</Button>
          <Button disabled={applying.busy} onClick={() => discard(proposal.key)}>Discard</Button>
        </div>}
    </article>)}
    {applying.status && <p className="setup-chat-error" role="alert">{applying.status}</p>}
  </section>;
  if (writing.loading || !writing.available) return null;
  return <SetupChat name={agent.name} title={`Change with ${agent.name}`} subtitle={`Private · only ${agent.name}’s owner and admins`} writing={writing} submit={submit} extra={list || undefined}
    greeting={`Tell me what changed, and I’ll propose new ground rules or skills for ${agent.name}. Nothing changes until you apply it.`} />;
}

function action(proposal: Proposal, agent: string) {
  if (proposal.kind === 'rules') return 'Use in the instructions box';
  if (proposal.kind === 'change') return `Save as version ${proposal.skill.version + 1}`;
  if (proposal.kind === 'new') return `Create and add to ${agent}`;
  if (proposal.kind === 'remove') return `Remove from ${agent}`;
  return `Add to ${agent}`;
}

function ProposalBody({ proposal, rules }: { proposal: Proposal; rules: string }) {
  if (proposal.kind === 'remove') return <p><strong>Remove {proposal.skill.name}</strong></p>;
  if (proposal.kind === 'reuse') return <p><strong>Add {proposal.pick.name}</strong> from the library: {proposal.pick.description}</p>;
  if (proposal.kind === 'new') return <><p><strong>New skill: {proposal.name}</strong> {proposal.description}</p><pre className="skill-diff">{proposal.steps}</pre></>;
  const [title, before, after] = proposal.kind === 'rules'
    ? ['Ground rules', rules, proposal.after]
    : [`${proposal.skill.name} · version ${proposal.skill.version}`, main(proposal.skill.files), skillText(proposal.skill.name, proposal.description, proposal.steps)];
  return <><p><strong>{title}</strong></p>
    <pre className="skill-diff">{lineDiff(before.trim(), after.trim()).map((line, index) =>
      <span key={index} className={`skill-diff-${line.kind}`}>{line.kind === 'added' ? '+ ' : line.kind === 'removed' ? '− ' : '  '}{line.text}{'\n'}</span>)}</pre></>;
}
