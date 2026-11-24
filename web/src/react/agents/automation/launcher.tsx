import { Component, lazy, Suspense, type ReactNode } from 'react';
import type { Agent } from '../model';
import { Button } from '../../ui';
const Pane = lazy(() => import('./pane').then(module => ({ default: module.AutomationPane })));
class AutomationBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <><p role="alert">Automations couldn’t load. Saved triggers and tasks are safe on the server.</p><Button onClick={() => location.reload()}>Reload application</Button></> : this.props.children;
  }
}
export function AutomationLauncher({ agent, openChat, start, changed }: { agent: Agent; openChat: () => void; start?: boolean; changed?: () => void }) {
  return <AutomationBoundary key={agent.id}><Suspense fallback={<p role="status">Opening automations…</p>}><Pane agent={agent} openChat={openChat} start={start} changed={changed} /></Suspense></AutomationBoundary>;
}
