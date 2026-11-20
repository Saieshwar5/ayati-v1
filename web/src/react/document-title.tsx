import { useEffect } from 'react';
import type { WorkspacePage } from './workspace-shell';
import { chatAttention, useUnreadCounts } from './chat/unread-counts';
import { useNeedsYouCount } from './attention/needs-you-count';

/** The tab's title: the page, and how much is waiting for the person, so a
 * background tab still says so: unread chat messages as the rail counts them,
 * and what needs them in Activity. "(3) Home · Cuckoo". */
export function documentTitle(page: WorkspacePage, waiting: number, more = false): string {
  const name = page[0].toUpperCase() + page.slice(1);
  if (waiting <= 0) return `${name} · Cuckoo`;
  return `(${waiting > 99 ? '99+' : `${waiting}${more ? '+' : ''}`}) ${name} · Cuckoo`;
}

/** Sets the title; it renders nothing, so a new count redraws nothing else. */
export function DocumentTitle({ page }: { page: WorkspacePage }) {
  const chat = chatAttention(useUnreadCounts()).count;
  const needs = useNeedsYouCount();
  const title = documentTitle(page, chat + needs.count, needs.more);
  useEffect(() => { document.title = title; }, [title]);
  return null;
}
