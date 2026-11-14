/**
 * Choosing and storing files for a message, with the web's limits: up to
 * 10 MiB each and eight per message. The bytes go straight to the
 * conversation; the message then shares their ids.
 */
import type { Attachment, WorkspaceClient } from '@cuckoo/shared';
import * as DocumentPicker from 'expo-document-picker';
import { File } from 'expo-file-system';
import * as ImagePicker from 'expo-image-picker';

export const MAX_BYTES = 10 * 1024 * 1024;
export const MAX_FILES = 8;

export type Picked = { uri: string; name: string; mime: string; size: number };

/** A photo from the camera or the library, re-encoded as JPEG where the phone allows. */
export async function pickPhoto(source: 'camera' | 'library'): Promise<Picked | undefined> {
  if (source === 'camera') {
    const permission = await ImagePicker.requestCameraPermissionsAsync();
    if (!permission.granted) throw new Error('Allow camera access in Settings to take a photo.');
  }
  const options: ImagePicker.ImagePickerOptions = {
    mediaTypes: 'images',
    quality: 0.8,
    preferredAssetRepresentationMode: ImagePicker.UIImagePickerPreferredAssetRepresentationMode.Compatible,
  };
  const result = source === 'camera' ? await ImagePicker.launchCameraAsync(options) : await ImagePicker.launchImageLibraryAsync(options);
  if (result.canceled || !result.assets?.length) return undefined;
  const asset = result.assets[0];
  const mime = asset.mimeType ?? 'image/jpeg';
  const extension = mime === 'image/png' ? 'png' : 'jpg';
  return { uri: asset.uri, name: asset.fileName ?? `photo.${extension}`, mime, size: asset.fileSize ?? 0 };
}

export async function pickFile(): Promise<Picked | undefined> {
  const result = await DocumentPicker.getDocumentAsync({ copyToCacheDirectory: true, multiple: false });
  if (result.canceled || !result.assets?.length) return undefined;
  const asset = result.assets[0];
  return { uri: asset.uri, name: asset.name, mime: asset.mimeType ?? 'application/octet-stream', size: asset.size ?? 0 };
}

/** Why this file can't be added, or undefined when it can. */
export function fileProblem(picked: Picked, already: number): string | undefined {
  if (already >= MAX_FILES) return 'A message can share at most eight files.';
  if (picked.size > MAX_BYTES) return 'Choose a file up to 10 MiB.';
  return undefined;
}

/** The server keeps names to 180 characters without slashes or control characters. */
export function safeName(name: string): string {
  const cleaned = Array.from(name.replace(/[\\/]/g, '_')).filter(char => char >= ' ' && char !== '\u007f').join('').trim();
  return (cleaned || 'file').slice(0, 180);
}

export async function store(api: WorkspaceClient, chat: string, picked: Picked, signal?: AbortSignal): Promise<Attachment> {
  const file = new File(picked.uri);
  const problem = fileProblem({ ...picked, size: picked.size || file.size }, 0);
  if (problem) throw new Error(problem);
  return api.upload(chat, safeName(picked.name), picked.mime, file, signal);
}
