import { useEffect, useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { QuestionDialog } from '../questions/dialog';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { useMutation } from '../requests';
import { acknowledgeAttention, attentionKey, attentionLabel, attentionSource, type AttentionItem, type AttentionView } from './model';
import { useAttention } from './use-attention';
import './attention.css';

export function AttentionInbox({ close, privateInbox, privateCount = 0 }: {
  close: () => void; privateInbox?: () => void; privateCount?: number;
}) {
  const [view, setView] = useState<AttentionView>('needs_you');
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const [selected, select] = useState<AttentionItem>();
  const { data, error, loading, refresh } = useAttention(view, cursors.at(-1) ?? null, !selected);
  const mutation = useMutation();
  const alive = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);
  const opener = useRef<HTMLElement | null>(null), restoreFocus = useRef(false);
  useEffect(() => {
    if (!selected && restoreFocus.current) {
      restoreFocus.current = false;
      if (opener.current?.isConnected) opener.current.focus();
    }
  }, [selected]);
  const changeView = (next: AttentionView) => { setView(next); setCursors([null]); mutation.setStatus(''); };
  const open = (item: AttentionItem) => {
    opener.current = document.activeElement as HTMLElement | null;
    void mutation.run(async () => {
      try {
        await acknowledgeAttention(item);
        if (!alive.current) return;
        if (item.kind === 'outcome') location.assign(attentionSource(item));
        else select(item);
      } catch (error) { await refresh(); throw error; }
    });
  };
  const review = (item: AttentionItem) => void mutation.run(async () => {
    try { await acknowledgeAttention(item, true); await refresh(); }
    catch (error) { await refresh(); throw error; }
  });
  const back = () => { restoreFocus.current = true; select(undefined); };
  return <Dialog drawer title="Your attention" close={close} busy={mutation.busy} status={mutation.status}>
    <p className="ui-help">Requests and outcomes from work you can currently access. Reading does not answer a question or approve an action.</p>
    <div className="ui-tabs" role="tablist" aria-label="Attention views">
      {(['needs_you', 'recent'] as const).map((tab, index) => <Button key={tab} ref={node => { tabs.current[index] = node; }}
        role="tab" id={`attention-${tab}`} aria-controls="attention-items" aria-selected={tab === view} tabIndex={tab === view ? 0 : -1}
        onClick={() => changeView(tab)} onKeyDown={event => {
          const next = ['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)
            ? event.key === 'Home' ? 0 : event.key === 'End' ? 1 : 1 - index : -1;
          if (next >= 0) { event.preventDefault(); changeView(next ? 'recent' : 'needs_you'); tabs.current[next]?.focus(); }
        }}>{tab === 'needs_you' ? 'Needs you' : 'Recent work'}</Button>)}
    </div>
    {error && <p role="alert">{error}</p>}
    {loading && <p role="status">Checking saved work…</p>}
    {data?.available === false && <p>Shared-work notifications are unavailable in this workspace. Private agent work remains in its own inbox.</p>}
    <section id="attention-items" role="tabpanel" aria-labelledby={`attention-${view}`}>
      {data?.available && !data.items.length && <p>{view === 'needs_you' ? 'No requests or unreviewed problems available to you.' : 'No recent work available to you.'}</p>}
      {data?.items.map(item => <article className="attention-row" key={attentionKey(item)}>
        <div className="attention-row-heading"><strong>{item.title.replace(/^\[Ayati request mode:[^\]]*\]\s*/, '')}</strong>
          {!item.read && <span className="attention-unread">Unread</span>}</div>
        <p>{item.agent_name} · {item.conversation_name}{item.archived ? ' · Archived' : ''}</p>
        <p>{attentionLabel(item)} · <time dateTime={new Date(item.at).toISOString()}>{new Date(item.at).toLocaleString()}</time>{item.reviewed ? ' · Reviewed' : ''}</p>
        {item.status === 'uncertain' && <p className="ui-help">Check the business system. This entry cannot repeat or confirm the action.</p>}
        <div className="attention-row-actions"><Button onClick={() => open(item)}>{item.kind === 'question' ? 'Answer question' : item.kind === 'approval' ? 'Review action' : 'Open work'}</Button>
          {item.kind !== 'outcome' && <a href={attentionSource(item)}>Source conversation{item.thread ? ' · Thread' : ''}</a>}
          {item.kind === 'outcome' && item.status !== 'complete' && !item.reviewed && <Button onClick={() => review(item)}>Mark outcome reviewed</Button>}
        </div>
      </article>)}
    </section>
    <nav className="attention-pagination" aria-label="Attention pages">
      {cursors.length > 1 && <Button disabled={loading} onClick={() => setCursors(current => current.slice(0, -1))}>Newer items</Button>}
      {data?.next_before && <Button disabled={loading} onClick={() => setCursors(current => [...current, data.next_before])}>Older items</Button>}
      <Button disabled={loading} onClick={() => { if (cursors.length > 1) setCursors([null]); else void refresh(); }}>Refresh attention</Button>
    </nav>
    {privateInbox && <Button onClick={privateInbox}>Private runtime inbox{privateCount ? ` · ${privateCount}` : ''}</Button>}
    <p className="ui-help">Pending requests stay here after reading. Successful work appears in Recent work. Reviewing an outcome does not change or restart the task.</p>
    {selected?.kind === 'question' && <QuestionDialog key={selected.id} id={selected.id} close={back} />}
    {selected?.kind === 'approval' && <ApprovalDialog key={selected.id} id={selected.id} close={back} />}
  </Dialog>;
}
