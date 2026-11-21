import { createContext, useContext, useEffect, useRef, useState } from 'react';
import { subscribeChanges } from './live';
import { chatRequest } from './model';

/** A thread: what your assistant does for you in your chat with one agent or
 * person (schemas 128 and 141). While it is `running`, `mode` says whether it
 * sends replies as you (`auto`) or writes each one for you to send (`ask`). */
export type AssistantThread = {
  id: string; kind: 'agent' | 'user'; counterpart: string; name: string | null;
  title: string; opening: string; chat: string | null;
  state: 'suggested' | 'running' | 'stopped'; mode?: 'ask' | 'auto'; detail: string; sent: number;
  /** On `ask`: the reply your assistant wrote that waits for you, once it is `complete`. */
  draft?: WaitingReply | null;
  origin: 'assistant' | 'owner'; updated_ms: number;
  /** What you asked it to do there, and the message its replies for you start from. */
  instruction?: string; anchor?: string | null; created_ms?: number;
  /** Your assistant is writing its next message. */
  working: boolean;
  /** Where your assistant keeps its notes on this thread, in your chat with it. */
  notes?: { chat: string; thread: string } | null;
};
export type WaitingReply = { reply: string; state: string; text: string };
/** What your assistant does in a chat: nothing, write replies for you to send, or reply for you. */
export type Position = 'off' | 'ask' | 'auto';
/** The private record on your own message that your assistant wrote: sent in a
 * thread (schema 128), or a draft you sent yourself (`draft`, schema 129). */
export type Assisted = { agent: string; thread?: string; draft?: string; title: string; by: 'assistant' | 'owner' };

export type ThreadsState = { threads: AssistantThread[]; reload: () => void; assistant?: string; room?: string };
export const AssistantThreads = createContext<ThreadsState>({ threads: [], reload() {} });
export const useThreads = () => useContext(AssistantThreads);

/** Your threads, read once and again when a chat they touch changes. */
export function useAssistantThreads(assistant: string | undefined, room: string | undefined): ThreadsState {
  const [threads, setThreads] = useState<AssistantThread[]>([]);
  const reload = useRef(() => {});
  const watched = useRef(new Set<string>());
  watched.current = new Set([room, ...threads.map(thread => thread.chat)].filter((id): id is string => Boolean(id)));
  useEffect(() => {
    if (!assistant) { setThreads([]); return; }
    const abort = new AbortController(); let timer: number | undefined;
    const load = async () => {
      try {
        const data = await chatRequest<{ threads: AssistantThread[] }>('/assistant/threads', undefined, abort.signal);
        if (!abort.signal.aborted) setThreads(data.threads ?? []);
      } catch { /* The last list stays; the next change reads again. */ }
    };
    const soon = () => { if (timer === undefined) timer = window.setTimeout(() => { timer = undefined; void load(); }, 300); };
    reload.current = soon; void load();
    const unsubscribe = subscribeChanges(change => {
      if (change.kind !== 'message' || (change.chat && watched.current.has(change.chat))) soon();
    });
    return () => { abort.abort(); unsubscribe(); if (timer !== undefined) window.clearTimeout(timer); };
  }, [assistant]);
  return { threads, reload: () => reload.current(), assistant, room };
}

export const setThreadState = (id: string, state: 'running' | 'stopped' | 'deleted') =>
  chatRequest(`/assistant/threads/${encodeURIComponent(id)}/state`, { state });

/** Set what your assistant does in a direct chat (schema 141). `id` names the thread if this turns it on there for the first time. */
export const setReplies = (id: string, chat: string, mode: Position, instruction?: string) =>
  chatRequest<{ thread: string | null }>('/assistant/replies', instruction === undefined ? { id, chat, mode } : { id, chat, mode, instruction });
/** You do not want the reply that waits for you in a thread. */
export const discardReply = (thread: string) => chatRequest(`/assistant/threads/${encodeURIComponent(thread)}/draft/discard`, {});
/** The thread to show for a chat: the running one, else the latest stopped one. */
export function threadIn(threads: AssistantThread[], chat: string) {
  const here = threads.filter(thread => thread.chat === chat && thread.state !== 'suggested');
  return here.find(thread => thread.state === 'running') ?? here[0];
}
/** A thread from before schema 141 has no mode: it replies for you. */
export const positionOf = (thread?: AssistantThread): Position => thread?.state === 'running' ? thread.mode ?? 'auto' : 'off';
