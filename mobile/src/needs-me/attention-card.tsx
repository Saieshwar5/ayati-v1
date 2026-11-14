/** One item that needs this person, with the action that opens it. */
import type { AttentionItem } from '@cuckoo/shared';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { shortTime } from '../chats/time';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { attentionKind } from './use-attention';

export function AttentionCard({ item, onOpen }: { item: AttentionItem; onOpen: (item: AttentionItem) => void }) {
  const t = useTokens();
  const kind = attentionKind(item);
  const action = item.kind === 'approval' ? 'Review' : item.kind === 'question' ? 'Answer' : 'Open';
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${kind.label} from ${item.agent_name}: ${item.title}. ${action}`}
      onPress={() => onOpen(item)}
      style={({ pressed }) => [styles.card, { borderColor: t.borderPanel, backgroundColor: pressed ? t.soft : t.surface }]}
    >
      <View style={styles.top}>
        <Text style={[typography.small, styles.kind, { color: t[kind.tone] }]}>{kind.label.toUpperCase()}</Text>
        <Text style={[typography.small, { color: t.muted }]}>{shortTime(item.at)}</Text>
      </View>
      <Text numberOfLines={2} style={[typography.body, { color: t.text }]}>
        {item.operation_title ?? item.title}
      </Text>
      <View style={styles.top}>
        <Text numberOfLines={1} style={[typography.meta, { color: t.muted, flexShrink: 1 }]}>
          {item.agent_name} · {item.conversation_name}
        </Text>
        <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>{action} ›</Text>
      </View>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  card: { borderWidth: 1, borderRadius: radius.card, padding: 14, gap: 6, minHeight: touchTarget },
  top: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', gap: 8 },
  kind: { fontWeight: '700', letterSpacing: 0.5 },
});
