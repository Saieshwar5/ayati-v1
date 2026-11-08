import './tools.css';
import type { ReactNode } from 'react';
import { useMutation } from '../requests';
import { saveAgent, type Agent, type Settings } from './model';

/** Web search is on unless its owner or a manager switched it off; agents saved
 * before it existed have no setting and are on (designs/web-search). */
export const searchesWeb = (settings: Settings) => settings.web_search !== false;

const about = 'Search the web and read public pages to answer questions';

/** Create step 2: the tools Cuckoo provides itself, above the admin's apps. */
export function BuiltInTools({ children }: { children: ReactNode }) {
  return <section className="agent-built-in" aria-labelledby="agent-built-in-title">
    <h4 id="agent-built-in-title" className="agent-group-label">Built into Cuckoo <small>Nothing to set up</small></h4>
    <ul className="agent-apps">{children}</ul>
  </section>;
}

/** Create step 2: web search, on for every new agent. */
export function WebSearchChoice({ name, on, change }: { name: string; on: boolean; change: (on: boolean) => void }) {
  return <li>
    <span className="agent-app-text"><strong>Web search</strong>
      <small>{about}</small>
      <span className="agent-app-kinds"><span className="agent-kind read">Reads</span><span className="agent-kind">On for every new agent</span></span>
      <small>{name} lists the pages it used under each answer. It only reads: it can’t sign in, buy or fill in forms.</small>
    </span>
    <label className="ui-switch"><input type="checkbox" role="switch" checked={on} onChange={event => change(event.target.checked)} />
      <span>Use web search</span></label>
  </li>;
}

/** Tools tab: the agent's web search switch, for its owner and managers. */
export function WebSearchCard({ agent, saved }: { agent: Agent; saved: () => Promise<void> }) {
  const mutation = useMutation();
  const on = searchesWeb(agent.settings);
  const editable = Boolean(agent.editable);
  const change = (next: boolean) => void mutation.run(async () => {
    await saveAgent({ ...agent, people: undefined, managers: undefined, settings: { ...agent.settings, web_search: next } });
    await saved();
    mutation.setStatus(next ? `${agent.name} can search the web.` : `${agent.name} no longer searches the web.`);
  });
  return <section className="agent-built-in" aria-labelledby="agent-web-search-title">
    <h3 className="agent-group-label">Built into Cuckoo</h3>
    <article className="agent-tool" aria-labelledby="agent-web-search-title">
      <header>
        <span><strong id="agent-web-search-title">Web search</strong>
          <small>{on ? `${agent.name} can search the web and read public pages. The pages it reads are listed under its answers.`
            : `Off: ${agent.name} answers from its apps and the Library only.`}</small></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={on} disabled={!editable || mutation.busy}
          onChange={event => change(event.target.checked)} />
          <span>Use web search</span></label>
      </header>
      {!editable && <p className="ui-help">Only {agent.name}’s owner, its managers and admins can change this.</p>}
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </article>
  </section>;
}
