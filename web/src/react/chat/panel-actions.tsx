import type { ComponentProps, ReactNode } from 'react';
import './panel-actions.css';

/**
 * The row of round actions under a profile or a channel's description: an icon in
 * a soft tile with its word under it. The bell and Pin inside `.panel-actions`
 * take the same shape (panel-actions.css), so one row reads as one set.
 */
export function ActionTile({ icon, label, primary = false, expanded, ...button }: Omit<ComponentProps<'button'>, 'children'> & {
  icon: ReactNode; label: string; primary?: boolean; expanded?: boolean;
}) {
  return <button type="button" {...button} aria-expanded={expanded} className={`panel-action${primary ? ' panel-action-primary' : ''}`}>
    {icon}<span>{label}</span>
  </button>;
}

export function ActionLink({ icon, label, href, title }: { icon: ReactNode; label: string; href: string; title?: string }) {
  return <a className="panel-action" href={href} title={title} aria-label={title}>{icon}<span>{label}</span></a>;
}

const paths = {
  message: <path d="M21 11a8 8 0 0 1-8 8H5l-3 3V11a9 9 0 0 1 19 0Z" />,
  plus: <path d="M12 5v14M5 12h14" />,
  check: <path d="m5 12 5 5 9-10" />,
  settings: <><path d="M4 7h9M17 7h3M4 17h3M11 17h9" /><circle cx="15" cy="7" r="2" /><circle cx="9" cy="17" r="2" /></>,
  edit: <><path d="M4 20h4L19 9l-4-4L4 16z" /><path d="m13 7 4 4" /></>,
  play: <path d="M8 5v14l11-7z" fill="currentColor" stroke="none" />,
  pause: <path d="M9 5v14M15 5v14" />,
  copy: <><rect x="8" y="8" width="12" height="12" rx="2" /><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3" /></>,
  mail: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="m3 7 9 6 9-6" /></>,
  person: <><circle cx="12" cy="8" r="3.5" /><path d="M5 20a7 7 0 0 1 14 0" /></>,
  hash: <path d="M9 3 7 21M17 3l-2 18M4 9h17M3 15h17" />,
  about: <><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7.5v.5" /></>,
  chevron: <path d="m9 6 6 6-6 6" />,
  thread: <path d="M4 5h16v10H9l-5 4z" />,
};

/** The small line icons the panels use; decorative, so hidden from screen readers. */
export function Icon({ name }: { name: keyof typeof paths }) {
  return <svg className={`panel-icon panel-icon-${name}`} viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}
