import type { Agent } from '../model';
export function JobSummary({ profile }: { profile: Agent }) {
  return <section className="sample-job" aria-label="Saved job used for sample"><h3>{profile.name} · Job version {profile.revision}</h3>
    <dl className="agent-facts"><dt>Job title</dt><dd>{profile.role || 'Not specified'}</dd>
      <dt>Instructions</dt><dd className="agent-instructions-text">{profile.settings.instructions || 'Not specified'}</dd>
      <dt>Agent-wide notes</dt><dd>{profile.settings.memory ? 'Enabled' : 'Disabled'}</dd></dl>
    <p className="ui-help">Job guidance does not grant tool permissions. This sample can draft an answer; it cannot execute connected actions.</p>
  </section>;
}
