/** What a shared file is, for its row in a conversation: the letters on its tile, the word
 * beside its size, and which tile colour it gets. Colour only tells common types apart. */
export type FileKind = { tile: string; label: string; tone?: 'pdf' | 'doc' | 'sheet' };

const byExtension: Record<string, FileKind> = {
  pdf: { tile: 'PDF', label: 'PDF', tone: 'pdf' },
  doc: { tile: 'DOC', label: 'Document', tone: 'doc' }, docx: { tile: 'DOC', label: 'Document', tone: 'doc' },
  odt: { tile: 'DOC', label: 'Document', tone: 'doc' }, rtf: { tile: 'DOC', label: 'Document', tone: 'doc' },
  xls: { tile: 'XLS', label: 'Spreadsheet', tone: 'sheet' }, xlsx: { tile: 'XLS', label: 'Spreadsheet', tone: 'sheet' },
  ods: { tile: 'XLS', label: 'Spreadsheet', tone: 'sheet' }, csv: { tile: 'CSV', label: 'Spreadsheet', tone: 'sheet' },
  ppt: { tile: 'PPT', label: 'Presentation' }, pptx: { tile: 'PPT', label: 'Presentation' },
  zip: { tile: 'ZIP', label: 'Archive' }, rar: { tile: 'RAR', label: 'Archive' }, '7z': { tile: '7Z', label: 'Archive' },
  tar: { tile: 'TAR', label: 'Archive' }, gz: { tile: 'GZ', label: 'Archive' },
  txt: { tile: 'TXT', label: 'Text' }, md: { tile: 'MD', label: 'Text' }, log: { tile: 'LOG', label: 'Text' },
  json: { tile: 'JSON', label: 'Text' },
};

const extension = (name: string) => { const dot = name.lastIndexOf('.'); return dot > 0 ? name.slice(dot + 1).toLowerCase() : ''; };

export function fileKind(name: string, mime: string): FileKind {
  const ending = extension(name), known = byExtension[ending];
  if (known) return known;
  if (mime === 'application/pdf') return byExtension.pdf;
  if (mime.startsWith('image/')) return { tile: 'IMG', label: 'Image' };
  if (mime.startsWith('video/')) return { tile: 'VID', label: 'Video' };
  if (mime.startsWith('audio/')) return { tile: 'AUD', label: 'Audio' };
  if (mime.startsWith('text/')) return { tile: 'TXT', label: 'Text' };
  // An unknown type shows its own ending when that is short enough to read on the tile.
  return { tile: /^[a-z0-9]{1,4}$/.test(ending) ? ending.toUpperCase() : 'FILE', label: 'File' };
}

/** "26 KB", "1.2 MB": whole kilobytes, then megabytes to one decimal. */
export const fileSize = (bytes: number) => bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;

/** A name in two parts, so a row too narrow for it drops the middle and keeps the ending
 * ("046199498_Rel_180…32753.pdf"). Short names are not split. */
export function splitName(name: string, keep = 9): { head: string; tail: string } {
  return name.length <= keep * 2 ? { head: name, tail: '' } : { head: name.slice(0, -keep), tail: name.slice(-keep) };
}
