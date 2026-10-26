import { Component, lazy, Suspense, type ReactNode } from 'react';
import type { Agent } from '../model';
import { Button } from '../../ui';
const Pane = lazy(() => import('./pane').then(module => ({ default: module.AutomationPane })));
class AutomationBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <><p role="alert">Automation could not load. Saved routines remain on the server.</p><Button onClick={() => location.reload()}>Reload application</Button></> : this.props.children;
  }
}
export function AutomationLauncher({ agent }: { agent: Agent }) {
  return <AutomationBoundary key={agent.id}><Suspense fallback={<p role="status">Opening automation…</p>}><Pane agent={agent} /></Suspense></AutomationBoundary>;
}
