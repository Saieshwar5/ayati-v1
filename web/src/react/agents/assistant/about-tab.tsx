import { useState } from 'react';
import { useMutation } from '../../requests';
import { saveAgent, type Agent } from '../model';
import { AgentSkills } from '../skills/agent-skills';
import { AgentSkillChat } from '../skills/agent-skill-chat';
import { AlwaysRead } from '../../library/always-read';
import { EverythingItReceives, SystemPromptCard } from '../system-prompt';
import { AboutYou } from './about-you';
import { AssistantRules } from './rules';
import './assistant.css';

/** A member's own assistant, in place of the Instructions tab: what it knows
 * about its person, its rules and its skills. There is no instructions box:
 * rules are short lines kept as the agent's instructions, one per line. The
 * notes about a person are theirs alone, so an admin looking at someone
 * else's assistant sees its rules and skills only. */
export function AssistantAboutTab({ agent, rules, saved }: { agent: Agent; rules: string; saved: () => Promise<void> }) {
  // The revision the list was last saved at, so a save never overwrites a newer change.
  const [base, setBase] = useState(agent);
  const [previewing, setPreviewing] = useState(false);
  const [skillsRevision, setSkillsRevision] = useState(0);
  const mutation = useMutation();
  if (base.revision < agent.revision) setBase(agent);
  const text = base.settings.instructions;
  const save = async (next: string, done: string) => {
    let kept = false;
    await mutation.run(async () => {
      const settings = { ...base.settings, instructions: next };
      const result = await saveAgent({ ...base, people: undefined, settings });
      setBase({ ...base, revision: result.revision, settings });
      kept = true;
      mutation.setStatus(done);
      await saved();
    });
    return kept;
  };
  return <div className="agent-instructions-tab">
    <div className="agent-create-main">
      {agent.is_owner && <AboutYou name={agent.name} />}
      <AssistantRules name={agent.name} text={text} editable={Boolean(agent.editable)} busy={mutation.busy} status={mutation.status} save={save} />
      <AgentSkills key={skillsRevision} agent={agent} />
      <section className="agent-knows" aria-label={`What ${agent.name} always reads`}>
        <AlwaysRead agent={agent} />
      </section>
    </div>
    <aside className="agent-create-side">
      {agent.editable && <AgentSkillChat agent={agent} rules={text}
        changed={() => setSkillsRevision(revision => revision + 1)} />}
      <SystemPromptCard name={agent.name} rules={rules} open={() => setPreviewing(true)} />
    </aside>
    {previewing && <EverythingItReceives draft={{ name: agent.name, role: agent.role, instructions: text, agent: agent.id }} close={() => setPreviewing(false)} />}
  </div>;
}
