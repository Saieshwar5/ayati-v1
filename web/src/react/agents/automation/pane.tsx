import { useEffect, useRef, useState } from 'react';
import type { Agent } from '../model';
import { Button } from '../../ui';
import { workspaceUrl } from '../../../workspace-context';
import { subscribeChanges } from '../../chat/live';
import './schedules.css';
import { EventTriggers } from './events';
import { NewAutomation } from './new-automation';

/** Where people see what an agent does at a time: the Calendar (tasks, schema 147). */
const calendarUrl = () => workspaceUrl('/?view=activity&tab=planned');

/** `start` opens "New automation" at once (the agent page's ＋); `changed` is told when a trigger is saved or changed. */
export function AutomationPane({ agent, openChat, start = false, changed }: { agent: Agent; openChat?: () => void; start?: boolean; changed?: () => void }) {
  return agent.editable ? <Automations key={agent.id} agent={agent} openChat={openChat} start={start} changed={changed} /> : <MemberAutomations agent={agent} openChat={openChat} />;
}
/** People who talk to the agent but don't look after it: they get work at a time by
 * asking in Chat, and it becomes a task that runs as them (designs/agent-tasks). */
function MemberAutomations({ agent, openChat }: { agent: Agent; openChat?: () => void }) {
  return <section className="automation-member" aria-label={`${agent.name} automations`}>
    <h3>Automations</h3>
    <p>Only {agent.name}’s owner or an admin can add triggers here.</p>
    <p>For work at a time, ask {agent.name} in Chat, in your own words. It becomes a task that runs as you.</p>
    <p className="automation-example">“Every Friday at 5 pm, send me this week’s delayed orders.”</p>
    <div className="automation-tools">
      {openChat && <Button primary onClick={openChat}>Ask {agent.name} in Chat</Button>}
      <a className="ui-button" href={calendarUrl()}>See what runs next on the Calendar</a>
    </div>
    <p className="ui-help">Pause, skip or end a task from its page in Activity.</p>
  </section>;
}
/** Work at a time is a task made in Chat; triggers are set up here. */
function Automations({ agent, openChat, start, changed }: { agent: Agent; openChat?: () => void; start: boolean; changed?: () => void }) {
  const [open, setOpen] = useState<'choose' | 'event' | undefined>(start ? 'choose' : undefined);
  const opener = useRef<HTMLElement | null>(null), newButton = useRef<HTMLButtonElement | null>(null);
  const focusTimer = useRef<number | undefined>(undefined);
  const choose = () => { opener.current = document.activeElement as HTMLElement | null; setOpen('choose'); };
  const close = () => {
    setOpen(undefined);
    focusTimer.current = window.setTimeout(() => {
      if (!opener.current?.isConnected) newButton.current?.focus();
    }, 0);
  };
  useEffect(() => {
    const access = subscribeChanges(change => {
      if (change.kind === 'access' || change.kind === 'reset') setOpen(undefined);
    });
    return () => { window.clearTimeout(focusTimer.current); access(); };
  }, []);
  const ask = openChat ? () => { setOpen(undefined); openChat(); } : undefined;
  return <section className="shared-schedules" aria-label={`${agent.name} automations`}>
    <header className="agent-section-heading"><div><h3>Automations</h3></div>
      <Button primary ref={newButton} className="automation-new" onClick={choose}>New automation</Button>
    </header>
    <section className="automation-group" aria-labelledby={`${agent.id}-at-a-time`}>
      <header className="agent-section-heading"><div><h4 id={`${agent.id}-at-a-time`}>At a time</h4>
        <p>Ask {agent.name} in Chat, in your own words. It becomes a task: it runs on its own and shows on the Calendar, where it can be paused or ended.</p></div></header>
      <div className="automation-tools">
        {ask && <Button onClick={ask}>Ask {agent.name} in Chat</Button>}
        <a className="ui-button" href={calendarUrl()}>Open the Calendar</a>
      </div>
    </section>
    <EventTriggers agent={agent.id} start={open === 'event'} finished={() => { close(); changed?.(); }} />
    {open === 'choose' && <NewAutomation agent={agent.name} ask={ask} close={close} choose={() => setOpen('event')} />}
  </section>;
}
