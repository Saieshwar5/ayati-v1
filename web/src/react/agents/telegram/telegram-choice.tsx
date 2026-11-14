/** Create step 2: Telegram sits with the other built-in tools, off by default.
 * Switching it on asks for the agent's own bot when the agent is created. */
export function TelegramChoice({ name, on, connected, change }: { name: string; on: boolean; connected: boolean; change: (on: boolean) => void }) {
  return <li className="agent-telegram-choice">
    <span className="agent-app-text"><strong>Telegram</strong>
      <small>People in this workspace talk to {name} on Telegram and get its answers there</small>
      <span className="agent-app-kinds"><span className="agent-kind">Off unless you switch it on</span></span>
      <small>{!connected ? 'A workspace admin needs to connect Telegram in Settings › Workspace first.'
        : on ? `${name} gets its own Telegram bot. The workspace’s Telegram owner creates it with one tap.`
          : 'Only members who connect their Telegram in their profile can talk to it.'}</small>
    </span>
    <label className="ui-switch"><input type="checkbox" role="switch" checked={on} disabled={!connected}
      onChange={event => change(event.target.checked)} />
      <span>Activate Telegram</span></label>
  </li>;
}
