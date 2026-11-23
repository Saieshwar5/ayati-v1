import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type MouseEvent, type PointerEvent, type SyntheticEvent } from 'react';
import { clampScale, fitScale, isZoomed, maxScale, nextView, shownView, stepScale, zoomAt, type Point, type Size, type View } from './image-zoom';

const unknown: Size = { width: 0, height: 0 };
/** A sideways touch this long (px) at the fitted size moves to the neighbouring image. */
const swipeDistance = 60;

/** Fit, zoom and pan for one image inside a stage element. The caller renders the stage and
 * the image with what this returns, and calls `reset` when it shows a different image. */
export function useImageZoom({ swipe, dismiss }: { swipe: (step: 1 | -1) => void; dismiss: () => void }) {
  const stage = useRef<HTMLDivElement>(null);
  const [stageSize, setStageSize] = useState<Size>(unknown);
  const [natural, setNatural] = useState<Size>(unknown);
  const [view, setView] = useState<View>();
  const pointers = useRef(new Map<number, Point>());
  const gesture = useRef({ travel: { x: 0, y: 0 }, pinched: false, moved: false });

  useLayoutEffect(() => {
    const node = stage.current;
    if (!node || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(entries => {
      const box = entries[0].contentRect;
      setStageSize({ width: box.width, height: box.height });
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const ready = natural.width > 0 && stageSize.width > 0;
  const fit = fitScale(natural, stageSize);
  const shown = shownView(view, natural, stageSize);
  const zoomed = ready && isZoomed(shown.scale, fit);

  /** Changes the view from its latest state, so quick wheel and pointer events build on each other. */
  const change = (next: (current: View) => View) => {
    if (ready) setView(stored => nextView(next(shownView(stored, natural, stageSize)), natural, stageSize));
  };
  const scaleTo = (target: (scale: number) => number, anchor: Point = { x: 0, y: 0 }) => change(current => {
    const scale = clampScale(target(current.scale), fit);
    return { scale, offset: zoomAt(current.offset, current.scale, scale, anchor) };
  });
  const pan = (x: number, y: number) => change(current => ({ scale: current.scale, offset: { x: current.offset.x + x, y: current.offset.y + y } }));

  /** A pointer position measured from where a centred picture's middle is: the centre of
   * the stage inside its padding, which is not the same on every side. */
  const fromCentre = (event: { clientX: number; clientY: number }): Point => {
    const node = stage.current!, box = node.getBoundingClientRect(), style = getComputedStyle(node);
    const left = box.left + parseFloat(style.paddingLeft), right = box.right - parseFloat(style.paddingRight);
    const top = box.top + parseFloat(style.paddingTop), bottom = box.bottom - parseFloat(style.paddingBottom);
    return { x: event.clientX - (left + right) / 2, y: event.clientY - (top + bottom) / 2 };
  };

  // React's wheel listener is passive, so the page behind could not be kept still from it.
  const wheel = useRef<(event: WheelEvent) => void>(() => {});
  wheel.current = event => {
    event.preventDefault();
    const pixels = event.deltaMode === 1 ? event.deltaY * 16 : event.deltaY;
    scaleTo(scale => scale * Math.exp(-pixels * 0.0015), fromCentre(event));
  };
  useEffect(() => {
    const node = stage.current;
    if (!node) return;
    const listener = (event: WheelEvent) => wheel.current(event);
    node.addEventListener('wheel', listener, { passive: false });
    return () => node.removeEventListener('wheel', listener);
  }, []);

  const overImage = (event: MouseEvent) => {
    const box = stage.current?.querySelector('img')?.getBoundingClientRect();
    return Boolean(box && event.clientX >= box.left && event.clientX <= box.right && event.clientY >= box.top && event.clientY <= box.bottom);
  };

  const stageProps = {
    ref: stage,
    onPointerDown: (event: PointerEvent<HTMLDivElement>) => {
      if (event.pointerType === 'mouse' && event.button !== 0) return;
      if (pointers.current.size === 0) gesture.current = { travel: { x: 0, y: 0 }, pinched: false, moved: false };
      pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (pointers.current.size > 1) gesture.current.pinched = true;
      // Keeps a drag going when the pointer leaves the stage.
      event.currentTarget.setPointerCapture?.(event.pointerId);
    },
    onPointerMove: (event: PointerEvent<HTMLDivElement>) => {
      const before = pointers.current.get(event.pointerId);
      if (!before) return;
      const now = { x: event.clientX, y: event.clientY };
      pointers.current.set(event.pointerId, now);
      const state = gesture.current;
      if (pointers.current.size === 1) {
        state.travel = { x: state.travel.x + now.x - before.x, y: state.travel.y + now.y - before.y };
        if (Math.hypot(state.travel.x, state.travel.y) > 4) state.moved = true;
        if (zoomed) pan(now.x - before.x, now.y - before.y);
        return;
      }
      const other = [...pointers.current].find(([id]) => id !== event.pointerId)?.[1];
      if (!other) return;
      const spread = Math.hypot(now.x - other.x, now.y - other.y), was = Math.hypot(before.x - other.x, before.y - other.y);
      if (was > 0) scaleTo(scale => scale * spread / was, fromCentre({ clientX: (now.x + other.x) / 2, clientY: (now.y + other.y) / 2 }));
    },
    onPointerUp: (event: PointerEvent<HTMLDivElement>) => {
      if (!pointers.current.delete(event.pointerId) || pointers.current.size > 0) return;
      const { travel, pinched } = gesture.current;
      const sideways = Math.abs(travel.x) > swipeDistance && Math.abs(travel.x) > 2 * Math.abs(travel.y);
      if (event.pointerType !== 'mouse' && !zoomed && !pinched && sideways) swipe(travel.x < 0 ? 1 : -1);
    },
    onPointerCancel: (event: PointerEvent<HTMLDivElement>) => { pointers.current.delete(event.pointerId); },
    onDoubleClick: (event: MouseEvent<HTMLDivElement>) => {
      if (!overImage(event)) return;
      if (zoomed) setView(undefined); else scaleTo(() => (fit >= 1 ? 2 : 1), fromCentre(event));
    },
    // A click beside the image closes the viewer, as a click on a dialog's backdrop would.
    onClick: (event: MouseEvent<HTMLDivElement>) => { if (!gesture.current.moved && !gesture.current.pinched && !overImage(event)) dismiss(); },
  };

  const imageStyle: CSSProperties | undefined = ready
    ? { width: natural.width * shown.scale, height: natural.height * shown.scale, transform: `translate(${shown.offset.x}px, ${shown.offset.y}px)` }
    : undefined;

  return {
    stageProps, imageStyle, ready, zoomed,
    percent: Math.round(shown.scale * 100),
    canZoomIn: ready && shown.scale < maxScale,
    zoomIn: () => scaleTo(scale => stepScale(scale, fit, 1)),
    zoomOut: () => scaleTo(scale => stepScale(scale, fit, -1)),
    fitToWindow: () => setView(undefined),
    pan,
    loaded: (event: SyntheticEvent<HTMLImageElement>) => setNatural({ width: event.currentTarget.naturalWidth, height: event.currentTarget.naturalHeight }),
    reset: () => { setView(undefined); setNatural(unknown); },
  };
}
