import { useEffect, useState } from 'react';

/** Whether a CSS media query matches now, following changes. False where the
 * browser cannot say, so callers fall back to their narrow layout. */
export function useMediaQuery(query: string): boolean {
  const read = () => typeof window.matchMedia === 'function' && window.matchMedia(query).matches;
  const [matches, setMatches] = useState(read);
  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return;
    const list = window.matchMedia(query);
    const update = () => setMatches(list.matches);
    update();
    list.addEventListener('change', update);
    return () => list.removeEventListener('change', update);
  }, [query]);
  return matches;
}
