import { useEffect, useState } from 'react';
import { identityRequest } from '../workspace-context';
import { Button } from './ui';
import { useMutation } from './requests';
type Roster = { workspace: string; employees: { number: number; name: string; team: string }[] };
export function SimulationPage() {
  const [roster, setRoster] = useState<Roster>();
  const [error, setError] = useState('');
  const mutation = useMutation();
  useEffect(() => {
    const abort = new AbortController();
    void identityRequest<Roster>('/simulation', undefined, abort.signal).then(setRoster).catch(e => { if (!abort.signal.aborted) setError(String(e)); });
    return () => abort.abort();
  }, []);
  return <main className="company-page"><h1>Aster Distribution</h1><p>Local company simulation · 20 employees · scripted agents · no LLM calls</p>
    <p>Choose an employee to inspect their workspace. Each person has separate permissions. Previous messages and files remain saved.</p>
    {error && <p role="alert">{error}</p>}{mutation.status && <p role="alert">{mutation.status}</p>}
    <div className="company-rows">{roster?.employees.map(person => <div key={person.number}><span><strong>{person.name}</strong><small>{person.team}</small></span><Button disabled={mutation.busy} onClick={() => void mutation.run(async () => {
      await identityRequest(`/simulation/employees/${person.number}/login`, {});
      location.assign(`/w/${roster.workspace}/?view=chat`);
    })}>Open workspace</Button></div>)}</div>
  </main>;
}
