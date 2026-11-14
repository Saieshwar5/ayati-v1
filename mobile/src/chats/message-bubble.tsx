/**
 * One message. Own messages sit on the right in the selection fill; agents'
 * replies sit under their author row in the soft box, with live progress in
 * purple while they work (designs/mobile-app, screen 8).
 */
import { filesOnlyContent, plainText, type Message } from '@cuckoo/shared';
import { memo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { Face, type FaceIdentity } from '../faces/face';
import { AiTag } from '../ui/controls';
import { shortTime } from './time';

type Props = {
  message: Message;
  mine: boolean;
  agent: boolean;
  face: FaceIdentity;
  workspace: string;
  /** Offered on a reply still being worked on. */
  onStop?: (message: Message) => void;
  stopping?: boolean;
};

function stateLabel(message: Message): string | undefined {
  if (message.state === 'failed') return 'Could not finish';
  if (message.state === 'cancelled') return 'Stopped';
  if (message.progress && !['sent', 'complete'].includes(message.state)) return message.progress.text || 'Working…';
  if (['queued', 'pending', 'streaming', 'running'].includes(message.state)) return 'Working…';
  return undefined;
}

export const MessageBubble = memo(function MessageBubble({ message, mine, agent, face, workspace, onStop, stopping }: Props) {
  const t = useTokens();
  const text = message.content === filesOnlyContent && message.attachments?.length ? '' : plainText(message.content);
  const files = message.attachments ?? [];
  const working = stateLabel(message);
  const author = message.name ?? (agent ? 'Agent' : 'Someone');
  if (mine) {
    return (
      <View style={styles.mineRow}>
        <View style={[styles.bubble, { backgroundColor: t.selection }]}>
          {text ? <Text selectable style={[typography.body, { color: t.text }]}>{text}</Text> : null}
          {files.map(file => (
            <Text key={file.id} style={[typography.meta, { color: t.muted }]}>📎 {file.name}</Text>
          ))}
        </View>
        <Text style={[typography.small, { color: t.muted }]}>{shortTime(message.at)}</Text>
      </View>
    );
  }
  return (
    <View style={styles.otherRow}>
      <Face {...face} size={32} workspace={workspace} />
      <View style={styles.otherBody}>
        <View style={styles.author}>
          <Text style={[typography.meta, styles.authorName, { color: t.text }]}>{author}</Text>
          {agent ? <AiTag /> : null}
          <Text style={[typography.small, { color: t.muted, marginLeft: 8 }]}>{shortTime(message.at)}</Text>
        </View>
        <View style={agent ? [styles.reply, { backgroundColor: t.soft }] : undefined}>
          {text ? <Text selectable style={[typography.body, { color: t.text }]}>{text}</Text> : null}
          {files.map(file => (
            <Text key={file.id} style={[typography.meta, { color: t.muted }]}>📎 {file.name}</Text>
          ))}
          {working || onStop ? (
            <View style={styles.progress}>
              {working ? (
                <Text accessibilityLiveRegion="polite" style={[typography.meta, styles.progressText, { color: message.state === 'failed' ? t.warning : t.working }]}>
                  {working}
                </Text>
              ) : null}
              {onStop ? (
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel={`Stop ${author}`}
                  accessibilityState={{ disabled: stopping, busy: stopping }}
                  disabled={stopping}
                  onPress={() => onStop(message)}
                  style={[styles.stop, { borderColor: t.borderControl }]}
                >
                  <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>{stopping ? 'Stopping…' : 'Stop'}</Text>
                </Pressable>
              ) : null}
            </View>
          ) : null}
        </View>
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  mineRow: { alignItems: 'flex-end', gap: 2, paddingHorizontal: 16, paddingVertical: 4 },
  bubble: { maxWidth: '82%', borderRadius: radius.bubble, paddingHorizontal: 14, paddingVertical: 9, gap: 4 },
  otherRow: { flexDirection: 'row', gap: 10, paddingHorizontal: 16, paddingVertical: 6 },
  otherBody: { flex: 1, gap: 4 },
  author: { flexDirection: 'row', alignItems: 'center' },
  authorName: { fontWeight: '600' },
  reply: { borderRadius: radius.card - 2, padding: 12, gap: 6 },
  progress: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  progressText: { flex: 1 },
  stop: { minHeight: touchTarget - 8, minWidth: touchTarget, borderWidth: 1, borderRadius: radius.button, paddingHorizontal: 12, alignItems: 'center', justifyContent: 'center' },
});
