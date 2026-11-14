/** The composer's extra rows: what to attach, and a voice note being recorded. */
import type { Attachment } from '@cuckoo/shared';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { clock, MAX_SECONDS } from './use-voice-note';

export function AttachRow({ choose, disabled }: { choose: (what: 'camera' | 'library' | 'file') => void; disabled: boolean }) {
  const t = useTokens();
  const items: { what: 'camera' | 'library' | 'file'; label: string }[] = [
    { what: 'camera', label: 'Camera' },
    { what: 'library', label: 'Photos' },
    { what: 'file', label: 'File' },
  ];
  return (
    <View style={styles.row}>
      {items.map(item => (
        <Pressable key={item.what} accessibilityRole="button" accessibilityLabel={`Attach from ${item.label}`} disabled={disabled}
          onPress={() => choose(item.what)} style={[styles.chip, { borderColor: t.borderControl, opacity: disabled ? 0.5 : 1 }]}>
          <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>{item.label}</Text>
        </Pressable>
      ))}
      <Text style={[typography.small, { color: t.muted, alignSelf: 'center' }]}>Library files: on the web</Text>
    </View>
  );
}

export function FileChips({ files, remove, disabled }: { files: Attachment[]; remove: (id: string) => void; disabled: boolean }) {
  const t = useTokens();
  if (!files.length) return null;
  return (
    <View style={styles.row}>
      {files.map(file => (
        <Pressable key={file.id} accessibilityRole="button" accessibilityLabel={`Remove ${file.name}`} disabled={disabled}
          onPress={() => remove(file.id)} style={[styles.chip, { backgroundColor: t.selection, borderColor: t.border }]}>
          <Text numberOfLines={1} style={[typography.meta, { color: t.text, maxWidth: 180 }]}>{file.name} ×</Text>
        </Pressable>
      ))}
    </View>
  );
}

/** Replaces the composer row while recording: red dot, timer, live level, × to discard. */
export function RecordingRow({ seconds, level, cancel }: { seconds: number; level?: number; cancel: () => void }) {
  const t = useTokens();
  // Metering is in dB (about -60 quiet to 0 loud).
  const loudness = Math.max(0.05, Math.min(1, ((level ?? -60) + 60) / 60));
  return (
    <View style={[styles.recording, { backgroundColor: t.raised, borderColor: t.borderControl }]}>
      <Pressable accessibilityRole="button" accessibilityLabel="Discard the voice note" onPress={cancel} style={styles.cancel}>
        <Text style={{ color: t.muted, fontSize: 20 }}>×</Text>
      </Pressable>
      <View style={[styles.dot, { backgroundColor: t.danger }]} />
      <Text accessibilityLabel={`Recording, ${clock(seconds)} of ${clock(MAX_SECONDS)}`} style={[typography.body, { color: t.text }]}>
        {clock(seconds)}
      </Text>
      <View style={[styles.level, { backgroundColor: t.selection }]}>
        <View style={[styles.levelFill, { width: `${loudness * 100}%`, backgroundColor: t.text }]} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', flexWrap: 'wrap', gap: 8, paddingHorizontal: 4 },
  chip: { minHeight: touchTarget - 8, borderWidth: 1, borderRadius: radius.button, paddingHorizontal: 12, justifyContent: 'center' },
  recording: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: 10, borderWidth: 1, borderRadius: radius.bubble, paddingRight: 12, minHeight: touchTarget },
  cancel: { width: touchTarget, height: touchTarget, alignItems: 'center', justifyContent: 'center' },
  dot: { width: 10, height: 10, borderRadius: 5 },
  level: { flex: 1, height: 6, borderRadius: 3, overflow: 'hidden' },
  levelFill: { height: 6 },
});
