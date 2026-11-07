import { useId, useState } from 'react';
import type { Attachment } from '../chat/model';

const lines = <svg className="chat-transcript-icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 4h11M2.5 8h11M2.5 12h7" /></svg>;

/** Under a voice note: "≡ Transcript ⌄" folds its text away, or says why there isn't one.
 * Shows nothing where the deployment doesn't transcribe audio. */
export function AudioTranscript({ file }: { file: Attachment }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const status = file.transcript_status;
  if (status === 'pending') return <small className="chat-transcript is-waiting" role="status">{lines}Transcribing…</small>;
  if (status === 'failed' || (status === 'ready' && typeof file.transcript !== 'string')) return <small className="chat-transcript is-waiting">{lines}Transcript unavailable</small>;
  if (status !== 'ready') return null;
  return <div className="chat-transcript">
    <button type="button" className="chat-transcript-toggle" aria-expanded={open} aria-controls={id} onClick={() => setOpen(value => !value)}>
      {lines}Transcript<svg className="chat-transcript-chevron" viewBox="0 0 16 16" aria-hidden="true"><path d="m4.5 6.5 3.5 3.5 3.5-3.5" /></svg>
    </button>
    <p id={id} hidden={!open}>{file.transcript?.trim() || 'No speech was recognised.'}</p>
  </div>;
}
