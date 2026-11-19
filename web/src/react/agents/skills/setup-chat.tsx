import { useEffect, useRef, useState, type ReactNode, type RefObject } from 'react';
import { Button } from '../../ui';
import { AgentFace } from '../../faces';
import { useMutation, useResource } from '../../requests';
import { askAssistant, askWriter, loadAssistantHelp, loadWriter, readAssistantAnswer,
  type ChatMessage, type WriterAnswer, type WriterRequest } from './writer-model';
import './setup-chat.css';

type Shown = ChatMessage & { key: string };
type Setup = Omit<WriterRequest, 'messages'>;
/** A turn the member's assistant is still writing. */
type Waiting = { reply: string; person: Shown; setup: Setup; apply: (answer: WriterAnswer) => void };

/** The setup chat's state: messages, the composer and the helper's notes.
 * The helper is the member's own assistant where it can work (schema 139),
 * otherwise the skill writer, otherwise nobody. The assistant is asked in its
 * own room and its drafts are read every two seconds until it has answered;
 * the writer answers at once. Neither saves anything. `earlier` is a
 * conversation kept with a draft, to carry on from. */
export function useSkillWriting(earlier?: ChatMessage[]) {
  const writer = useResource(loadWriter);
  const help = useResource(loadAssistantHelp);
  const assistant = help.data?.available ? help.data.assistant : undefined;
  const [messages, setMessages] = useState<Shown[]>(() => (earlier ?? []).map(message => ({ ...message, key: crypto.randomUUID() })));
  const [notes, setNotes] = useState<string[]>([]);
  const [used, setUsed] = useState<string[]>([]);
  const [composer, setComposer] = useState('');
  const [waiting, setWaiting] = useState<Waiting>();
  const [failure, setFailure] = useState('');
  const sending = useMutation();
  const turn = useRef(waiting); turn.current = waiting;
  const take = ({ person, setup, apply }: Omit<Waiting, 'reply'>, answer: WriterAnswer) => {
    setMessages(old => [...old, person, { key: crypto.randomUUID(), role: 'agent', text: answer.reply }]);
    const agent = setup.name || 'This agent';
    setNotes([...answer.notes, ...answer.missing_apps.map(app => assistant
      ? `${agent} will need ${app}. Connect it on its Connections tab${setup.agent ? '' : ' once it is created'}.`
      : `${app} isn’t one of the apps here, so no skill uses it. An admin can add it.`)]);
    setUsed(answer.used ?? []);
    apply(answer);
  };
  // A turn that ends without an answer gives the member their words back.
  const stop = (message: string) => {
    const text = turn.current?.person.text ?? '';
    setWaiting(undefined); setFailure(message);
    setComposer(old => old || text);
  };
  const reply = waiting?.reply;
  useEffect(() => {
    if (!reply) return;
    const abort = new AbortController(); let tries = 0, reading = false;
    const failed = () => stop('Your assistant couldn’t answer. Try again, or write it yourself.');
    const timer = window.setInterval(async () => {
      if (reading) return;
      // Three minutes is long for a draft: stop reading and say so.
      if (++tries > 90) { failed(); return; }
      reading = true;
      try {
        const result = await readAssistantAnswer(reply, abort.signal);
        const current = turn.current;
        if (abort.signal.aborted || current?.reply !== reply) return;
        if (result.state === 'complete') {
          setWaiting(undefined);
          take(current, { reply: result.reply ?? '', ground_rules: result.ground_rules ?? null, skills: result.skills ?? [],
            remove: result.remove ?? [], reuse: result.reuse ?? [], missing_apps: result.missing_apps ?? [], notes: result.notes ?? [],
            job_title: result.job_title, used: result.used });
        } else if (!['queued', 'streaming'].includes(result.state)) failed();
      } catch { /* The next read tries again. */ }
      finally { reading = false; }
    }, 2000);
    return () => { abort.abort(); window.clearInterval(timer); };
  // The turn is read through a ref, so the effect follows the reply alone.
  }, [reply]);
  /** Sends the composer; on an answer, `apply` gets it. A failed turn keeps
   * the text so it can be sent again. */
  const send = (request: Setup | (() => Promise<Setup>), apply: (answer: WriterAnswer) => void) => {
    const text = composer.trim();
    if (!text || turn.current) return;
    const person: Shown = { key: crypto.randomUUID(), role: 'person', text };
    setFailure('');
    void sending.run(async () => {
      const history = [...messages, person].slice(-40).map(({ role, text }) => ({ role, text }));
      const setup = typeof request === 'function' ? await request() : request;
      if (assistant) {
        const asked = await askAssistant(person.key, { ...setup, messages: history });
        setComposer(''); setWaiting({ reply: asked.reply, person, setup, apply });
        return;
      }
      take({ person, setup, apply }, await askWriter({ ...setup, messages: history }));
      setComposer('');
    });
  };
  return { available: Boolean(assistant) || (writer.data?.available ?? false),
    loading: (!writer.data && !writer.error) || (!help.data && !help.error),
    /** The member's assistant, when it is the one helping. */
    helper: assistant, guide: (assistant ? help.data?.guide : writer.data?.guide) ?? '',
    messages, notes, used, composer, setComposer, pending: waiting?.person,
    busy: sending.busy || Boolean(waiting), status: sending.status || failure, send,
    /** Stops waiting for the assistant; what it writes later is not used. */
    cancel: waiting ? () => stop('') : undefined };
}
export type SkillWriting = ReturnType<typeof useSkillWriting>;

/** The chat panel (designs/agent-built-skills 2 and 5). `name` is who answers:
 * the member's assistant, or the agent being set up when the writer drafts.
 * `greeting` opens an empty chat; `extra` sits above the composer (starters,
 * proposed changes). */
export function SetupChat({ name, title = name, subtitle, greeting, writing, submit, extra, input }: {
  name: string; title?: string; subtitle: string; greeting: string; writing: SkillWriting; submit: () => void; extra?: ReactNode;
  input?: RefObject<HTMLTextAreaElement | null>;
}) {
  const { messages, notes, used, composer, setComposer, busy, status, guide, helper, pending, cancel } = writing;
  return <section className="setup-chat" aria-label={title === name ? `Chat with ${name}` : title}>
    <header className="setup-chat-header">
      {helper && <AgentFace agent={{ id: helper.id, name: helper.name, avatar: helper.avatar ?? undefined }} className="setup-chat-face" />}
      <span><strong>{title}</strong><small>{subtitle}</small></span>
    </header>
    <ol className="setup-chat-messages" aria-live="polite">
      <li className="setup-chat-message agent">{greeting}</li>
      {messages.map(message => <li key={message.key} className={`setup-chat-message ${message.role}`}>{message.text}</li>)}
      {pending && <li className="setup-chat-message person">{pending.text}</li>}
      {busy && <li className="setup-chat-message agent" role="status">{helper ? `${name} is writing…` : 'Writing…'}
        {cancel && <Button className="ui-link" onClick={cancel}>Stop</Button>}</li>}
    </ol>
    {used.length > 0 && <section className="setup-chat-used" aria-label={`What ${name} used`}>
      <h4>What {name} used</h4>
      <ul>{used.map(line => <li key={line}>{line}</li>)}</ul>
      <p>From its memory of you and the Library. Check these, and tell {name} if one is wrong.</p>
    </section>}
    {notes.length > 0 && <ul className="setup-chat-notes" aria-label="Notes">{notes.map(note => <li key={note}>{note}</li>)}</ul>}
    {extra}
    <form className="setup-chat-composer" onSubmit={event => { event.preventDefault(); submit(); }}>
      <label className="setup-chat-label" htmlFor="setup-chat-input">Message {name}</label>
      <textarea ref={input} id="setup-chat-input" rows={3} maxLength={helper ? 2000 : 8000} value={composer} placeholder={`Tell ${name} what to do or change…`}
        onChange={event => setComposer(event.target.value)}
        onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); submit(); } }} />
      <Button primary type="submit" disabled={busy || !composer.trim()}>{busy && !pending ? 'Sending…' : 'Send'}</Button>
    </form>
    {status && <p className="setup-chat-error" role="alert">{status}</p>}
    {guide && <details className="setup-chat-guide"><summary>What {name} is told while setting up</summary><pre>{guide}</pre></details>}
  </section>;
}
