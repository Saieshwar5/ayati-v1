/** Create step 2: WhatsApp sits with the other built-in tools, off by default.
 * Switching it on gives the agent a WhatsApp number when it is created; nothing
 * else to set up. */
export function WhatsappChoice({ name, on, change }: { name: string; on: boolean; change: (on: boolean) => void }) {
  return <li className="agent-whatsapp-choice">
    <span className="agent-app-text"><strong>WhatsApp</strong>
      <small>People in this workspace message {name} on WhatsApp and get its answers there</small>
      <span className="agent-app-kinds"><span className="agent-kind">Off unless you switch it on</span></span>
      <small>{on ? `${name} gets its own WhatsApp number when you create it. People link their own number in their profile to message it.`
        : 'Cuckoo provides the number. Only people who link their WhatsApp number can message it.'}</small>
    </span>
    <label className="ui-switch"><input type="checkbox" role="switch" checked={on}
      onChange={event => change(event.target.checked)} />
      <span>Activate WhatsApp</span></label>
  </li>;
}
