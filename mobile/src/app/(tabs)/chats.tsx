/** Screen 7: Chats, with search and filters. */
import { conversationName, filterConversations, type ChatFilter } from '@cuckoo/shared';
import { useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { FlatList, RefreshControl, StyleSheet, Text, TextInput, View } from 'react-native';
import { ChatRow } from '../../chats/chat-row';
import { FilterChips } from '../../chats/filter-chips';
import { useChatList } from '../../chats/use-chat-list';
import { useWorkspace } from '../../session/session-context';
import { radius, touchTarget, type as typography } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { ConnectionBanner } from '../../ui/connection-banner';
import { ErrorText } from '../../ui/controls';
import { Screen } from '../../ui/screen';

export default function Chats() {
  const t = useTokens();
  const router = useRouter();
  const { user, workspace } = useWorkspace();
  const { data, error, loading, unread, refresh } = useChatList();
  const [filter, setFilter] = useState<ChatFilter>('all');
  const [search, setSearch] = useState('');
  const open = useCallback((id: string) => router.push({ pathname: '/chat/[id]', params: { id } }), [router]);

  const query = search.trim().toLowerCase();
  const chats = data
    ? filterConversations(data.chats, filter, data.directory, user.id, unread).filter(
        chat => !query || conversationName(chat, data.directory, user.id).toLowerCase().includes(query),
      )
    : [];

  return (
    <Screen title="Chats" above={workspace.name}>
      <ConnectionBanner />
      <View style={styles.search}>
        <TextInput
          accessibilityLabel="Search conversations"
          placeholder="Search"
          placeholderTextColor={t.muted}
          value={search}
          onChangeText={setSearch}
          style={[styles.input, { backgroundColor: t.soft, color: t.text }]}
        />
      </View>
      <FilterChips value={filter} onChange={setFilter} />
      <FlatList
        data={chats}
        keyExtractor={chat => chat.id}
        refreshControl={<RefreshControl refreshing={loading && !!data} onRefresh={() => void refresh()} />}
        renderItem={({ item }) => (
          <ChatRow chat={item} directory={data!.directory} viewer={user.id} unread={unread[item.id] ?? 0} onOpen={open} />
        )}
        ListEmptyComponent={
          <View style={styles.empty}>
            {error ? (
              <ErrorText>{error}</ErrorText>
            ) : (
              <Text style={[typography.body, { color: t.muted }]}>
                {loading ? 'Loading conversations…' : query || filter !== 'all' ? 'Nothing matches.' : 'No conversations yet.'}
              </Text>
            )}
          </View>
        }
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  search: { paddingHorizontal: 16, paddingBottom: 8 },
  input: { minHeight: touchTarget, borderRadius: radius.button, paddingHorizontal: 14, fontSize: 16 },
  empty: { padding: 24, alignItems: 'center' },
});
