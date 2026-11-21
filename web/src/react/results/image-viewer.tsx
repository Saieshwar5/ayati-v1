import { useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { Button, useModal } from '../ui';
import { query, type ResultSource, type ViewerImage } from './model';
import { useImageZoom } from './use-image-zoom';
import { useSavedFile } from './use-saved-file';
import './image-viewer.css';

/** How far an arrow key moves a zoomed image (px). */
const keyStep = 80;

function Icon({ children }: { children: ReactNode }) {
  return <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{children}</svg>;
}

const sentAt = (seconds: number) => {
  const date = new Date(seconds * 1000);
  const sameYear = date.getFullYear() === new Date().getFullYear();
  return date.toLocaleString([], { day: 'numeric', month: 'short', year: sameYear ? undefined : 'numeric', hour: 'numeric', minute: '2-digit' });
};

/** A picture from a conversation, shown over the whole window. `images` are the other
 * pictures of that conversation, for the arrows; it may be empty or not hold this one. */
export function ImageViewer({ id: opened, name, images, source, close }: {
  id: string; name: string; images: ViewerImage[]; source: ResultSource; close: () => void;
}) {
  const [id, setId] = useState(opened);
  const [failed, setFailed] = useState<string>();
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  useModal(dialog, closeButton);
  const { file, error, retry } = useSavedFile(id, source);

  const index = images.findIndex(image => image.id === id);
  const known = index >= 0 ? images[index] : undefined;
  // The name it was opened by, unless the arrows have moved on to another picture.
  const title = known?.name ?? (id === opened ? name : file?.name ?? name);
  const show = (step: 1 | -1) => {
    const next = index >= 0 ? images[index + step] : undefined;
    if (!next) return;
    setId(next.id);
    zoom.reset();
  };
  const zoom = useImageZoom({ swipe: show, dismiss: close });
  const broken = failed === id;
  const several = index >= 0 && images.length > 1;
  const first = index <= 0, last = index >= images.length - 1;
  // Until the picture has loaded its real size is unknown; it is shown fitted.
  const level = zoom.ready ? `${zoom.percent}%` : 'Fit';

  const keys = (event: KeyboardEvent) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const { key } = event;
    if (key === '+' || key === '=') zoom.zoomIn();
    else if (key === '-') zoom.zoomOut();
    else if (key === '0') zoom.fitToWindow();
    else if (key === 'ArrowLeft') { if (zoom.zoomed) zoom.pan(keyStep, 0); else show(-1); }
    else if (key === 'ArrowRight') { if (zoom.zoomed) zoom.pan(-keyStep, 0); else show(1); }
    else if (key === 'ArrowUp' && zoom.zoomed) zoom.pan(0, keyStep);
    else if (key === 'ArrowDown' && zoom.zoomed) zoom.pan(0, -keyStep);
    else return;
    event.preventDefault();
  };

  // Controls at their limit stay focusable (aria-disabled, not disabled), so the keyboard
  // shortcuts keep working after the last step with the focus still on that button.
  return createPortal(
    <dialog
      ref={dialog}
      className="image-viewer"
      aria-label={title}
      onKeyDown={keys}
      onCancel={event => { event.preventDefault(); event.stopPropagation(); close(); }}
    >
      <header className="image-viewer-bar">
        <div className="image-viewer-who">
          {known?.face}
          <div>
            {known?.by && (
              <p className="image-viewer-sender">
                <strong>{known.by}</strong>
                {known.at !== undefined && <time dateTime={new Date(known.at * 1000).toISOString()}>{sentAt(known.at)}</time>}
              </p>
            )}
            <p className={known?.by ? 'image-viewer-file' : 'image-viewer-file image-viewer-file-only'} title={title}>
              <span>{title}</span>
              {file && <span> · {Math.ceil(file.size / 1024)} KB</span>}
            </p>
          </div>
        </div>
        <div className="image-viewer-tools">
          {!broken && (
            <div className="image-viewer-zoom" role="group" aria-label="Zoom">
              <button type="button" aria-label="Zoom out" aria-disabled={!zoom.zoomed} onClick={zoom.zoomOut}>
                <Icon><path d="M5 12h14" /></Icon>
              </button>
              <button type="button" className="image-viewer-level" aria-label={`${level}, fit to window`} title="Fit to window" onClick={zoom.fitToWindow}>
                {level}
              </button>
              <button type="button" aria-label="Zoom in" aria-disabled={!zoom.canZoomIn} onClick={zoom.zoomIn}>
                <Icon><path d="M12 5v14M5 12h14" /></Icon>
              </button>
            </div>
          )}
          {file && (
            <a href={source.href(query(source.file(id), { version: file.checksum }))} download={file.name} aria-label="Download original" title="Download original">
              <Icon><path d="M12 4v11M7 11l5 5 5-5M5 20h14" /></Icon>
            </a>
          )}
          <button ref={closeButton} type="button" aria-label="Close" title="Close" onClick={close}>
            <Icon><path d="M6 6l12 12M18 6 6 18" /></Icon>
          </button>
        </div>
      </header>
      {error && <p className="chat-error" role="alert">{error} <Button onClick={() => { setFailed(undefined); retry(); }}>Retry</Button></p>}
      <div className="image-viewer-body">
        <div {...zoom.stageProps} className={`image-viewer-stage${zoom.zoomed ? ' is-zoomed' : ''}`}>
          {file && !broken && (
            <img
              key={id}
              src={source.href(query(source.file(id), { inline: true, version: file.checksum }))}
              alt={title}
              draggable={false}
              style={zoom.imageStyle}
              onLoad={zoom.loaded}
              onError={() => setFailed(id)}
            />
          )}
          {broken && <p className="image-viewer-note">Preview unavailable for this image. Download the original to open it.</p>}
          {!file && !error && <p className="image-viewer-note" role="status">Loading image…</p>}
        </div>
        {several && (
          <button type="button" className="image-viewer-step image-viewer-previous" aria-label="Previous image" title="Previous image" aria-disabled={first} onClick={() => show(-1)}>
            <Icon><path d="M15 6l-6 6 6 6" /></Icon>
          </button>
        )}
        {several && (
          <button type="button" className="image-viewer-step image-viewer-next" aria-label="Next image" title="Next image" aria-disabled={last} onClick={() => show(1)}>
            <Icon><path d="M9 6l6 6-6 6" /></Icon>
          </button>
        )}
        <p className="image-viewer-foot" role="status">
          {zoom.zoomed ? <span className="image-viewer-hint">Drag to move · Double-click to fit</span>
            : several && <span>{index + 1} of {images.length}</span>}
        </p>
      </div>
    </dialog>,
    document.body,
  );
}
