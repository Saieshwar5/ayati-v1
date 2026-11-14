import { addressPreview } from './model';
import './email.css';

/** Create step 2: Email sits with the other built-in tools, off by default.
 * Switching it on gives the agent its own address when it is created; people
 * write from the address they sign in with, so nobody links anything. */
export function EmailChoice({ name, workspace, domain, on, change }: {
  name: string; workspace: string; domain: string; on: boolean; change: (on: boolean) => void;
}) {
  return <li className="agent-email-choice">
    <span className="agent-app-text"><strong>Email</strong>
      <small>People in this workspace email {name} and get its answers in the same mail thread</small>
      <span className="agent-app-kinds"><span className="agent-kind">Off unless you switch it on</span></span>
      <small>{on ? `${name} gets its own address when you create it, like ${addressPreview(name, workspace, domain)}. People write from the address they sign in with; nothing to link.`
        : 'Cuckoo provides the address. Only people in this workspace get answers.'}</small>
    </span>
    <label className="ui-switch"><input type="checkbox" role="switch" checked={on}
      onChange={event => change(event.target.checked)} />
      <span>Activate email</span></label>
  </li>;
}
