import { Component, lazy, Suspense, type ReactNode } from 'react';
import { Button, Dialog } from '../../ui';
import type { Agent } from '../model';
const RehearsalDialog = lazy(() => import('./dialog').then(module => ({ default: module.RehearsalDialog })));
class SampleBoundary extends Component<{ close: () => void; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <Dialog title="Sample could not open" close={this.props.close}>
      <p role="alert">The sample view could not be loaded. Your saved agent is unchanged.</p><Button onClick={() => location.reload()}>Reload application</Button>
    </Dialog> : this.props.children;
  }
}
export function RehearsalLauncher(props: { agent: Agent; close: () => void }) {
  return <SampleBoundary close={props.close}><Suspense fallback={<Dialog title="Try a sample task" close={props.close}><p role="status">Opening saved samples…</p></Dialog>}><RehearsalDialog {...props} /></Suspense></SampleBoundary>;
}
