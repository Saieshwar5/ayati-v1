import { AgentFace, AiTag } from '../faces';
import { PageIcon } from '../workspace-shell';
import { LockIcon } from './conversation-face';
import type { Participant } from './model';
import './assistant-start.css';

/** The top of your empty assistant chat: who it is, and who sees its work. */
export function AssistantIntro({ assistant }: { assistant: Participant }) {
  return <div className="chat-assistant-intro">
    <AgentFace agent={assistant} className="chat-avatar chat-assistant-intro-face" />
    <h3>{assistant.name} <AiTag /></h3>
    <p><LockIcon size={13} />Only you can talk to {assistant.name} · admins can see its work</p>
  </div>;
}

/** Things your assistant can always do. Each starts a message you finish yourself;
 * nothing is sent until you send it. */
const EXAMPLES = [
  { label: 'Set up an agent', text: 'Help me set up a new agent that ', icon: <PageIcon name="agents" /> },
  { label: 'Summarize a file', text: 'Summarize the file I am attaching: ', icon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" /><path d="M14 3v5h5M9 13h6M9 17h4" /></svg> },
  { label: 'Draft a message', text: 'Help me write a message to my team about ', icon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16z" /><path d="m13 7 4 4" /></svg> },
];

export function ExampleRequests({ use }: { use: (text: string) => void }) {
  return <div className="chat-examples" role="group" aria-label="Examples">
    {EXAMPLES.map(example => <button type="button" key={example.label} className="chat-example" onClick={() => use(example.text)}>
      {example.icon}{example.label}
    </button>)}
  </div>;
}
