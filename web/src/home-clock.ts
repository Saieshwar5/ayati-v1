import { canManageResources } from './workspace-context';
import { api, post } from './types';
import { element } from './view';
import type { BusinessClock } from './business-time';
import './home-clock.css';
let clock: BusinessClock = { location: 'Device time', timezone: Intl.DateTimeFormat().resolvedOptions().timeZone, hour12: true };
let loaded: Promise<BusinessClock> | undefined;
export function businessClock(): BusinessClock { return clock; }
export function loadBusinessClock(): Promise<BusinessClock> {
  return loaded ??= api<{ preferences: BusinessClock | null }>('/home/preferences').then(data => {
    if (data.preferences) clock = data.preferences;
    window.dispatchEvent(new Event('business-clock-changed'));
    return clock;
  }).catch(error => { loaded = undefined; throw error; });
}
export function businessDate(value: number | string): string {
  return new Date(value).toLocaleString(undefined, { timeZone: clock.timezone, hour12: clock.hour12 });
}
export function installHomeClock(time: HTMLElement): void {
  const button = element('button', 'text-button home-location', 'Choose location');
  time.after(button); button.disabled = !canManageResources();
  const draw = (): void => {
    time.textContent = new Date().toLocaleString(undefined, { weekday: 'long', day: 'numeric', month: 'long', hour: 'numeric', minute: '2-digit', timeZone: clock.timezone, hour12: clock.hour12 });
    button.textContent = `${clock.location} ⌄`;
  };
  draw(); window.setInterval(draw, 30000);
  void loadBusinessClock().then(draw).catch(() => { button.textContent = 'Set business location'; });
  button.onclick = () => {
    const dialog = document.createElement('dialog'); dialog.className = 'clock-dialog'; dialog.setAttribute('aria-label', 'Business location and time');
    dialog.innerHTML = `<form><div class="panel-header"><h2>Your business clock</h2><button type="button" class="icon-button" aria-label="Close location">×</button></div>
      <label>City or location<input name="location" list="business-locations" required maxlength="100" placeholder="Hyderabad"></label>
      <label>Timezone<input name="timezone" list="business-zones" required placeholder="Asia/Kolkata"></label><datalist id="business-zones"></datalist><datalist id="business-locations"></datalist>
      <label>Clock format<select name="format"><option value="12">12-hour</option><option value="24">24-hour</option></select></label>
      <p data-preview></p><p class="clock-note">Home and new schedule times use this location. Existing schedules keep their saved timing. Travelling with your device will not change them.</p>
      <p role="status"></p><button class="files-button" type="submit">Save location</button></form>`;
    const form = dialog.querySelector('form')!, locationInput = form.elements.namedItem('location') as HTMLInputElement;
    const timezone = form.elements.namedItem('timezone') as HTMLInputElement, format = form.elements.namedItem('format') as HTMLSelectElement;
    locationInput.value = clock.location === 'Device time' ? '' : clock.location; timezone.value = clock.timezone; format.value = clock.hour12 ? '12' : '24';
    const zones = (Intl as typeof Intl & { supportedValuesOf(key: string): string[] }).supportedValuesOf('timeZone');
    for (const zone of new Set([clock.timezone, 'UTC', ...zones])) { const option = document.createElement('option'); option.value = zone; dialog.querySelector('datalist')!.append(option); }
    const cities = new Map(zones.map(zone => [zone.split('/').at(-1)!.replaceAll('_', ' '), zone]));
    for (const city of ['Hyderabad', 'Mumbai', 'Bengaluru', 'New Delhi', 'Kolkata']) cities.set(city, 'Asia/Kolkata');
    for (const [city, zone] of [...cities].sort(([a], [b]) => a.localeCompare(b))) { const option = document.createElement('option'); option.value = city; option.textContent = zone; dialog.querySelector('#business-locations')!.append(option); }
    locationInput.addEventListener('input', () => { const zone = cities.get(locationInput.value); if (zone) timezone.value = zone; });
    const preview = (): void => { try { dialog.querySelector('[data-preview]')!.textContent = new Date().toLocaleString(undefined, { timeZone: timezone.value, hour12: format.value === '12' }); } catch { dialog.querySelector('[data-preview]')!.textContent = 'Choose a valid timezone.'; } };
    form.oninput = preview; preview();
    form.onsubmit = async event => {
      event.preventDefault(); const submit = form.querySelector<HTMLButtonElement>('[type=submit]')!; submit.disabled = true;
      try {
        new Intl.DateTimeFormat(undefined, { timeZone: timezone.value });
        const next = { location: locationInput.value.trim(), timezone: timezone.value, hour12: format.value === '12' };
        await post('/home/preferences', next); clock = next; loaded = Promise.resolve(clock); draw(); dialog.close(); window.dispatchEvent(new Event('business-clock-changed'));
      } catch (error) { form.querySelector('[role=status]')!.textContent = `${(error as Error).message} Location was not confirmed saved.`; }
      finally { submit.disabled = false; }
    };
    dialog.querySelector<HTMLButtonElement>('[type=button]')!.onclick = () => dialog.close();
    dialog.onclose = () => dialog.remove(); document.body.append(dialog); dialog.showModal();
  };
}
