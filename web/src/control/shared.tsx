import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { useMutation } from '../react/requests';

/** Every change in Cuckoo Control needs a reason; the change log keeps it. */
export function ReasonDialog({ title, children, confirm, danger = false, cli, onConfirm, onClose, extraValid = true }: {
  title: string; children?: ReactNode; confirm: string; danger?: boolean; cli?: string; extraValid?: boolean;
  onConfirm: (reason: string) => Promise<void>; onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const opener = useRef<Element | null>(null);
  const [reason, setReason] = useState('');
  const mutation = useMutation();
  const titleId = useId();
  useEffect(() => {
    opener.current = document.activeElement;
    const node = dialog.current;
    node?.showModal();
    return () => {
      if (node?.open) node.close();
      (opener.current as HTMLElement | null)?.focus?.();
    };
  }, []);
  const ready = reason.trim().length >= 4 && extraValid;
  return <dialog ref={dialog} className={`cc-dialog${danger ? ' danger' : ''}`} aria-labelledby={titleId}
    onCancel={event => { event.preventDefault(); if (!mutation.busy) onClose(); }}>
    <form method="dialog" onSubmit={event => { event.preventDefault(); if (!ready) return; void mutation.run(async () => { await onConfirm(reason.trim()); onClose(); }); }}>
      <header><h2 id={titleId}>{title}</h2></header>
      <div className="cc-dialog-body">
        {children}
        <label className="cc-field"><span>Reason <span className="cc-dim">(saved in the change log)</span></span>
          <textarea name="reason" rows={2} maxLength={300} value={reason} onChange={event => setReason(event.target.value)}
            placeholder="Why are you making this change?" required />
        </label>
        {cli && <div className="cc-cli"><span aria-hidden="true">$</span> {cli}</div>}
        {mutation.status && <p className="cc-alert" role="alert">{mutation.status}</p>}
      </div>
      <footer>
        <button type="button" className="cc-btn" onClick={onClose} disabled={mutation.busy}>Cancel</button>
        <button type="submit" className={`cc-btn ${danger ? 'danger-fill' : 'primary'}`} disabled={!ready || mutation.busy}>
          {mutation.busy ? 'Saving…' : confirm}
        </button>
      </footer>
    </form>
  </dialog>;
}

/** Re-reads every `ms` while the tab is visible; stops on unmount. */
export function usePolling(refresh: () => Promise<unknown>, ms: number) {
  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState !== 'hidden') void refresh().catch(() => {});
    }, ms);
    return () => clearInterval(timer);
  }, [refresh, ms]);
}

/** A small trend line; decorative, the number beside it carries the meaning. */
export function Sparkline({ values, width = 72, height = 24, colour = 'var(--cc-accent)' }: { values: number[]; width?: number; height?: number; colour?: string }) {
  if (values.length < 2) return null;
  const max = Math.max(...values), min = Math.min(...values), range = max - min || 1;
  const points = values.map((v, i) => `${(i * width / (values.length - 1)).toFixed(1)},${(height - 2 - (v - min) / range * (height - 4)).toFixed(1)}`).join(' ');
  return <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
    <polyline points={points} fill="none" stroke={colour} strokeWidth="1.5" strokeLinejoin="round" />
  </svg>;
}

const HUES = ['#f0883e', '#3dd6c6', '#bc8cff', '#8b9cff', '#e3b341', '#ff8e88'];
/** One stable colour per provider name, used in every chart and table. */
export function providerHue(name: string) {
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return HUES[hash % HUES.length];
}

export function Tag({ tone, children }: { tone: 'ok' | 'warn' | 'bad' | 'off' | 'new'; children: ReactNode }) {
  return <span className={`cc-tag ${tone}`}>{children}</span>;
}

export function Loading({ error, label }: { error: string; label: string }) {
  return error ? <p className="cc-alert" role="alert">{error}</p> : <p className="cc-dim" role="status">Loading {label}…</p>;
}
