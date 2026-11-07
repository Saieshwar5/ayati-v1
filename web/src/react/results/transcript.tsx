import { useId, useState } from 'react';
import type { Attachment } from '../chat/model';
import { Button } from '../ui';

/** Under a voice note: its transcript, folded away, or why there isn't one.
 * Shows nothing where the deployment doesn't transcribe audio. */
export function AudioTranscript({ file }: { file: Attachment }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const status = file.transcript_status;
  if (status === 'pending') return <small className="chat-transcript" role="status">Transcribing…</small>;
  if (status === 'failed' || (status === 'ready' && typeof file.transcript !== 'string')) return <small className="chat-transcript">Transcript unavailable</small>;
  if (status !== 'ready') return null;
  return <div className="chat-transcript">
    <Button className="chat-transcript-toggle" aria-expanded={open} aria-controls={id} onClick={() => setOpen(value => !value)}>Transcript</Button>
    <p id={id} hidden={!open}>{file.transcript?.trim() || 'No speech was recognised.'}</p>
  </div>;
}
