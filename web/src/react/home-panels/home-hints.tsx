import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import { openAgentChat } from '../chat/direct-conversation';
import type { AttentionPage } from '../attention/model';

/** Home's one line (HP7): "3 things need you →", only when something does. */
export function NeedsYouLine({ page }: { page?: AttentionPage | null }) {
  const waiting = page?.items.filter(item => item.kind !== 'outcome').length ?? 0;
  if (!waiting) return null;
  return <a className="home-needs-line" href={workspaceUrl('/?view=activity&tab=needs')}>
    {waiting}{page?.next_before ? '+' : ''} {waiting === 1 && !page?.next_before ? 'thing needs' : 'things need'} you <span aria-hidden="true">→</span></a>;
}

type Suggestion = { agent: string; agent_name: string; ask: string; count: number };
const hidden = (): string[] => { try { return JSON.parse(localStorage.getItem('home-panel-suggestions-hidden') ?? '[]'); } catch { return []; } };

/** Panels worth having (HP7): a question you keep asking an agent could live on Home. */
export function PanelSuggestions() {
  const [items, setItems] = useState<Suggestion[]>([]);
  const [gone, setGone] = useState<string[]>(hidden);
  const ask = useMutation();
  const attempt = useRef<string>(undefined);
  useEffect(() => {
    let live = true;
    identityRequest<{ suggestions: Suggestion[] }>(`/workspaces/${workspaceId}/home-panels/suggestions`)
      .then(data => { if (live) setItems(data.suggestions); }).catch(() => {});
    return () => { live = false; };
  }, []);
  const key = (s: Suggestion) => `${s.agent}/${s.ask.toLowerCase()}`;
  const shown = items.filter(s => !gone.includes(key(s)));
  if (!shown.length) return null;
  const dismiss = (s: Suggestion) => {
    const next = [...gone, key(s)];
    setGone(next);
    try { localStorage.setItem('home-panel-suggestions-hidden', JSON.stringify(next)); } catch { /* per-device convenience only */ }
  };
  return <section className="home-panel-suggestions" aria-label="Panel suggestions">
    {shown.map(s => <p key={key(s)}>
      <span>You asked {s.agent_name} “{s.ask}” {s.count} times lately. A panel could keep the answer on Home.</span>
      <Button disabled={ask.busy} onClick={() => void ask.run(() => openAgentChat({ id: s.agent, name: s.agent_name }, `Make me a Home panel that answers: ${s.ask}`, attempt))}>Ask for a panel</Button>
      <Button onClick={() => dismiss(s)}>Not now</Button>
    </p>)}
    {ask.status && <small role="alert">{ask.status}</small>}
  </section>;
}
