import { useRef, type ReactNode } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { openAgentChat } from '../chat/direct-conversation';
import { workspaceUrl } from '../../workspace-context';
import type { AttentionItem } from '../attention/model';
import type { PersonalPanel, Snapshot } from './model';

// The panel frame (people-and-agents-first, Phase 1e): who keeps it, what changed in
// the agent's words, honest freshness, what needs you, and links to the proof and the
// agent. Panels take no input: every decision opens Activity.

const activity = (path: string) => workspaceUrl(`/?view=activity&${path}`);

/** "20 min ago", "3 h ago", "yesterday". */
export function since(ms: number, now = Date.now()): string {
  const minutes = Math.max(0, Math.round((now - ms) / 60000));
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  return days === 1 ? 'yesterday' : `${days} days ago`;
}

/** What the platform (not the agent) says about the panel's freshness, if anything
 * is wrong. Uses the update status the server works out from the panel's maximum age. */
export function healthLine(panel: PersonalPanel, snapshot: Snapshot, now = Date.now()): { text: string; fix?: string } | null {
  const status = panel.maintenance?.status;
  const read = snapshot.checked_ms ?? snapshot.saved_ms;
  const age = read ? since(read, now).replace(' ago', '') : undefined;
  const lastUpdate = panel.maintenance?.last_update ?? undefined;
  const fix = lastUpdate ? activity(`tab=history&run=${encodeURIComponent(lastUpdate)}`) : undefined;
  if (status === 'delayed') return { text: `Not updated for ${age ?? 'a while'}. These numbers may be out of date.`, fix };
  if (status === 'failed') return { text: 'The last update could not complete. Showing the previous result.', fix };
  if (status === 'blocked') return { text: 'Updates stopped: access to a source changed. Review the panel’s update settings.' };
  return null;
}

/** "What changed", written by the agent with the update; nothing when it wrote none. */
export function PanelNote({ snapshot }: { snapshot: Snapshot }) {
  if (!snapshot.what_changed) return null;
  return <p className="panel-changed"><strong>What changed:</strong> {snapshot.what_changed}</p>;
}

export function PanelHealth({ panel, snapshot }: { panel: PersonalPanel; snapshot: Snapshot }) {
  const health = healthLine(panel, snapshot);
  if (!health) return null;
  return <p className="panel-stale" role="status">{health.text}
    {health.fix && <> <a href={health.fix}>How to fix it →</a></>}</p>;
}

/** The waiting approvals and questions from this panel's agent that you can act on. */
export function PanelNeedsYou({ agent, items, more }: { agent?: string | null; items?: AttentionItem[]; more?: boolean }) {
  const count = agent ? (items ?? []).filter(item => item.agent === agent && item.kind !== 'outcome').length : 0;
  if (!count) return null;
  return <a className="panel-needs-you" href={activity('tab=needs')}>
    {count}{more ? '+' : ''} {count === 1 && !more ? 'needs' : 'need'} you <span aria-hidden="true">→</span></a>;
}

/** Who keeps it, when it was updated, and the two links: the proof in Activity and a
 * conversation with the agent about this panel. */
export function PanelFooterLine({ panel, snapshot, needs }: { panel: PersonalPanel; snapshot: Snapshot; needs?: ReactNode }) {
  const ask = useMutation();
  const attempt = useRef<string>(undefined);
  const title = snapshot.definition?.title ?? 'this panel';
  const updated = snapshot.saved_ms ? `Updated ${since(snapshot.saved_ms)}` : undefined;
  const at = snapshot.checked_ms ? new Date(snapshot.checked_ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : undefined;
  // Numbers the controller read straight from the source say so (living panels, HP3).
  const read = at ? snapshot.from_source ? `numbers from the source, ${at}` : `sources read ${at}` : undefined;
  return <div className="panel-frame-line">
    <small>Kept by {panel.agent_name ?? 'an agent'} · for you{updated ? ` · ${updated}` : ''}{read ? ` · ${read}` : ''}</small>
    <span className="panel-frame-links">
      {needs}
      {snapshot.task && <a href={activity(`tab=history&run=${encodeURIComponent(snapshot.task)}`)}>Show proof</a>}
      {panel.agent && <Button className="panel-ask" disabled={ask.busy}
        onClick={() => void ask.run(() => openAgentChat({ id: panel.agent!, name: panel.agent_name ?? 'Agent' }, `About the ${title} panel: `, attempt))}>
        Ask about this</Button>}
    </span>
    {ask.status && <small role="alert">{ask.status}</small>}
  </div>;
}
