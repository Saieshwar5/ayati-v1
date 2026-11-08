import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode, type WheelEvent } from 'react';
import { Button, Dialog } from '../../ui';
import { errorMessage } from '../../requests';
import { base } from '../model';
import { sendLiveInput, type LiveInput } from './model';
import './computer.css';

type Frame = { src: string; width: number; height: number };
type Live = { frame?: Frame; url?: string; control: boolean; state: 'connecting' | 'live' | 'ended' | 'failed'; problem?: string };

/** The agent's browser, live (designs/agent-computers 3 and 12). View only, unless the
 * agent handed it over to this person: then their clicks, scrolling and typing go to
 * the browser. The agent sees nothing while they are in control. */
export function LiveView({ agent, name, close, actions }: { agent: string; name: string; close: () => void; actions?: ReactNode }) {
  const [live, setLive] = useState<Live>({ control: false, state: 'connecting' });
  const [typing, setTyping] = useState('');
  const [problem, setProblem] = useState('');
  const picture = useRef<HTMLImageElement>(null);
  useEffect(() => {
    const stream = new EventSource(`/api/identity${base}/${encodeURIComponent(agent)}/computer/live`);
    stream.onmessage = event => {
      const line = JSON.parse(event.data);
      setLive(old => line.watching ? { ...old, control: Boolean(line.watching.control), state: 'live' }
        : line.frame ? { ...old, state: 'live', frame: { src: `data:image/jpeg;base64,${line.frame}`, width: line.width, height: line.height } }
        : line.url ? { ...old, url: line.url }
        : line.problem ? { ...old, problem: line.problem } : old);
    };
    stream.addEventListener('ended', () => { setLive(old => ({ ...old, state: 'ended' })); stream.close(); });
    stream.onerror = () => { setLive(old => old.state === 'connecting' ? { ...old, state: 'failed' } : old); stream.close(); };
    return () => stream.close();
  }, [agent]);
  // Inputs go one at a time, in order; a failure is shown and the rest still go.
  const queue = useRef(Promise.resolve());
  const send = (input: LiveInput) => {
    if (!live.control) return;
    queue.current = queue.current.then(() => sendLiveInput(agent, input)).then(() => setProblem(''), error => setProblem(errorMessage(error)));
  };
  /** Where on the page a point on the picture is, in the page's own pixels. */
  const at = (event: MouseEvent | WheelEvent) => {
    const box = picture.current!.getBoundingClientRect();
    const frame = live.frame!;
    return { x: Math.round((event.clientX - box.left) * frame.width / box.width), y: Math.round((event.clientY - box.top) * frame.height / box.height) };
  };
  const keys: Record<string, string> = { Enter: 'Enter', Tab: 'Tab', Escape: 'Escape', Backspace: 'Backspace', ArrowDown: 'ArrowDown', ArrowUp: 'ArrowUp', PageDown: 'PageDown', PageUp: 'PageUp' };
  const typeText = () => { if (typing) { send({ text: typing }); setTyping(''); } };
  return <Dialog title={live.control ? `You’re using ${name}’s browser` : `${name}’s browser`} close={close}
    footer={<>{actions}<Button onClick={close}>{live.control ? 'Close the view' : 'Close'}</Button></>}>
    <div className="computer-live">
      <p className="computer-live-address" aria-label="Address">{live.url ?? 'about:blank'}</p>
      {live.state === 'connecting' && <p role="status">Connecting to the browser…</p>}
      {live.state === 'failed' && <p role="alert">The browser can’t be shown now. It may be asleep, or you may not be able to watch it.</p>}
      {live.state === 'ended' && <p role="status">The live picture ended. The computer may have gone to sleep.</p>}
      {live.frame && <img ref={picture} className={`computer-live-picture${live.control ? ' controlled' : ''}`} src={live.frame.src}
        alt={`${name}’s browser${live.url ? `, showing ${live.url}` : ''}`} draggable={false}
        onMouseDown={event => { event.preventDefault(); send({ mouse: { type: 'mousePressed', ...at(event) } }); }}
        onMouseUp={event => send({ mouse: { type: 'mouseReleased', ...at(event) } })}
        onWheel={event => send({ wheel: { ...at(event), delta_y: Math.round(event.deltaY) } })} />}
      {live.control ? <div className="computer-live-keys">
        <label className="ui-field">Type into the page<input value={typing} maxLength={2000} autoComplete="off" spellCheck={false}
          onChange={event => setTyping(event.target.value)}
          onKeyDown={(event: KeyboardEvent) => { if (event.key === 'Enter') { event.preventDefault(); typeText(); send({ key: 'Enter' }); } }} /></label>
        <Button disabled={!typing} onClick={typeText}>Type</Button>
        {Object.keys(keys).map(key => <Button key={key} onClick={() => send({ key })}>{key.replace('Arrow', '')}</Button>)}
        <p className="ui-help">Click the page to choose a field, then type here. Everyone who uses {name} shares its logins, so sign in only to accounts meant for this work. {name} sees nothing while you are in control.</p>
      </div> : live.state === 'live' && <p className="ui-help">View only. {name} is working; you’ll be asked if it needs you.</p>}
      {(problem || live.problem) && <p role="alert">{problem || live.problem}</p>}
    </div>
  </Dialog>;
}
