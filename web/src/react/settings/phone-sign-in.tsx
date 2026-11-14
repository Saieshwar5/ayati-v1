import { useEffect, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { countdown, createSignInCode, lastUsed, loadDevices, platformName, signedInOn, signOutDevice, type Device, type SignInCode } from './phone-sign-in-model';

/** The code on screen: `deadline` uses this browser's clock (from `expires_in`,
 * so a wrong clock still counts down 10 minutes); `issued` is the server's. */
type Shown = SignInCode & { deadline: number; issued: number };

/** Profile › Your phone (runtime/device-sessions.md): a one-time QR code signs
 * the Cuckoo app in as you, and every signed-in phone can be signed out here. */
export function PhoneSignIn() {
  const devices = useResource(loadDevices);
  const mutation = useMutation();
  const [code, setCode] = useState<Shown>();
  const [now, setNow] = useState(() => Date.now());
  const [notice, setNotice] = useState('');
  const { refresh, setData } = devices;
  // A code is used once; any phone signed in since it was made used it.
  const used = code && devices.data?.devices.find(device => device.created >= code.issued);
  const expired = Boolean(code && now >= code.deadline);
  const waiting = Boolean(code && !used && !expired);
  // While the code waits for a scan, count down and look for the new phone.
  useEffect(() => {
    if (!waiting) return;
    let ticks = 0;
    const timer = setInterval(() => {
      setNow(Date.now());
      if (++ticks % 4 === 0) void refresh().catch(() => {});
    }, 1000);
    return () => clearInterval(timer);
  }, [waiting, refresh]);
  const start = () => mutation.run(async () => {
    const next = await createSignInCode();
    const at = Date.now();
    setNow(at);
    setNotice('');
    setCode({ ...next, deadline: at + next.expires_in * 1000, issued: next.expires - next.expires_in });
  });
  const signedOut = (device: Device) => {
    setData(current => current && { devices: current.devices.filter(item => item.id !== device.id) });
    setNotice(`Signed out ${device.name}.`);
    // A used code must not show again once its phone is gone.
    if (used) setCode(undefined);
  };
  const list = devices.data?.devices;
  return <section className="settings-phone" aria-labelledby="settings-phone-title">
    <h3 id="settings-phone-title">Your phone</h3>
    <p>Open the Cuckoo app on your phone and scan a code from here. You’ll be signed in as yourself, no password needed.</p>
    {code && !used ? <div className="settings-phone-code">
      {expired
        ? <p className="settings-phone-expired">This code expired. Make a new one to sign in.</p>
        : <img className="settings-phone-qr" src={code.qr} alt="Sign-in code for your phone" width={180} height={180} />}
      <div className="settings-phone-details">
        <strong>Works once, for {Math.round(code.expires_in / 60)} minutes</strong>
        {!expired && <p role="timer">Expires in {countdown((code.deadline - now) / 1000)}. Don’t share it: whoever scans it signs in as you.</p>}
        <span className="settings-phone-actions">
          <Button disabled={mutation.busy} onClick={() => void start()}>New code</Button>
          <Button onClick={() => setCode(undefined)}>Hide</Button>
        </span>
      </div>
    </div> : <Button primary disabled={mutation.busy} onClick={() => void start()}>Sign in on your phone</Button>}
    <p className="ui-status" role="status">{mutation.busy ? 'Making a code…' : mutation.status || (used ? `Signed in on ${used.name}.` : notice)}</p>
    <h4 id="settings-devices-title">Signed-in devices</h4>
    {devices.error && <p role="alert">{devices.error}</p>}
    {!list && !devices.error && <p role="status">Loading devices…</p>}
    {list?.length === 0 && <p>No phones are signed in.</p>}
    {list && list.length > 0 && <ul className="settings-devices" aria-labelledby="settings-devices-title">
      {list.map(device => <DeviceRow key={device.id} device={device} now={now} signedOut={signedOut} />)}
    </ul>}
  </section>;
}

function DeviceRow({ device, now, signedOut }: { device: Device; now: number; signedOut: (device: Device) => void }) {
  const mutation = useMutation();
  const signOut = () => mutation.run(async () => {
    await signOutDevice(device.id);
    signedOut(device);
  });
  return <li className="settings-device">
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><rect x="7" y="3" width="10" height="18" rx="2" /><path d="M11 18h2" /></svg>
    <span className="settings-device-text">
      <strong>{device.name}</strong>
      <small>{platformName(device.platform)} · Signed in {signedInOn(device)} · Last used {lastUsed(device.last_used, now)}</small>
      {mutation.status && <small role="alert" className="settings-device-error">{mutation.status}</small>}
    </span>
    <Button disabled={mutation.busy} aria-label={`${mutation.busy ? 'Signing out' : 'Sign out'} ${device.name}`} onClick={() => void signOut()}>{mutation.busy ? 'Signing out…' : 'Sign out'}</Button>
  </li>;
}
