import { messageHtml } from '../../markdown';
import { MessageFiles } from '../results/files';
import type { Attachment } from './model';
import './forward-message.css';

/** A copy of the message a forward carries, as it was when forwarded (schema 125).
 * `files` names the forward's attachments that came with that message. */
export type Forwarded = { name: string; kind: 'user' | 'agent'; place: string; at: number; text: string;
  files?: { id: string; name: string; mime: string; size: number }[] };

/** Stored as the content of a forward sent to a person without words of its own. */
export const forwardedOnly = 'Forwarded message';

export function ForwardIcon() {
  return <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="m14 5 6 6-6 6" /><path d="M20 11H9a5 5 0 0 0-5 5v3" />
  </svg>;
}

/** A forward's attachments, split into the files its copy carries and the sender's own documents. */
export function forwardedFiles(forwarded: Forwarded | null | undefined, attachments: Attachment[] = []) {
  const carried = new Set(forwarded?.files?.map(file => file.id));
  return { carried: attachments.filter(file => carried.has(file.id)), own: attachments.filter(file => !carried.has(file.id)) };
}

/** The forwarded message inside the message that carries it: who wrote it, where and when, and its files. */
export function ForwardedBlock({ forwarded, files = [] }: { forwarded: Forwarded; files?: Attachment[] }) {
  const when = new Date(forwarded.at * 1000).toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  return <section className="chat-forwarded" aria-label={`Forwarded message from ${forwarded.name}`}>
    <header><ForwardIcon /><span>Forwarded from <strong>{forwarded.name}</strong></span>{forwarded.kind === 'agent' && <span className="ai-tag">AI</span>}<span>· {[forwarded.place, when].filter(Boolean).join(' · ')}</span></header>
    {forwarded.text && <div className="message-markdown" dangerouslySetInnerHTML={{ __html: messageHtml(forwarded.text) }} />}
    {files.length > 0 && <MessageFiles files={files} />}
  </section>;
}
