import { createContext, useContext, useEffect, useRef, useState } from 'react';
import { subscribeChanges } from './live';
import { chatRequest } from './model';

/** A thread: your assistant talking to one agent or person for you about one
 * matter, in your chat with them, until you stop it (schema 128). */
export type AssistantThread = {
  id: string; kind: 'agent' | 'user'; counterpart: string; name: string | null;
  title: string; opening: string; chat: string | null;
  state: 'suggested' | 'running' | 'stopped'; detail: string; sent: number;
  origin: 'assistant' | 'owner'; updated_ms: number;
  /** A chat handed over from a message (schema 136): what you asked, and the message it started from. */
  instruction?: string; anchor?: string | null; created_ms?: number;
  /** Your assistant is writing its next message. */
  working: boolean;
  /** Where your assistant keeps its notes on this thread, in your chat with it. */
  notes?: { chat: string; thread: string } | null;
};
/** The private record on your own message that a thread sent (schema 128). */
export type Assisted = { agent: string; thread: string; title: string; by: 'assistant' | 'owner' };

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

/** Hand a direct chat over from one of the other side's messages; the same ID starts once. */
export const takeOver = (id: string, chat: string, message: string, instruction: string) =>
  chatRequest<{ chat: string }>('/assistant/take-over', { id, chat, message, instruction });
/** The thread to show for a chat: the running one, else the latest stopped one. */
export function threadIn(threads: AssistantThread[], chat: string) {
  const here = threads.filter(thread => thread.chat === chat && thread.state !== 'suggested');
  return here.find(thread => thread.state === 'running') ?? here[0];
}
