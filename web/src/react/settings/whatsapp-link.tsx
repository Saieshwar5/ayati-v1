import { useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { displayNumber, linkMobile, loadMobile, unlinkMobile, type LinkCode } from '../agents/whatsapp/model';
import '../agents/whatsapp/whatsapp.css';

/** Profile: your WhatsApp number. You link it by sending a short code from your
 * WhatsApp; after that, agents with WhatsApp know your messages are yours.
 * Without a linked number you can't message agents on WhatsApp. */
export function WhatsappLink() {
  const mobile = useResource(loadMobile);
  const mutation = useMutation();
  const [code, setCode] = useState<LinkCode>();
  const data = mobile.data;
  if (!data || (!data.offered && !data.number)) return null;
  const start = () => mutation.run(async () => setCode(await linkMobile()));
  // Linking happens when the code arrives on WhatsApp; this reads the result.
  const check = () => mutation.run(async () => {
    const next = await loadMobile();
    mobile.setData(next);
    if (next.number) setCode(undefined);
    else mutation.setStatus('Not linked yet. Send the code from your WhatsApp, then try again.');
  });
  const unlink = () => mutation.run(async () => {
    mobile.setData({ ...(await unlinkMobile()), offered: data.offered });
    setCode(undefined);
  });
  const linked = data.number;
  return <section className="settings-whatsapp" aria-labelledby="settings-whatsapp-title">
    <h3 id="settings-whatsapp-title">WhatsApp</h3>
    {linked ? <div className="settings-whatsapp-row">
      <p>Linked: <strong>{displayNumber(linked)}</strong>. You can message agents that have WhatsApp from this number.</p>
      <Button disabled={mutation.busy} onClick={() => void unlink()}>Unlink</Button>
    </div> : <>
      <p>Link your WhatsApp number to message agents on WhatsApp. People without a linked number can’t use WhatsApp with Cuckoo.</p>
      {!code && <Button primary disabled={mutation.busy} onClick={() => void start()}>Link WhatsApp</Button>}
      {code && <div className="settings-whatsapp-code" aria-label="Link code">
        <p>Send this code from your WhatsApp to {displayNumber(code.send_to)}. It works for {Math.round(code.expires_in / 60)} minutes.</p>
        <code>{code.code}</code>
        <span className="settings-whatsapp-actions">
          <a className="ui-button primary" href={code.link} target="_blank" rel="noreferrer">Open WhatsApp</a>
          <Button disabled={mutation.busy} onClick={() => void check()}>I’ve sent it</Button>
          <Button disabled={mutation.busy} onClick={() => void start()}>New code</Button>
        </span>
      </div>}
    </>}
    <p className="ui-status" role="status">{mutation.busy ? 'Checking…' : mutation.status}</p>
  </section>;
}
