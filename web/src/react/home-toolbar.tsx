import { Button } from './ui';
import { canManageResources } from '../workspace-context';

/** React owns the toolbar; existing panel controllers own their isolated host. */
export function HomeToolbar({ ready, arrange, add }: {
  ready: boolean; arrange: () => void; add: () => void;
}) {
  return <div className="home-toolbar">
    {canManageResources() && <div className="home-toolbar-actions">
      <Button disabled={!ready} onClick={arrange} aria-label="Customize Home">Customize</Button>
      <Button primary disabled={!ready} onClick={add}>＋ Add panel</Button>
    </div>}
  </div>;
}
