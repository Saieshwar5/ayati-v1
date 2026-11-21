import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from 'react';
import { ResponseReader } from './reader';
import { FileViewer } from './file-viewer';
import { ImageViewer } from './image-viewer';
import { PageReader } from '../pages/page-reader';
import type { ResultSource, Selection, ViewerImage } from './model';
import './results.css';
type Open = (selection: Selection) => void;
const ResultContext = createContext<Open | undefined>(undefined);
const SourceContext = createContext<ResultSource | undefined>(undefined);
const ReadingContext = createContext<((key: string, open: boolean) => void) | undefined>(undefined);
export const useResultSource = () => useContext(SourceContext);
export const useResultReading = () => useContext(ReadingContext);
export const useResults = () => useContext(ResultContext);
export function ResultDialog({ selection, source, images, close }: { selection: Selection; source: ResultSource; images?: () => ViewerImage[]; close: () => void }) {
  if (selection.kind === 'page') return <PageReader key={selection.id} id={selection.id} source={source} close={close} />;
  if (selection.kind === 'image') return <ImageViewer key={selection.id} id={selection.id} name={selection.name} images={images?.() ?? []} source={source} close={close} />;
  return selection.kind === 'response' ? <ResponseReader key={selection.id} id={selection.id} source={source} close={close} /> : <FileViewer key={selection.id} {...selection} source={source} close={close} />;
}
/** `images` lists the conversation's pictures for the image viewer. It is called only while
 * that viewer is open, so a conversation does not build the list on every render. */
export function Results({ source, children, changed, images }: { source: ResultSource; children: ReactNode; changed: (open: boolean) => void; images?: () => ViewerImage[] }) {
  const [selection, setSelection] = useState<Selection>();
  const readers = useRef(new Set<string>());
  const notify = useRef(changed); notify.current = changed;
  const reading = useCallback((key: string, open: boolean) => {
    if (open) readers.current.add(key); else readers.current.delete(key);
    notify.current(readers.current.size > 0);
  }, []);
  return <SourceContext value={source}><ReadingContext value={reading}><ResultContext value={next => { reading('dialog', true); setSelection(next); }}>{children}{selection && <ResultDialog selection={selection} source={source} images={images} close={() => { setSelection(undefined); reading('dialog', false); }} />}</ResultContext></ReadingContext></SourceContext>;
}
