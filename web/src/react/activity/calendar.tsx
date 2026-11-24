import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { agentSectionUrl } from '../agents/workspace-tabs';
import { taskUrl, type Scope } from './model';
import { useLive } from './use-live';
import { UpcomingView } from './upcoming';
import { days, itemState, loadCalendar, range, timeIn, weekStart, zoneLabel, type CalendarItem } from './calendar-model';

/** Activity › Calendar (designs/agent-tasks), which replaces Planned: a week of what
 * agents did and will do on their own, in the workspace's time zone. Tasks are
 * made and changed by asking the agent in chat; here people look and open a task
 * to pause or stop it. The List keeps Planned's list: triggers, booked task runs and
 * follow-ups (schedules became tasks in schema 147). */
export function CalendarView({ scope, search, list: startOnList = false }: { scope: Scope; search: string; list?: boolean }) {
  const [list, setList] = useState(startOnList);
  const [offset, setOffset] = useState(0);
  return <section className="activity-calendar" aria-label="Calendar">
    <div className="activity-calendar-bar">
      {!list && <>
        <Button aria-label="Previous week" onClick={() => setOffset(o => o - 1)}>‹</Button>
        <Button onClick={() => setOffset(0)} disabled={offset === 0}>This week</Button>
        <Button aria-label="Next week" onClick={() => setOffset(o => o + 1)}>›</Button>
      </>}
      <span className="activity-calendar-spacer" />
      <div className="activity-kinds" role="group" aria-label="Show as">
        <Button aria-pressed={!list} onClick={() => setList(false)}>Week</Button>
        <Button aria-pressed={list} onClick={() => setList(true)}>List</Button>
      </div>
    </div>
    {list ? <UpcomingView scope={scope} search={search} /> : <Week scope={scope} search={search} offset={offset} />}
  </section>;
}

function Week({ scope, search, offset }: { scope: Scope; search: string; offset: number }) {
  const start = weekStart(Date.now(), offset);
  const { from, to } = range(start);
  const { data, error } = useLive(useCallback(() => loadCalendar(scope, from, to), [scope, from, to]), 30000);
  if (error) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading the calendar…</p>;
  const query = search.trim().toLowerCase();
  const matches = (item: { title: string; agent_name: string }) => !query || `${item.title} ${item.agent_name}`.toLowerCase().includes(query);
  const week = days(data, start);
  const today = week.findIndex(day => day.key === new Intl.DateTimeFormat('en-CA', { timeZone: data.timezone }).format(new Date(data.now)));
  const untimed = data.untimed.filter(matches);
  const empty = !data.items.length && !data.untimed.length;
  return <>
    <h2 className="activity-calendar-title">{week[0].date} – {week[6].date} <small>{zoneLabel(data.timezone)}</small></h2>
    {empty && offset === 0 && <p className="activity-empty">Nothing runs on its own yet. Ask an agent in Chat, for example “every morning at 9, tell me which orders are late”.</p>}
    <div className="activity-week-box">
      <div className="activity-week" role="list" aria-label="Days">{week.map((day, index) => {
        const shown = day.items.filter(matches);
        return <section key={day.key} role="listitem" className={`activity-day${index === today ? ' today' : ''}`} aria-label={`${day.name} ${day.date}`}>
          <h3>{day.name} <b>{day.date}</b>{index === today && <span> · today</span>}</h3>
          {shown.map(item => <Entry key={`${item.kind}.${item.task}.${item.at}`} item={item} zone={data.timezone} />)}
          {!shown.length && <p className="activity-day-empty">—</p>}
        </section>;
      })}</div>
    </div>
    {untimed.length > 0 && <section className="activity-untimed" aria-label="No fixed time">
      <h3>No fixed time</h3>
      <ul>{untimed.map(item => <li key={item.kind === 'trigger' ? `t.${item.trigger}` : `p.${item.task}`}>
        {item.kind === 'trigger'
          ? <a href={agentSectionUrl(item.agent, 'Automations')}><strong>{item.title}</strong></a>
          : <a href={taskUrl(item.task)}><strong>{item.title}</strong></a>}
        <small>{item.kind === 'trigger' ? 'When something happens' : 'Paused'} · {item.agent_name}{item.detail ? ` · ${item.detail}` : ''}</small>
      </li>)}</ul>
    </section>}
  </>;
}

function Entry({ item, zone }: { item: CalendarItem; zone: string }) {
  const state = itemState(item);
  return <a className={`activity-entry ${state.tone}`} href={taskUrl(item.task)}>
    <time>{timeIn(item.at, zone)}{state.tone !== 'planned' ? ` · ${state.label}` : ''}</time>
    <span>{item.title}</span>
    <small>{item.agent_name}</small>
  </a>;
}
