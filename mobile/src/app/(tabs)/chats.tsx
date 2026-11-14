/**
 * Screen 7: Chats, with search and filters. On a tablet (768 pt and wider) the
 * open conversation sits beside the list, as on the web (D253).
 */
import { conversationName, filterConversations, type ChatFilter } from '@cuckoo/shared';
import { useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { FlatList, RefreshControl, StyleSheet, Text, TextInput, View, useWindowDimensions } from 'react-native';
import { ChatRow } from '../../chats/chat-row';
import { ConversationView } from '../../chats/conversation-view';
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
  const wide = useWindowDimensions().width >= 768;
  const [selected, setSelected] = useState<string>();
  const open = useCallback(
    (id: string) => (wide ? setSelected(id) : router.push({ pathname: '/chat/[id]', params: { id } })),
    [router, wide],
  );

  const query = search.trim().toLowerCase();
  const chats = data
    ? filterConversations(data.chats, filter, data.directory, user.id, unread).filter(
        chat => !query || conversationName(chat, data.directory, user.id).toLowerCase().includes(query),
      )
    : [];

  const list = (
    <>
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
          <ChatRow chat={item} directory={data!.directory} viewer={user.id} unread={unread[item.id] ?? 0} onOpen={open} workspace={workspace.id} />
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
    </>
  );

  if (!wide) {
    return (
      <Screen title="Chats" above={workspace.name}>
        {list}
      </Screen>
    );
  }
  return (
    <Screen>
      <View style={styles.panes}>
        <View style={[styles.listPane, { borderRightColor: t.border }]}>
          <View style={styles.paneTitle}>
            <Text style={[typography.meta, { color: t.muted }]}>{workspace.name}</Text>
            <Text accessibilityRole="header" style={[typography.largeTitle, { color: t.text }]}>Chats</Text>
          </View>
          {list}
        </View>
        <View style={styles.conversationPane}>
          {selected ? (
            <ConversationView key={selected} id={selected} />
          ) : (
            <View style={styles.empty}>
              <Text style={[typography.body, { color: t.muted }]}>Choose a conversation.</Text>
            </View>
          )}
        </View>
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  search: { paddingHorizontal: 16, paddingBottom: 8 },
  input: { minHeight: touchTarget, borderRadius: radius.button, paddingHorizontal: 14, fontSize: 16 },
  empty: { padding: 24, alignItems: 'center' },
  panes: { flex: 1, flexDirection: 'row' },
  listPane: { width: 360, borderRightWidth: StyleSheet.hairlineWidth },
  paneTitle: { paddingHorizontal: 16, paddingTop: 8, paddingBottom: 12 },
  conversationPane: { flex: 1 },
});
