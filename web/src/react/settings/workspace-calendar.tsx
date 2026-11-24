import { useId, useState } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { SaveBar } from './save-bar';

/** The workspace calendar (designs/agent-tasks): which days are working days, the
 * holidays the workspace keeps itself, and what a task run due on a day off does.
 * Tasks marked "only on working days" follow it. Admins change it. */
export type HolidayRule = 'next' | 'before' | 'skip';
export type WorkspaceCalendar = { timezone: string; working_days: number[]; holiday_rule: HolidayRule;
  holidays: { day: string; name: string }[]; editable?: boolean };

const path = () => `/workspaces/${workspaceId}/settings/calendar`;
export const loadCalendarSettings = () => identityRequest<WorkspaceCalendar>(path());
export const saveCalendarSettings = (input: Omit<WorkspaceCalendar, 'timezone' | 'editable'>) =>
  identityRequest<WorkspaceCalendar>(path(), input);

const WEEK: [number, string][] = [[1, 'Mon'], [2, 'Tue'], [3, 'Wed'], [4, 'Thu'], [5, 'Fri'], [6, 'Sat'], [7, 'Sun']];
const RULES: [HolidayRule, string][] = [['next', 'Runs on the next working day'], ['before', 'Runs on the working day before'], ['skip', 'Is skipped']];

export function WorkspaceCalendarSettings() {
  const resource = useResource(loadCalendarSettings);
  if (resource.error) return <p role="alert">{resource.error}</p>;
  if (!resource.data) return <p role="status">Loading the calendar…</p>;
  // A server without the workspace calendar (before schema 144) answers something else.
  if (!Array.isArray(resource.data.working_days) || !Array.isArray(resource.data.holidays)) return null;
  return <CalendarForm calendar={resource.data} saved={resource.setData} />;
}

function CalendarForm({ calendar, saved }: { calendar: WorkspaceCalendar; saved: (next: WorkspaceCalendar) => void }) {
  const form = useId();
  const [days, setDays] = useState(calendar.working_days);
  const [rule, setRule] = useState<HolidayRule>(calendar.holiday_rule);
  const [holidays, setHolidays] = useState(calendar.holidays);
  const [day, setDay] = useState('');
  const [name, setName] = useState('');
  const mutation = useMutation();
  const editable = calendar.editable !== false;
  const changed = days.join() !== calendar.working_days.join() || rule !== calendar.holiday_rule
    || JSON.stringify(holidays) !== JSON.stringify(calendar.holidays);
  const toggle = (value: number) => setDays(current => current.includes(value)
    ? current.filter(d => d !== value) : [...current, value].sort((a, b) => a - b));
  const add = () => {
    if (!day || !name.trim()) return;
    setHolidays(current => [...current.filter(h => h.day !== day), { day, name: name.trim() }].sort((a, b) => a.day.localeCompare(b.day)));
    setDay(''); setName('');
  };
  const save = () => void mutation.run(async () => {
    const next = await saveCalendarSettings({ working_days: days, holiday_rule: rule, holidays });
    saved({ ...next, editable: calendar.editable });
    setDays(next.working_days); setRule(next.holiday_rule); setHolidays(next.holidays);
    mutation.setStatus('Saved.');
  });
  return <form id={form} className="ui-form settings-calendar" aria-label="Working days and holidays"
    onSubmit={event => { event.preventDefault(); if (changed && days.length) save(); }}>
    <h3>Working days and holidays</h3>
    <p className="ui-help">Tasks set to run only on working days follow these. Task times use the workspace time zone above.</p>
    <fieldset className="ui-fieldset" disabled={mutation.busy || !editable}>
      <legend>Working days</legend>
      <div className="settings-days">{WEEK.map(([value, label]) =>
        <label key={value} className="settings-day"><input type="checkbox" checked={days.includes(value)} onChange={() => toggle(value)} />{label}</label>)}</div>
      {!days.length && <p className="ui-help" role="alert">Choose at least one working day.</p>}
      <label className="ui-field">A task due on a day off
        <select value={rule} onChange={event => setRule(event.target.value as HolidayRule)}>
          {RULES.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
      <div className="settings-holidays">
        <h4>Holidays</h4>
        {holidays.length ? <ul>{holidays.map(holiday => <li key={holiday.day}>
          <span><strong>{holiday.name}</strong> <small>{holiday.day}</small></span>
          {editable && <Button onClick={() => setHolidays(current => current.filter(h => h.day !== holiday.day))} aria-label={`Remove ${holiday.name}`}>Remove</Button>}
        </li>)}</ul> : <p className="ui-help">No holidays yet. Cuckoo does not guess public holidays; add the days your workspace takes off.</p>}
        {editable && <div className="settings-holiday-add">
          <label className="ui-field">Date<input type="date" value={day} onChange={event => setDay(event.target.value)} /></label>
          <label className="ui-field">Name<input maxLength={80} value={name} onChange={event => setName(event.target.value)} placeholder="Diwali" /></label>
          <Button onClick={add} disabled={!day || !name.trim()}>Add holiday</Button>
        </div>}
      </div>
    </fieldset>
    {changed && editable && <SaveBar form={form} busy={mutation.busy} disabled={!days.length}
      discard={() => { setDays(calendar.working_days); setRule(calendar.holiday_rule); setHolidays(calendar.holidays); mutation.setStatus(''); }} />}
    <p className="ui-status" role="status">{mutation.status}</p>
  </form>;
}
