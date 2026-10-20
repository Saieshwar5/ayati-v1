import { useRef } from 'react';
import type { Participant } from './model';

/** Keep recipient selection available without making the composer a second toolbar. */
export function Recipient({ agents, value, change, disabled = false }: {
  agents: Participant[]; value: string; change: (value: string) => void; disabled?: boolean;
}) {
  const menu = useRef<HTMLDetailsElement>(null);
  const name = agents.find(agent => agent.id === value)?.name;
  return <details ref={menu} className="chat-recipient" onBlur={event => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) event.currentTarget.open = false;
  }}>
    <summary aria-label={name ? `Ask ${name}; change recipient` : 'Message everyone; choose recipient'} title={name ? `Ask ${name}` : 'Choose recipient'} data-targeted={!!value}>@</summary>
    <div><label>Send to<select aria-label="Message everyone or ask an agent" value={value} disabled={disabled} onChange={event => { change(event.target.value); if (menu.current) menu.current.open = false; }}><option value="">Everyone</option>{agents.map(agent => <option key={agent.id} value={agent.id}>Ask {agent.name}</option>)}</select></label></div>
  </details>;
}
