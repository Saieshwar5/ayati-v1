import type { Agent } from '../model';
export function JobSummary({ profile }: { profile: Agent }) {
  return <section className="sample-job" aria-label="Saved job used for sample"><h3>{profile.name} · Job version {profile.revision}</h3>
    <dl className="agent-facts"><dt>Responsibility</dt><dd>{profile.role}</dd>
      <dt>Successful outcome</dt><dd>{profile.settings.successful_outcome || 'Not specified'}</dd>
      <dt>Ask a person when</dt><dd>{profile.settings.escalation || 'Not specified'}</dd>
      <dt>Routine</dt><dd>{profile.settings.routine || 'Not specified'}</dd>
      <dt>Agent-wide notes</dt><dd>{profile.settings.memory ? 'Enabled' : 'Disabled'}</dd></dl>
    <p className="ui-help">Job guidance does not grant tool permissions. This sample can draft an answer; it cannot execute connected actions.</p>
  </section>;
}
