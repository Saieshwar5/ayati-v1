import { Component, type ReactNode } from 'react';
import { Button } from '../ui';

/** A failed optional inbox chunk must leave the other Home panels usable. */
export class AttentionBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    if (this.state.failed) return <section className="home-notifications" aria-label="Notifications">
      <h2>Notifications</h2><p role="alert">The attention view could not be opened. Saved work remains in Chat.</p>
      <Button onClick={() => location.reload()}>Reload application</Button>
    </section>;
    return this.props.children;
  }
}
