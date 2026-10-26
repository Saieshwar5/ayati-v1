import { createContext, useContext, useState, type ReactNode } from 'react';
import { ApprovalDialog } from './approval-dialog';
const OpenApproval = createContext<((id: string) => void) | null>(null);
export const useApprovalViewer = () => useContext(OpenApproval);
/** The conversation owns an open review, independently of a refreshed message row. */
export function ApprovalViewer({ children }: { children: ReactNode }) {
  const [opened, open] = useState<string>();
  return (
    <OpenApproval value={open}>
      {children}
      {opened && <ApprovalDialog key={opened} id={opened} close={() => open(undefined)} />}
    </OpenApproval>
  );
}
