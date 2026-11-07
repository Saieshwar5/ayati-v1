/** Line icons for the Add skills and Upload a skill panels (designs/chat-and-account 7–8). */
const paths = {
  book: 'M4 5.5C4 4.7 4.7 4 5.5 4H11v16H5.5c-.8 0-1.5-.7-1.5-1.5ZM13 4h5.5c.8 0 1.5.7 1.5 1.5v13c0 .8-.7 1.5-1.5 1.5H13ZM7 8h1.5M7 11h1.5M15.5 8H17',
  upload: 'M12 15V4m-4.5 4.5L12 4l4.5 4.5M4 15v3.5c0 .8.7 1.5 1.5 1.5h13c.8 0 1.5-.7 1.5-1.5V15',
  folder: 'M3 6.5C3 5.7 3.7 5 4.5 5H9l2 2.5h8.5c.8 0 1.5.7 1.5 1.5v8.5c0 .8-.7 1.5-1.5 1.5h-15c-.8 0-1.5-.7-1.5-1.5Z',
  file: 'M6 3h8l5 5v12c0 .6-.4 1-1 1H6c-.6 0-1-.4-1-1V4c0-.6.4-1 1-1ZM14 3v5h5',
  chevron: 'm6 9 6 6 6-6',
  check: 'm5 12.5 4.5 4.5L19 7.5',
  warning: 'M12 4 2.5 20h19ZM12 10v4.5M12 17.5v.5',
  search: 'M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13ZM15.5 15.5 20 20',
} as const;

export function SkillIcon({ name }: { name: keyof typeof paths }) {
  return <svg className="skill-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6"
    strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[name]} /></svg>;
}
