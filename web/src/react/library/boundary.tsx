import { Component, type ReactNode } from 'react';
import { Button } from '../ui';

/** A missing deployment chunk must leave navigation and active uploads usable. */
export class LibraryBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? (
      <section className="library-empty" role="alert">
        <h2>Library could not open</h2>
        <p>Reload this tab to try again.</p>
        <Button onClick={() => window.location.reload()}>Reload Library</Button>
      </section>
    ) : (
      this.props.children
    );
  }
}
