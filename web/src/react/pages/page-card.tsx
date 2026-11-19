import type { PageSummary } from './page-format';
import './pages.css';

const kinds: Record<string, string> = { chart: 'chart', table: 'table', diagram: 'diagram' };

function details(page: PageSummary) {
  const parts = [];
  if (page.sections) parts.push(`${page.sections} ${page.sections === 1 ? 'section' : 'sections'}`);
  const blocks = page.blocks.map(kind => kinds[kind]).filter(Boolean);
  if (blocks.length) parts.push(blocks.join(', '));
  parts.push(`${page.minutes} min read`);
  return parts.join(' · ');
}

/** The card for a page in an agent's reply. It only opens the page for reading. */
export function PageCard({ page, open }: { page: PageSummary; open?: () => void }) {
  return <button type="button" className="page-card" disabled={!open} onClick={open} aria-label={`Open page: ${page.title}`}>
    <span className="page-card-picture" aria-hidden="true"><i /><i /><i /><span><b /><b /><b /></span></span>
    <span className="page-card-text">
      <small>Page</small>
      <strong>{page.title}</strong>
      <span>{page.complete ? details(page) : 'Writing page…'}</span>
    </span>
    <svg className="page-card-open" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M14 4h6v6m0-6-7 7M10 20H4v-6m0 6 7-7" /></svg>
  </button>;
}
