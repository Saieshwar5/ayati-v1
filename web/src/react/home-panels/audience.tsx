import { useEffect, useId, useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { chatRequest, type Conversation } from '../chat/model';
import { audienceLabel, shareHomePanel, type Audience, type PersonalPanel } from './model';

/** Team panels (people-and-agents-first, Phase 3 HP4): the owner chooses who sees a
 * panel. Only a panel whose every number is read straight from the source can be
 * shared; others see the numbers, and the agent's notes only if they can see the
 * conversation they were written in. Each person still arranges their own Home. */
export function PanelAudience({ panel, refresh }: { panel: PersonalPanel; refresh: () => Promise<void> }) {
  const [open, setOpen] = useState(false);
  return <>
    <Button onClick={() => setOpen(true)}>Who sees this</Button>
    {open && <AudienceDialog panel={panel} close={() => setOpen(false)} refresh={refresh} />}
  </>;
}

function AudienceDialog({ panel, close, refresh }: { panel: PersonalPanel; close: () => void; refresh: () => Promise<void> }) {
  const [audience, setAudience] = useState<Audience>(panel.audience ?? 'personal');
  const [chat, setChat] = useState(panel.audience_chat ?? '');
  const [channels, setChannels] = useState<Conversation[]>();
  const m = useMutation();
  const fields = useId();
  const attempt = useRef<{ key: string; id: string }>(undefined);
  useEffect(() => {
    let live = true;
    chatRequest<{ chats: Conversation[] }>('/chats')
      .then(data => { if (live) setChannels(data.chats.filter(c => c.kind === 'channel' && !c.archived && panel.agent && c.agents.includes(panel.agent))); })
      .catch(() => { if (live) setChannels([]); });
    return () => { live = false; };
  }, [panel.agent]);
  const widening = audience !== 'personal';
  const ready = panel.shareable?.ok !== false || !widening;
  const save = () => void m.run(async () => {
    const content = { revision: panel.audience_revision ?? 0, audience, chat: audience === 'channel' ? chat : null };
    const key = JSON.stringify(content);
    if (attempt.current?.key !== key) attempt.current = { key, id: crypto.randomUUID() };
    await shareHomePanel(panel.id, { ...content, request: attempt.current.id });
    await refresh();
    close();
  });
  const name = panel.active?.definition?.title ?? 'This panel';
  return <Dialog title="Who sees this panel" close={close} busy={m.busy}>
    <p>{name} · now for {audienceLabel(panel)}</p>
    <fieldset className="workspace-panel-audience" disabled={m.busy}>
      <legend>Show it to</legend>
      <label><input type="radio" name={`${fields}-audience`} checked={audience === 'personal'} onChange={() => setAudience('personal')} /> Only me</label>
      <label><input type="radio" name={`${fields}-audience`} checked={audience === 'channel'} onChange={() => setAudience('channel')}
        disabled={!channels?.length} /> Members of a channel</label>
      {audience === 'channel' && <select aria-label="Channel" value={chat} onChange={event => setChat(event.target.value)}>
        <option value="">Choose a channel</option>
        {channels?.map(c => <option key={c.id} value={c.id}>#{c.name}</option>)}
      </select>}
      <label><input type="radio" name={`${fields}-audience`} checked={audience === 'workspace'} onChange={() => setAudience('workspace')} /> Everyone in the workspace</label>
    </fieldset>
    {channels && !channels.length && <small>{panel.agent_name ?? 'The agent'} isn't in any channel you can choose.</small>}
    {widening && !ready && <p role="status">{panel.shareable?.reason}</p>}
    {widening && ready && <p className="ui-help">They'll see its numbers, read straight from the source, and when it was updated. The
      agent's notes stay with people who can see the conversation they came from. Updates still run as you; each person can hide or move it.</p>}
    <div className="panel-maintenance-actions">
      <Button primary disabled={m.busy || !ready || (audience === 'channel' && !chat) || (audience === (panel.audience ?? 'personal') && (audience !== 'channel' || chat === panel.audience_chat))}
        onClick={save}>{m.busy ? 'Saving…' : widening ? `Share with ${audience === 'workspace' ? 'everyone' : 'this channel'}` : 'Keep it to me'}</Button>
    </div>
    {m.status && <p role="alert">{m.status}</p>}
  </Dialog>;
}
