import { useEffect, useState } from 'react';
import { workspaceSession } from '../../workspace-context';
import { subscribeChanges } from './live';
import { chatRequest, type Conversation, type Directory, type Participant } from './model';

/**
 * Who this person can add to a conversation, read from the chat directory Chat
 * already loads. Your own assistant never counts: it joins no channel (the
 * server refuses), and its chat is already first in the list.
 * Only presentation: the server authorizes every invitation and conversation.
 */
export type StartFacts = {
  admin: boolean;
  /** Everyone in the workspace except you. */
  people: Participant[];
  /** Agents that can join a channel or be messaged: not anyone's assistant. */
  teamAgents: Participant[];
  /** Your own assistant, when you have one. */
  assistant?: Participant;
};

export function startFacts(directory: Directory, me = workspaceSession?.user?.id): StartFacts {
  return {
    admin: directory.role === 'Admin',
    people: directory.users.filter(user => user.id !== me).sort((a, b) => a.name.localeCompare(b.name)),
    teamAgents: directory.agents.filter(teamAgent),
    assistant: directory.agents.find(agent => agent.assistant && agent.yours),
  };
}

/** Agents a channel can include. Older "Only me" agents and assistants stay out. */
export function teamAgent(agent: Participant): boolean {
  return !agent.assistant && agent.audience !== 'only_me';
}

/** Nobody but you could join a new channel. */
export const nobodyForChannel = (facts: StartFacts) => !facts.people.length && !facts.teamAgents.length;

/** Your assistant is the only one you could message. */
export const nobodyToMessage = nobodyForChannel;

/** Nothing in the list but For you and your assistant: the first visit. */
export function onlyAssistantChat(chats: Conversation[], assistantChat?: string): boolean {
  return !chats.some(chat => !chat.archived && chat.id !== assistantChat);
}

export type PendingInvite = { id: string; email: string };
type Invitation = { id: string; email: string; status: string; expires: number };

/**
 * An admin's invitations still waiting to be accepted, so Invite people can show
 * that an invite went out. An invited person joins the directory only on
 * accepting. Members read nothing. A read finishing after Chat closes is ignored;
 * a workspace or account switch reloads the page.
 */
export function usePendingInvites(admin: boolean): PendingInvite[] {
  const [invites, setInvites] = useState<PendingInvite[]>([]);
  useEffect(() => {
    if (!admin) { setInvites([]); return; }
    const abort = new AbortController();
    const read = () => void chatRequest<{ invitations?: Invitation[] }>('/people', undefined, abort.signal)
      .then(people => {
        if (abort.signal.aborted) return;
        const now = Date.now();
        setInvites((people.invitations ?? []).filter(invite => invite.status === 'pending' && invite.expires * 1000 > now)
          .map(({ id, email }) => ({ id, email })));
      })
      .catch(() => { if (!abort.signal.aborted) setInvites([]); });
    read();
    // Someone accepting or an invite being cancelled changes access.
    const unsubscribe = subscribeChanges(change => { if (change.kind === 'access' || change.kind === 'reset') read(); });
    return () => { abort.abort(); unsubscribe(); };
  }, [admin]);
  return invites;
}
