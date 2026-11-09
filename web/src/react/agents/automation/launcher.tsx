import { Component, lazy, Suspense, type ReactNode } from 'react';
import type { Agent } from '../model';
import { Button } from '../../ui';
const Pane = lazy(() => import('./pane').then(module => ({ default: module.AutomationPane })));
class AutomationBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <><p role="alert">Automations couldn’t load. Saved schedules and triggers are safe on the server.</p><Button onClick={() => location.reload()}>Reload application</Button></> : this.props.children;
  }
}
export function AutomationLauncher({ agent, openChat }: { agent: Agent; openChat: () => void }) {
  return <AutomationBoundary key={agent.id}><Suspense fallback={<p role="status">Opening automations…</p>}><Pane agent={agent} openChat={openChat} /></Suspense></AutomationBoundary>;
}
