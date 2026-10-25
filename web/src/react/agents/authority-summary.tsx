import type { Controls } from './control-model';

export function AuthoritySummary({ data, error }: {
  data?: Controls; error: string;
}) {
  return <section className="agent-authority-summary"><h3>Working authority</h3>
    {error && <p role="alert">Working authority could not be loaded. {error}</p>}
    {!data && !error && <p role="status">Loading permissions…</p>}
    {data && <>
      {(['allow', 'ask', 'block'] as const).map(mode => {
        const rules = data.rules.filter(rule => rule.mode === mode);
        return rules.length ? <div className="agent-authority-group" key={mode}>
          <strong className={`agent-authority-label ${mode}`}>{mode === 'allow' ? 'Automatic' : mode === 'ask' ? 'Ask first' : 'Never allow'}</strong>
          <p>{rules.slice(0, 3).map(rule => data.connections.find(c => c.id === rule.connection)?.definition.operations.find(op => op.id === rule.operation)?.title ?? rule.operation).join(' · ')}{rules.length > 3 ? ` · ${rules.length - 3} more` : ''}</p>
        </div> : null;
      })}
      {!data.rules.length && <p>No business actions assigned in conversations you can view.</p>}
      <p className="ui-help">Rules apply to their assigned accounts and conversations. Current access is checked again before an action.</p>
    </>}
  </section>;
}
