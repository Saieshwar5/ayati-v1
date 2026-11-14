import { identityRequest } from '../../workspace-context';

/** A one-time code a phone scans to sign in (runtime/device-sessions.md). `qr` is
 * an SVG data URL of `link`; times are Unix seconds. */
export type SignInCode = { code: string; expires: number; expires_in: number; link: string; qr: string };
/** A signed-in phone. `current` marks the phone asking; the web is never listed. */
export type Device = { id: string; name: string; platform: 'ios' | 'android'; created: number; last_used: number; expires: number; current: boolean };

export const loadDevices = () => identityRequest<{ devices: Device[] }>('/devices');
export const createSignInCode = () => identityRequest<SignInCode>('/devices/link-code', {});
export const signOutDevice = (id: string) => identityRequest<{ revoked: boolean }>(`/devices/${encodeURIComponent(id)}/revoke`, {});

export const platformName = (platform: Device['platform']) => platform === 'ios' ? 'iPhone' : 'Android';

/** `9:05` for 545 seconds. */
export function countdown(seconds: number): string {
  const left = Math.max(0, Math.ceil(seconds));
  return `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`;
}

const day = (seconds: number) => new Date(seconds * 1000).toLocaleDateString([], { day: 'numeric', month: 'short', year: 'numeric' });

/** "just now", "5 min ago", "3 h ago", then the date. */
export function lastUsed(seconds: number, nowMs: number): string {
  const minutes = Math.floor((nowMs / 1000 - seconds) / 60);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 24 * 60) return `${Math.floor(minutes / 60)} h ago`;
  return day(seconds);
}

export const signedInOn = (device: Device) => day(device.created);
