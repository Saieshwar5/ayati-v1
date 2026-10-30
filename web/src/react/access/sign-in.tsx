import { useState } from 'react';
import { Brand } from '../ui';
import { Notice } from './shell';

const POINTS = [
  ['Agents work beside your team', 'Ask in chat; they reply, act and report back.'],
  ['Nothing happens without approval', 'Sensitive actions wait for the right person.'],
  ['One place for people and agents', 'Chat, Home panels, apps and files together.'],
];

/** Both buttons continue to WorkOS AuthKit; Cuckoo never sees passwords. */
export function SignIn({ notice }: { notice: string }) {
  const [email, setEmail] = useState('');
  return <div className="access-sign-in">
    <section className="access-story" aria-label="About Cuckoo">
      <Brand symbol className="access-story-mark" />
      <h1>Your team and AI agents, working together.</h1>
      <p>A workspace where people and agents share chat, work and decisions.</p>
      <ul>{POINTS.map(([title, text]) => <li key={title}><strong>{title}</strong><span>{text}</span></li>)}</ul>
    </section>
    <section className="access-form-side">
      <div className="access-form">
        <Brand className="access-logo" />
        <h1>Sign in</h1>
        <p className="access-lead">Use the work email your invitation or invite code was sent to.</p>
        {notice && <p className="access-alert" role="alert">{notice}</p>}
        <a className="access-button" href="/auth/login?method=google"><span className="access-google" aria-hidden="true">G</span>Continue with Google</a>
        <div className="access-or"><span>or</span></div>
        <form action="/auth/login" method="get">
          <label className="access-field">Work email
            <input name="email" type="email" autoComplete="email" required value={email} onChange={event => setEmail(event.target.value)} />
          </label>
          <button type="submit" className="access-button primary">Continue with email</button>
        </form>
        <Notice title="Cuckoo is invite-only during early access">
          Companies join with an invite code from the Cuckoo team; their people join by invitation from their workspace.
        </Notice>
        <p className="access-fine">Sign-in by WorkOS · Cuckoo never sees your Google password.</p>
      </div>
    </section>
  </div>;
}
