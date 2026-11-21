import { isViewerImage, type ViewerImage } from '../results/model';
import { messageAuthor } from './message-author';
import type { Directory, Message } from './model';

/** The pictures of the loaded part of a conversation, oldest first, for the image viewer's
 * arrows and its "who shared this" line. A picture shared again (a forward) is listed once,
 * where it first appears; deleted messages show no files, so theirs are left out. */
export function conversationImages(messages: Message[], directory: Directory): ViewerImage[] {
  const listed = new Set<string>();
  const images: ViewerImage[] = [];
  for (const message of messages) {
    if (message.state === 'deleted') continue;
    const pictures = (message.attachments ?? []).filter(file => isViewerImage(file.mime) && !listed.has(file.id));
    if (!pictures.length) continue;
    const { name, own, face } = messageAuthor(message, directory);
    for (const file of pictures) {
      listed.add(file.id);
      images.push({ id: file.id, name: file.name, by: own ? 'You' : name, at: message.at, face });
    }
  }
  return images;
}
