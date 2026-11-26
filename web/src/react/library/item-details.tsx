import { useState } from 'react';
import { Button } from '../ui';
import { DescriptionControl } from './description';
import { KeepingControl } from './keeping';
import { ReviewControl } from './upkeep';
import { ItemFields } from './watching';
import { dateLabel, folderUrl, levelName, useForName, visibilityName, type Item } from './model';

const tabs = ['About', 'Sharing', 'History', 'Activity'] as const;
type Tab = (typeof tabs)[number];

/** Everything about an item that is not its content, in one panel beside it: what it
 * is, who can use it, its versions, and what is happening to it now. */
export function ItemDetails({
  item,
  refresh,
  share,
  history,
}: {
  item: Item;
  refresh: () => void;
  share: () => void;
  history: () => void;
}) {
  const [tab, setTab] = useState<Tab>('About');
  const working = item.working ?? [];
  return (
    <aside className="library-item-details" aria-label={`Details of ${item.title}`}>
      <div role="tablist" aria-label="Details">
        {tabs.map((name) => (
          <button
            key={name}
            type="button"
            role="tab"
            aria-selected={tab === name}
            onClick={() => setTab(name)}
          >
            {name}
          </button>
        ))}
      </div>
      <div role="tabpanel" aria-label={tab}>
        {tab === 'About' && (
          <>
            <DescriptionControl key={`about:${item.id}`} item={item} changed={refresh} />
            <ReviewControl key={`review:${item.revision}`} item={item} />
            <KeepingControl key={`keeping:${item.revision}`} item={item} />
            <ItemFields item={item} />
          </>
        )}
        {tab === 'Sharing' && (
          <>
            <dl>
              <dt>People</dt>
              <dd>{visibilityName(item.visibility)}</dd>
              <dt>Sharing follows</dt>
              <dd>
                {item.folder ? (
                  <a href={folderUrl(item.source ?? item.folder)}>its folder</a>
                ) : (
                  'this item'
                )}
              </dd>
              <dt>Agents</dt>
              <dd>
                {item.agents.length === 0
                  ? 'None'
                  : item.agents
                      .map((grant) => `${levelName(grant.level)} · ${useForName(grant.use_for)}`)
                      .join('; ')}
              </dd>
            </dl>
            {item.manageable && !item.folder && <Button onClick={share}>Share</Button>}
          </>
        )}
        {tab === 'History' && (
          <>
            <p className="library-muted">
              Revision {item.revision}, last changed {dateLabel(item.updated)}. Every save is a
              version you can restore.
            </p>
            {item.editable ? (
              <Button onClick={history}>Version history</Button>
            ) : (
              <p className="library-muted">People who can edit this see its versions.</p>
            )}
          </>
        )}
        {tab === 'Activity' && (
          <p className="library-muted">
            {working.length > 0
              ? `${working.join(', ')} ${working.length === 1 ? 'is' : 'are'} working on this now.`
              : 'No agent has this open right now.'}{' '}
            Versions written by an agent are marked with its name in History.
          </p>
        )}
      </div>
    </aside>
  );
}
