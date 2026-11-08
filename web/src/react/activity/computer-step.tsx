import type { Step } from './model';
import '../agents/computer/computer.css';

/** One thing the agent did on its computer (runtime/agent-computers.md): a command with
 * its exit code and the start of its output, or a file it listed, read or saved. */
export function ComputerStep({ step }: { step: Step }) {
  const done = step.computer;
  const command = done?.kind === 'command';
  const seconds = done && done.milliseconds >= 1000 ? ` · ${(done.milliseconds / 1000).toFixed(1)} s` : '';
  const [tone, note] = step.state === 'pending' ? ['waiting', 'Asked the person first']
    : step.state === 'failed' && done?.exit_code == null ? ['problem', 'Couldn’t run: the computer wasn’t available']
    : step.state === 'failed' ? ['problem', `Finished with exit code ${done?.exit_code}${seconds}`]
    : ['done', command ? `Finished${seconds}` : 'On its computer'];
  return <li className={`agent-computer-step ${tone}`}>
    <strong>{command ? <>Ran <code>{done.detail}</code></> : step.title}</strong>
    <small>{note}</small>
    {command && done.output && <details><summary>Output{done.output_bytes > done.output.length ? ' (start)' : ''}</summary><pre>{done.output}</pre></details>}
  </li>;
}
