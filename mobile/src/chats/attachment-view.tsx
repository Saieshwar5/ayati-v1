/**
 * Files shared in a message (screen 11): photos as thumbnails, voice notes
 * with play and a folded transcript, and other files to download and open.
 * Every read carries the device token; stored files are private.
 */
import type { Attachment } from '@cuckoo/shared';
import { setAudioModeAsync, useAudioPlayer, useAudioPlayerStatus } from 'expo-audio';
import { Directory, File, Paths } from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import { useEffect, useState } from 'react';
import { Image, Pressable, StyleSheet, Text, View } from 'react-native';
import { useSession, useWorkspace } from '../session/session-context';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { useMutation } from '../ui/requests';
import { clock } from './use-voice-note';

const image = (mime: string) => mime === 'image/png' || mime === 'image/jpeg';

function useAuthHeaders() {
  const { token } = useSession();
  return { Authorization: `Bearer ${token() ?? ''}`, 'X-Ayati-Request': 'identity' };
}

export function sizeText(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Download to the cache, then open the phone's share sheet (Save to Files, open in…). */
function useOpenFile(file: Attachment) {
  const { api } = useWorkspace();
  const headers = useAuthHeaders();
  const mutation = useMutation();
  const open = () =>
    mutation.run(async () => {
      const folder = new Directory(Paths.cache, 'cuckoo-files', file.id);
      if (!folder.exists) folder.create({ intermediates: true });
      const target = new File(folder, file.name.replace(/[\\/]/g, '_'));
      if (!target.exists) {
        const task = File.createDownloadTask(api.objectUrl(file.id), target, { headers });
        const saved = await task.downloadAsync();
        if (!saved) throw new Error('The file could not be downloaded.');
      }
      if (!(await Sharing.isAvailableAsync())) throw new Error('This phone cannot open the file here.');
      await Sharing.shareAsync(target.uri, { mimeType: file.mime });
    });
  return { ...mutation, open };
}

function Photo({ file }: { file: Attachment }) {
  const { api } = useWorkspace();
  const headers = useAuthHeaders();
  const opener = useOpenFile(file);
  return (
    <Pressable accessibilityRole="imagebutton" accessibilityLabel={`Photo ${file.name}. Open`} onPress={() => void opener.open()}>
      <Image source={{ uri: api.objectUrl(file.id, 'thumbnail'), headers }} style={styles.photo} resizeMode="cover" />
    </Pressable>
  );
}

/** The player exists only once someone presses play. */
function VoicePlayer({ file, autoplay }: { file: Attachment; autoplay: boolean }) {
  const t = useTokens();
  const { api } = useWorkspace();
  const headers = useAuthHeaders();
  const player = useAudioPlayer({ uri: api.objectUrl(file.id, 'inline'), headers });
  const status = useAudioPlayerStatus(player);
  // Opened by pressing play, so it starts at once; play is idempotent if this runs twice.
  useEffect(() => {
    if (!autoplay) return;
    void setAudioModeAsync({ playsInSilentMode: true }).then(() => player.play());
  }, [autoplay, player]);
  const toggle = () => (status.playing ? player.pause() : player.play());
  return (
    <Pressable accessibilityRole="button" accessibilityLabel={status.playing ? 'Pause the voice note' : 'Play the voice note'}
      onPress={toggle} style={[styles.play, { borderColor: t.borderControl }]}>
      <Text style={{ color: t.text }}>{status.playing ? '❚❚' : '▶'}</Text>
      <Text style={[typography.small, { color: t.muted }]}>{clock(status.currentTime)}</Text>
    </Pressable>
  );
}

function Voice({ file }: { file: Attachment }) {
  const t = useTokens();
  const [playing, setPlaying] = useState(false);
  const [transcript, setTranscript] = useState(false);
  const length = file.duration_ms ? clock(file.duration_ms / 1000) : undefined;
  return (
    <View style={[styles.voice, { backgroundColor: t.soft }]}>
      <View style={styles.voiceLine}>
        {playing ? (
          <VoicePlayer file={file} autoplay />
        ) : (
          <Pressable accessibilityRole="button" accessibilityLabel="Play the voice note" onPress={() => setPlaying(true)}
            style={[styles.play, { borderColor: t.borderControl }]}>
            <Text style={{ color: t.text }}>▶</Text>
          </Pressable>
        )}
        <Text style={[typography.meta, { color: t.text, flex: 1 }]}>Voice note{length ? ` · ${length}` : ''}</Text>
      </View>
      {file.transcript ? (
        <Pressable accessibilityRole="button" accessibilityState={{ expanded: transcript }} onPress={() => setTranscript(!transcript)}>
          <Text style={[typography.small, { color: t.text, fontWeight: '600' }]}>{transcript ? 'Hide transcript' : 'Transcript'}</Text>
          {transcript ? <Text selectable style={[typography.meta, { color: t.text }]}>{file.transcript}</Text> : null}
        </Pressable>
      ) : file.transcript_status === 'pending' ? (
        <Text style={[typography.small, { color: t.muted }]}>Transcribing…</Text>
      ) : null}
    </View>
  );
}

function OtherFile({ file }: { file: Attachment }) {
  const t = useTokens();
  const opener = useOpenFile(file);
  return (
    <Pressable accessibilityRole="button" accessibilityLabel={`${file.name}, ${sizeText(file.size)}. Download`} disabled={opener.busy}
      onPress={() => void opener.open()} style={[styles.file, { borderColor: t.borderPanel, backgroundColor: t.surface }]}>
      <Text numberOfLines={1} style={[typography.meta, { color: t.text, flex: 1 }]}>📄 {file.name}</Text>
      <Text style={[typography.small, { color: t.muted }]}>{opener.busy ? 'Downloading…' : sizeText(file.size)}</Text>
      {opener.error ? <Text style={[typography.small, { color: t.danger }]}>{opener.error}</Text> : null}
    </Pressable>
  );
}

export function AttachmentList({ files }: { files: Attachment[] }) {
  if (!files.length) return null;
  return (
    <View style={styles.list}>
      {files.map(file =>
        image(file.mime) ? <Photo key={file.id} file={file} /> : file.mime.startsWith('audio/') ? <Voice key={file.id} file={file} /> : <OtherFile key={file.id} file={file} />,
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  list: { gap: 6 },
  photo: { width: 220, height: 160, borderRadius: radius.card - 4 },
  voice: { borderRadius: radius.button, padding: 10, gap: 6, minWidth: 220 },
  voiceLine: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  play: { minWidth: touchTarget, minHeight: touchTarget - 8, borderWidth: 1, borderRadius: radius.button, flexDirection: 'row', gap: 6, alignItems: 'center', justifyContent: 'center', paddingHorizontal: 10 },
  file: { minHeight: touchTarget, borderWidth: 1, borderRadius: radius.button, paddingHorizontal: 12, paddingVertical: 8, flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', gap: 8 },
});
