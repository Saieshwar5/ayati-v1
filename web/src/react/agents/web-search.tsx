import './tools.css';
import { useMutation } from '../requests';
import { saveAgent, type Agent, type Settings } from './model';

/** Web search is on unless its owner or an admin switched it off; agents saved
 * before it existed have no setting and are on (designs/web-search). */
export const searchesWeb = (settings: Settings) => settings.web_search !== false;

/** Connections tab: the agent's web search switch, for its owner and admins. */
export function WebSearchCard({ agent, saved }: { agent: Agent; saved: () => Promise<void> }) {
  const mutation = useMutation();
  const on = searchesWeb(agent.settings);
  const editable = Boolean(agent.editable);
  const change = (next: boolean) => void mutation.run(async () => {
    await saveAgent({ ...agent, people: undefined, settings: { ...agent.settings, web_search: next } });
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
      {!editable && <p className="ui-help">Only {agent.name}’s owner and admins can change this.</p>}
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </article>
  </section>;
}
