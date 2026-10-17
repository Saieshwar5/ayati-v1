import { post, selectedAgent } from './types';
import { loadBusinessClock } from './home-clock';
import { localMinute, matchingInstants } from './business-time';

export interface ScheduleInput { id: string; content: string; run_at_ms: number; interval_seconds: number | null }

export function installScheduleForm(container: HTMLElement, saved: () => Promise<void>): (available: boolean) => void {
  container.innerHTML = `<form id="schedule-form">
    <h3>Create a schedule</h3>
    <label>Task instructions<textarea name="content" required maxlength="16000" placeholder="What should this agent do?"></textarea></label>
    <label>First run<input name="start" type="datetime-local" required></label>
    <p id="schedule-timezone"></p>
    <label>Repeat<select name="repeat"><option value="once">Run once</option><option value="interval">At a fixed interval</option></select></label>
    <fieldset id="schedule-interval" hidden><legend>Fixed interval</legend>
      <label>Every<input name="amount" type="number" min="1" step="1" value="1"></label>
      <label>Unit<select name="unit"><option value="3600">hours</option><option value="60">minutes</option><option value="86400">24-hour periods</option></select></label>
      <p>Intervals are elapsed time, not calendar recurrence. Local clock time may shift with daylight saving.</p>
    </fieldset>
    <p>Scheduled work can use your configured model and incur API charges, even when this page is closed. This computer and Ayati must stay on. Tool approvals still apply.</p>
    <p id="schedule-save-status" role="status"></p><button type="submit" class="files-button">Create schedule</button>
  </form>`;
  const form = container.querySelector<HTMLFormElement>('form')!;
  const input = <T extends HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>(name: string): T => form.elements.namedItem(name) as T;
  const content = input<HTMLTextAreaElement>('content');
  const start = input<HTMLInputElement>('start');
  const repeat = input<HTMLSelectElement>('repeat');
  const amount = input<HTMLInputElement>('amount');
  const unit = input<HTMLSelectElement>('unit');
  const submit = form.querySelector<HTMLButtonElement>('[type=submit]')!;
  const status = form.querySelector('#schedule-save-status')!;
  const key = `ayati.schedule.pending.${selectedAgent}`;
  let pending: ScheduleInput | undefined;
  let busy = false;
  let available = false, clockReady = false;
  let timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  function localDate(ms: number): string { return localMinute(ms, timezone); }
  function defaults(): void { form.reset(); start.value = localDate(Date.now() + 3600000); }
  function controls(): void {
    for (const field of [content, start, repeat, amount, unit]) field.disabled = busy || !!pending || !available || !clockReady;
    amount.disabled ||= repeat.value === 'once';
    unit.disabled ||= repeat.value === 'once';
    submit.disabled = busy || !available || !clockReady;
    submit.textContent = pending ? 'Retry same save' : 'Create schedule';
    form.querySelector<HTMLElement>('#schedule-interval')!.hidden = repeat.value !== 'interval';
    amount.required = repeat.value === 'interval';
  }
  defaults();
  try {
    const raw = sessionStorage.getItem(key);
    if (raw) {
      const parsed = JSON.parse(raw) as ScheduleInput;
      if (typeof parsed.id === 'string' && typeof parsed.content === 'string' && Number.isSafeInteger(parsed.run_at_ms)
        && (parsed.interval_seconds === null || Number.isSafeInteger(parsed.interval_seconds))) {
        pending = parsed; content.value = parsed.content; start.value = localDate(parsed.run_at_ms);
        if (parsed.interval_seconds !== null) { repeat.value = 'interval'; amount.value = String(parsed.interval_seconds / 60); unit.value = '60'; }
        status.textContent = 'A save was not acknowledged. Retry the same save safely; it cannot create a second schedule. Inspect the list before retrying.';
      }
    }
  } catch { /* Storage is optional; pending state remains in memory for this page. */ }
  void loadBusinessClock().then(clock => {
    timezone = clock.timezone; clockReady = true;
    form.querySelector('#schedule-timezone')!.textContent = `Times use ${clock.location}: ${timezone}. Repeated daylight-saving hours must be avoided; choose an unambiguous time.`;
    if (pending) start.value = localDate(pending.run_at_ms); else defaults();
    controls();
  }).catch(() => { status.textContent = 'Could not load the business timezone. Reopen this page before creating a schedule.'; });
  repeat.onchange = controls;
  form.onsubmit = async event => {
    event.preventDefault(); if (busy || !available || !clockReady) return;
    if (!pending) {
      const matches = matchingInstants(start.value, timezone);
      if (matches.length !== 1) { status.textContent = 'This local time is missing or repeated by a daylight-saving change. Choose another time.'; return; }
      const runAt = matches[0];
      const interval = repeat.value === 'once' ? null : Number(amount.value) * Number(unit.value);
      if (!Number.isFinite(runAt) || localDate(runAt) !== start.value || runAt <= Date.now()) {
        status.textContent = 'Choose a valid future local time. Some times do not exist when daylight saving changes.'; return;
      }
      if (interval !== null && (!Number.isSafeInteger(interval) || interval < 1 || interval > 31536000)) {
        status.textContent = 'Choose a fixed interval of at most 365 24-hour periods.'; return;
      }
      if (!content.value.trim() || new TextEncoder().encode(content.value).length > 16000) {
        status.textContent = 'Task instructions must contain 1–16000 UTF-8 bytes.'; return;
      }
      pending = { id: crypto.randomUUID(), content: content.value, run_at_ms: runAt, interval_seconds: interval };
      try { sessionStorage.setItem(key, JSON.stringify(pending)); } catch { /* Optional storage. */ }
    }
    busy = true; controls(); status.textContent = 'Saving…';
    let acknowledged = false;
    try {
      await post('/schedules', pending);
      acknowledged = true; pending = undefined;
      try { sessionStorage.removeItem(key); } catch { /* Optional storage. */ }
      defaults(); status.textContent = 'Schedule saved.';
      await saved();
    } catch (error) {
      status.textContent = acknowledged ? 'Schedule saved, but the list could not refresh. Use Refresh to inspect it.'
        : `${error instanceof Error ? error.message : 'Save result unavailable.'} Retry the same save; no automatic retry was sent.`;
    } finally { busy = false; controls(); }
  };
  controls();
  return value => { available = value; controls(); };
}
