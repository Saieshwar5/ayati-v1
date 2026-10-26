export function CornerIcon({ collapsed = false }: { collapsed?: boolean }) {
  return <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d={collapsed ? 'M14 4h6v6m0-6-6 6M10 20H4v-6m0 6 6-6' : 'M20 10h-6V4m0 6 6-6M4 14h6v6m0-6-6 6'} />
  </svg>;
}
