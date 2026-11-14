/** One conversation in the list: face, name with the AI tag, kind and unread count. */
import { conversationKind, conversationName, type Conversation, type Directory } from '@cuckoo/shared';
import { memo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { AiTag, Count, Face } from '../ui/controls';
import { shortTime } from './time';

type Props = {
  chat: Conversation;
  directory: Directory;
  viewer: string;
  unread: number;
  onOpen: (id: string) => void;
};

export const ChatRow = memo(function ChatRow({ chat, directory, viewer, unread, onOpen }: Props) {
  const t = useTokens();
  const name = conversationName(chat, directory, viewer);
  const kind = conversationKind(chat, directory, viewer);
  const detail =
    kind === 'Channel'
      ? `${chat.users.length} people${chat.agents.length ? ` · ${chat.agents.length} agents` : ''}`
      : kind === 'Agent'
        ? (directory.agents.find(agent => chat.agents.includes(agent.id))?.title ?? 'Agent')
        : 'Direct message';
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${name}${kind === 'Agent' ? ', AI agent' : ''}${unread ? `, ${unread} unread` : ''}`}
      onPress={() => onOpen(chat.id)}
      style={({ pressed }) => [styles.row, { borderBottomColor: t.border, backgroundColor: pressed ? t.soft : t.surface }]}
    >
      <Face name={kind === 'Channel' ? `# ${name}` : name} agent={kind === 'Agent'} />
      <View style={styles.text}>
        <View style={styles.title}>
          <Text numberOfLines={1} style={[typography.header, styles.name, { color: t.text }]}>
            {kind === 'Channel' ? `# ${name}` : name}
          </Text>
          {kind === 'Agent' ? <AiTag /> : null}
        </View>
        <Text numberOfLines={1} style={[typography.meta, { color: t.muted }]}>
          {detail}
        </Text>
      </View>
      <View style={styles.side}>
        {chat.activity ? <Text style={[typography.small, { color: t.muted }]}>{shortTime(chat.activity)}</Text> : null}
        <Count value={unread} />
      </View>
    </Pressable>
  );
});

const styles = StyleSheet.create({
  row: {
    minHeight: 68,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 10,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  text: { flex: 1, gap: 2 },
  title: { flexDirection: 'row', alignItems: 'center' },
  name: { flexShrink: 1 },
  side: { alignItems: 'flex-end', gap: 4 },
});
