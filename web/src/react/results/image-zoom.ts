/** Zoom arithmetic for the image viewer. A scale is a fraction of the image's real size (1 = 100%). */
export type Size = { width: number; height: number };
export type Point = { x: number; y: number };
export type View = { scale: number; offset: Point };

export const maxScale = 4;
const stops = [0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4];
const centred: Point = { x: 0, y: 0 };

const within = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

/** The largest scale at which the whole image is visible. A small image is not enlarged. */
export function fitScale(image: Size, stage: Size): number {
  if (!image.width || !image.height || !stage.width || !stage.height) return 1;
  return Math.min(1, stage.width / image.width, stage.height / image.height);
}

export const clampScale = (scale: number, fit: number) => within(scale, fit, maxScale);

/** True when the image is larger than its fitted size. */
export const isZoomed = (scale: number, fit: number) => scale > fit * 1.001;

/** The next round zoom level above or below `scale`. Zooming out ends at the fitted size. */
export function stepScale(scale: number, fit: number, direction: 1 | -1): number {
  // A stop almost equal to the fitted size would be a step nobody can see.
  const levels = [fit, ...stops.filter(stop => stop > fit * 1.1)];
  if (direction > 0) return levels.find(level => level > scale * 1.01) ?? maxScale;
  return levels.reverse().find(level => level < scale * 0.99) ?? fit;
}

/** Keeps the image over the stage: it moves only as far as it overflows on each axis. */
export function clampOffset(offset: Point, image: Size, stage: Size, scale: number): Point {
  const x = Math.max(0, (image.width * scale - stage.width) / 2);
  const y = Math.max(0, (image.height * scale - stage.height) / 2);
  return { x: within(offset.x, -x, x), y: within(offset.y, -y, y) };
}

/** The offset after changing scale so the part of the image under `anchor` stays there.
 * Offsets and the anchor are measured from the centre of the stage. */
export function zoomAt(offset: Point, from: number, to: number, anchor: Point): Point {
  const ratio = to / from;
  return { x: anchor.x - (anchor.x - offset.x) * ratio, y: anchor.y - (anchor.y - offset.y) * ratio };
}

/** The view actually shown for a stored one: fitted and centred when there is none or it no longer applies. */
export function shownView(view: View | undefined, image: Size, stage: Size): View {
  const fit = fitScale(image, stage);
  if (!view || !isZoomed(view.scale, fit)) return { scale: fit, offset: centred };
  const scale = clampScale(view.scale, fit);
  return { scale, offset: clampOffset(view.offset, image, stage, scale) };
}

/** The view to store after a change. Undefined means fitted, which then follows the window size. */
export function nextView(change: View, image: Size, stage: Size): View | undefined {
  const fit = fitScale(image, stage);
  const scale = clampScale(change.scale, fit);
  return isZoomed(scale, fit) ? { scale, offset: clampOffset(change.offset, image, stage, scale) } : undefined;
}
