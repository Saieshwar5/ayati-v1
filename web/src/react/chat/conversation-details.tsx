import { useId, useRef } from 'react';
import { currentWorkspace } from '../../workspace-context';
import { Button } from '../ui';
import { ChannelLeave } from './channel-leave';
import { ArchiveReview } from './channel-lifecycle';
import { ChannelPeople } from './channel-people';
import { ChannelSettings } from './channel-settings';
import { ChannelBots } from './channel-bots';
import { LockIcon } from './conversation-face';
import { conversationName, directPartner, type Conversation, type Directory } from './model';
import { NotifyMenu } from './notify-controls';
import type { Level } from './notifications';
import type { Pin, Preferences } from './organization-model';
import { PinAction } from './pin-control';
import { ProfileDetails, ProfilePanel, type ProfileTarget } from './profile-card';
import { SidePanel } from './side-panel';
import './conversation-details.css';

/** What the side panel shows: the open conversation's Details or someone's profile. */
export type PanelView = { kind: 'details'; review?: string } | { kind: 'profile'; target: ProfileTarget; fromDetails?: boolean };
type Pins = { preferences?: Preferences; busy: boolean; toggle: (pin: Pin) => void };
type Shared = {
  directory: Directory; chats: Conversation[]; pins: Pins; changed: () => Promise<void>;
  message: (target: ProfileTarget & { name: string }) => void;
  /** After leaving a channel you can no longer read: back to the conversation list. */
  left: () => void;
};

/** The one side panel in Chat. Details follows the open conversation; a profile stays until closed. */
export function ChatPanel({ view, setView, chat, level, levelChanged, ...shared }: Shared & {
  view: PanelView; setView: (view: PanelView | undefined) => void; chat?: Conversation;
  level?: Level; levelChanged: (level: Level) => void;
}) {
  const close = () => setView(undefined);
  const { directory, chats, message } = shared;
  if (view.kind === 'profile') {
    const back = view.fromDetails && chat ? { label: `Back to ${conversationName(chat, directory)} details`, go: () => setView({ kind: 'details' }) } : undefined;
    return <ProfilePanel key={`${view.target.kind}:${view.target.id}`} target={view.target} directory={directory} chats={chats} close={close} message={message} back={back} />;
  }
  if (!chat) return null;
  const name = conversationName(chat, directory);
  const direct = chat.kind === 'direct';
  return <SidePanel key="details" label={direct ? `${name} profile` : `${name} details`} close={close}
    title={direct ? name : <><span className="chat-side-panel-mark" aria-hidden="true">{chat.visibility === 'public' ? '#' : <LockIcon />}</span>{name}</>}>
    <ConversationDetails key={chat.id} chat={chat} level={level} levelChanged={levelChanged} {...shared}
      review={view.review === chat.id} setReview={open => setView({ kind: 'details', review: open ? chat.id : undefined })}
      viewProfile={target => setView({ kind: 'profile', target, fromDetails: true })} />
  </SidePanel>;
}

function ConversationDetails({ chat, directory, chats, level, levelChanged, pins, review, setReview, changed, message, left, viewProfile }: Shared & {
  chat: Conversation; level?: Level; levelChanged: (level: Level) => void;
  review: boolean; setReview: (open: boolean) => void; viewProfile: (target: ProfileTarget) => void;
}) {
  const archive = useRef<HTMLButtonElement>(null);
  const about = useId();
  const actions = <div className="chat-details-actions">
    <NotifyMenu chat={chat} level={level} changed={levelChanged} labelled />
    <PinAction pin={{ kind: 'chat', id: chat.id }} preferences={pins.preferences} busy={pins.busy} toggle={pins.toggle} />
    {chat.kind !== 'direct' && chat.can_archive && <Button ref={archive} className="chat-details-action" aria-expanded={review} onClick={() => setReview(!review)}>
      <ArchiveIcon />{chat.archived ? 'Restore' : 'Archive'}
    </Button>}
  </div>;
  if (chat.kind === 'direct') {
    const partner = directPartner(chat, directory);
    return <>
      {actions}
      {partner ? <ProfileDetails target={{ kind: partner.kind, id: partner.who.id }} directory={directory} chats={chats} />
        : <p className="chat-help">This person or agent is no longer in the workspace.</p>}
      <p className="chat-help chat-details-note">Direct messages have no member list, settings or archive.</p>
    </>;
  }
  // Focus goes back to Archive/Restore when the review closes, whichever way.
  const endReview = () => { setReview(false); archive.current?.focus(); };
  const workspace = currentWorkspace()?.name || 'the workspace';
  return <>
    {actions}
    {review && chat.can_archive && <ArchiveReview chat={chat} changed={changed} close={endReview} />}
    <section className="chat-details-section" aria-labelledby={about}>
      <h3 id={about}>About</h3>
      <p className={chat.description ? 'chat-details-description' : 'chat-help'}>{chat.description || 'No description yet.'}</p>
      <ul className="chat-details-facts">
        <li>{chat.visibility === 'public' ? <span aria-hidden="true">#</span> : <LockIcon size={13} />}
          {chat.visibility === 'public' ? `Public · everyone in ${workspace} can find it` : 'Private · only people added can find it'}</li>
        {chat.announcements && <li><span aria-hidden="true">!</span>Only admins can post</li>}
        {chat.creator && <li><PersonIcon />{createdBy(chat, directory)}</li>}
        {chat.archived && <li><ArchiveIcon />Archived · history and files stay readable</li>}
      </ul>
    </section>
    <ChannelPeople chat={chat} directory={directory} changed={changed} message={message} viewProfile={viewProfile} />
    {/* The creator and admins manage the channel; only admins change who can find it. */}
    {chat.can_archive && <ChannelSettings chat={chat} admin={directory.role === 'Admin'} changed={changed} />}
    {chat.can_archive && <ChannelBots key={chat.id} chat={chat} />}
    <ChannelLeave chat={chat} changed={changed} left={left} />
  </>;
}

/** "Created by Priya Shah · 12 March 2026"; older channels have no recorded time. */
function createdBy(chat: Conversation, directory: Directory): string {
  const who = directory.users.find(user => user.id === chat.creator)?.name ?? 'a former member';
  const when = chat.created_at ? new Date(chat.created_at * 1000).toLocaleDateString([], { day: 'numeric', month: 'long', year: 'numeric' }) : '';
  return `Created by ${who}${when ? ` · ${when}` : ''}`;
}

function PersonIcon() {
  return <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="12" cy="8" r="3.5" /><path d="M5 20a7 7 0 0 1 14 0" /></svg>;
}

function ArchiveIcon() {
  return <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><rect x="3" y="4" width="18" height="5" rx="1" /><path d="M5 9v10h14V9M10 13h4" /></svg>;
}
