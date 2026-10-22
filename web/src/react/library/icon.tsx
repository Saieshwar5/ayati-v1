export function LibraryIcon({ name }: { name: string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      {name === 'all' && (
        <>
          <path d="M4 4h5v16H4zM12 4h3v16h-3zM16 5l3-1 4 15-3 1z" />
        </>
      )}
      {name === 'private' && (
        <>
          <rect x="5" y="10" width="14" height="11" rx="2" />
          <path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v3" />
        </>
      )}
      {name === 'shared' && (
        <>
          <circle cx="9" cy="8" r="3" />
          <path d="M3 21v-3a6 6 0 0 1 12 0v3m1-16a3 3 0 0 1 0 6m2 3a5 5 0 0 1 3 4v3" />
        </>
      )}
      {name === 'starred' && (
        <path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9Z" />
      )}
      {name === 'trash' && (
        <>
          <path d="M4 7h16M9 3h6l1 4M6 7l1 14h10l1-14M10 11v6m4-6v6" />
        </>
      )}
      {name === 'page' && (
        <>
          <path d="M5 3h9l5 5v13H5ZM14 3v6h5M8 12h8m-8 4h8" />
        </>
      )}
      {name === 'file' && (
        <>
          <path d="M5 3h9l5 5v13H5ZM14 3v6h5" />
          <path d="m8 17 3-4 2 2 2-3 2 5Z" />
        </>
      )}
    </svg>
  );
}
