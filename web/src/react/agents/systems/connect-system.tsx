import { useRef, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation } from '../../requests';
import { connectSystem, initialChoices, previewSystem, type ListedTool, type ToolChoice } from './model';
import { ReviewTools, chosen } from './review-tools';

type Exact = { id: string; name: string; url: string; key?: string; tools: ToolChoice[] };

/** Connect a system (C1 of the connected systems plan): 1 address and key,
 * 2 review each tool. Nothing is saved until the review is confirmed, and a
 * repeated click or retry sends the same request. The key is sent once and
 * never shown again. */
export function ConnectSystem({ agent, agentName, close, saved }: { agent: string; agentName: string; close: () => void; saved: () => Promise<void> }) {
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [key, setKey] = useState('');
  const [tools, setTools] = useState<ListedTool[]>();
  const [choices, setChoices] = useState<ReturnType<typeof initialChoices>>({});
  const [done, setDone] = useState(false);
  const attempt = useRef<Exact | undefined>(undefined);
  const m = useMutation();
  const look = () => void m.run(async () => {
    if (!name.trim()) throw new Error('Give the system a name, such as StudyHub.');
    if (!url.trim().startsWith('https://')) throw new Error('Use the system\'s HTTPS address, such as https://studyhub.example/mcp.');
    const listed = (await previewSystem(agent, url.trim(), key.trim())).tools;
    attempt.current = undefined;
    setTools(listed); setChoices(initialChoices(listed));
  });
  const save = () => void m.run(async () => {
    const picked = chosen(choices);
    if (!picked.length) throw new Error('Switch on at least one tool.');
    const exact = attempt.current ?? { id: crypto.randomUUID(), name: name.trim(), url: url.trim(), ...(key.trim() ? { key: key.trim() } : {}), tools: picked };
    attempt.current = exact;
    await connectSystem(agent, exact);
    setDone(true);
    setKey('');
    await saved();
  });
  const supported = tools?.filter(t => t.supported).length ?? 0;
  const footer = done ? <Button onClick={close}>Done</Button> : tools ? <>
    <Button disabled={m.busy || Boolean(attempt.current)} onClick={() => setTools(undefined)}>Back</Button>
    <Button primary disabled={m.busy || !supported} onClick={save}>{attempt.current ? 'Retry same save' : `Connect ${name.trim() || 'system'}`}</Button>
  </> : <Button primary disabled={m.busy} onClick={look}>Find its tools</Button>;
  return <Dialog drawer title="Connect a system" close={close} busy={m.busy} status={m.status} footer={footer}>
    {done ? <p role="status">{name.trim()} is connected. {agentName} can use its tools under the rules you chose; changes ask first where you said so.</p>
      : <>
        <ol className="system-steps" aria-label="Steps"><li className={tools ? '' : 'current'}>Address</li><li className={tools ? 'current' : ''}>Review tools</li></ol>
        {!tools ? <div className="ui-form">
          <p className="ui-help">Your system needs an MCP server: one web address that lists its tools. {agentName} uses them only under the rules you set here, and every change can ask first.</p>
          <label className="ui-field">Name<input value={name} maxLength={100} placeholder="StudyHub" onChange={event => setName(event.target.value)} /></label>
          <label className="ui-field">MCP address<input value={url} inputMode="url" placeholder="https://studyhub.example/mcp" onChange={event => setUrl(event.target.value)} /></label>
          <label className="ui-field">Key (if the server needs one)<input type="password" autoComplete="off" value={key} onChange={event => setKey(event.target.value)} /></label>
          <p className="ui-help">Cuckoo sends the key as a bearer token, stores it sealed and never shows it again. Public addresses only.</p>
        </div> : <>
          {!supported && <p role="alert">This system offers no tools Cuckoo can use yet. Each one below says why.</p>}
          <ReviewTools tools={tools} choices={choices} change={next => { attempt.current = undefined; setChoices(next); }} />
        </>}
      </>}
  </Dialog>;
}
