import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from 'react';
import { ResponseReader } from './reader';
import { FileViewer } from './file-viewer';
import type { ResultSource, Selection } from './model';
import './results.css';
type Open = (selection: Selection) => void;
const ResultContext = createContext<Open | undefined>(undefined);
const SourceContext = createContext<ResultSource | undefined>(undefined);
const ReadingContext = createContext<((key: string, open: boolean) => void) | undefined>(undefined);
export const useResultSource = () => useContext(SourceContext);
export const useResultReading = () => useContext(ReadingContext);
export const useResults = () => useContext(ResultContext);
export function ResultDialog({ selection, source, close }: { selection: Selection; source: ResultSource; close: () => void }) {
  return selection.kind === 'response' ? <ResponseReader key={selection.id} id={selection.id} source={source} close={close} /> : <FileViewer key={selection.id} {...selection} source={source} close={close} />;
}
export function Results({ source, children, changed }: { source: ResultSource; children: ReactNode; changed: (open: boolean) => void }) {
  const [selection, setSelection] = useState<Selection>();
  const readers = useRef(new Set<string>());
  const notify = useRef(changed); notify.current = changed;
  const reading = useCallback((key: string, open: boolean) => {
    if (open) readers.current.add(key); else readers.current.delete(key);
    notify.current(readers.current.size > 0);
  }, []);
  return <SourceContext value={source}><ReadingContext value={reading}><ResultContext value={next => { reading('dialog', true); setSelection(next); }}>{children}{selection && <ResultDialog selection={selection} source={source} close={() => { setSelection(undefined); reading('dialog', false); }} />}</ResultContext></ReadingContext></SourceContext>;
}
