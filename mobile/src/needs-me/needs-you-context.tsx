/**
 * What is waiting for this person, read once for the tab bar's badge, Home's
 * summary and the Needs me tab, so the three never poll separately.
 */
import { createContext, useContext, type ReactNode } from 'react';
import { useAttention } from './use-attention';

type NeedsYou = ReturnType<typeof useAttention>;
const NeedsYouContext = createContext<NeedsYou | undefined>(undefined);

export function NeedsYouProvider({ children }: { children: ReactNode }) {
  const attention = useAttention('needs_you');
  return <NeedsYouContext.Provider value={attention}>{children}</NeedsYouContext.Provider>;
}

export function useNeedsYou(): NeedsYou {
  const value = useContext(NeedsYouContext);
  if (!value) throw new Error('useNeedsYou must be used inside NeedsYouProvider');
  return value;
}
