import { createContext, useContext, useState, type ReactNode } from 'react';
import { ResponseReader } from './reader';
import { FileViewer } from './file-viewer';
import type { ResultSource, Selection } from './model';
import './results.css';
type Open = (selection: Selection) => void;
const ResultContext = createContext<Open | undefined>(undefined);
export const useResults = () => useContext(ResultContext);
export function ResultDialog({ selection, source, close }: { selection: Selection; source: ResultSource; close: () => void }) {
  return selection.kind === 'response' ? <ResponseReader key={selection.id} id={selection.id} source={source} close={close} /> : <FileViewer key={selection.id} {...selection} source={source} close={close} />;
}
export function Results({ source, children, changed }: { source: ResultSource; children: ReactNode; changed: (open: boolean) => void }) {
  const [selection, setSelection] = useState<Selection>();
  return <ResultContext value={next => { changed(true); setSelection(next); }}>{children}{selection && <ResultDialog selection={selection} source={source} close={() => { setSelection(undefined); changed(false); }} />}</ResultContext>;
}
