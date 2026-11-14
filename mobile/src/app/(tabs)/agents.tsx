/** Screen 14: every agent this person may use. Creating and changing agents stays on the web. */
import type { Participant } from '@cuckoo/shared';
import { useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { FlatList, Pressable, RefreshControl, StyleSheet, Text, TextInput, View } from 'react-native';
import { useOpenAgent } from '../../agents/use-open-agent';
import { useWorkspace } from '../../session/session-context';
import { radius, touchTarget, type as typography } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { AiTag, ErrorText, Face } from '../../ui/controls';
import { useResource } from '../../ui/requests';
import { Screen } from '../../ui/screen';

export default function Agents() {
  const t = useTokens();
  const router = useRouter();
  const { api, workspace } = useWorkspace();
  const [search, setSearch] = useState('');
  const { data, error, loading, refresh } = useResource(
    useCallback(
      async (signal: AbortSignal) => {
        const [directory, chats] = await Promise.all([api.directory(signal), api.chats(signal)]);
        return { agents: directory.agents, chats: chats.chats };
      },
      [api],
    ),
  );
  const opener = useOpenAgent(data?.chats, chat => router.push({ pathname: '/chat/[id]', params: { id: chat } }));
  const query = search.trim().toLowerCase();
  const agents = (data?.agents ?? []).filter(
    agent => !query || `${agent.name} ${agent.title ?? ''} ${agent.about ?? ''}`.toLowerCase().includes(query),
  );
  const row = ({ item }: { item: Participant }) => (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`Message ${item.name}, AI agent${item.title ? `, ${item.title}` : ''}`}
      disabled={opener.busy}
      onPress={() => void opener.open(item)}
      style={({ pressed }) => [styles.row, { borderBottomColor: t.border, backgroundColor: pressed ? t.soft : t.surface }]}
    >
      <Face name={item.name} agent />
      <View style={styles.text}>
        <View style={styles.title}>
          <Text numberOfLines={1} style={[typography.header, { color: t.text, flexShrink: 1 }]}>{item.name}</Text>
          <AiTag />
        </View>
        {item.title || item.about ? (
          <Text numberOfLines={2} style={[typography.meta, { color: t.muted }]}>
            {[item.title, item.about].filter(Boolean).join(' · ')}
          </Text>
        ) : null}
      </View>
    </Pressable>
  );
  return (
    <Screen title="Agents" above={workspace.name}>
      <View style={styles.search}>
        <TextInput
          accessibilityLabel="Search agents"
          placeholder="Search"
          placeholderTextColor={t.muted}
          value={search}
          onChangeText={setSearch}
          style={[styles.input, { backgroundColor: t.soft, color: t.text }]}
        />
      </View>
      <ErrorText>{opener.error}</ErrorText>
      <FlatList
        data={agents}
        keyExtractor={agent => agent.id}
        renderItem={row}
        refreshControl={<RefreshControl refreshing={loading && !!data} onRefresh={() => void refresh()} />}
        ListEmptyComponent={
          <View style={styles.empty}>
            {error ? <ErrorText>{error}</ErrorText> : <Text style={[typography.body, { color: t.muted }]}>{loading ? 'Loading agents…' : 'No agents match.'}</Text>}
          </View>
        }
        ListFooterComponent={
          <Text style={[typography.meta, styles.footer, { color: t.muted }]}>Create or change agents in Cuckoo on the web.</Text>
        }
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  search: { paddingHorizontal: 16, paddingBottom: 8 },
  input: { minHeight: touchTarget, borderRadius: radius.button, paddingHorizontal: 14, fontSize: 16 },
  row: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 16, paddingVertical: 12, minHeight: 68, borderBottomWidth: StyleSheet.hairlineWidth },
  text: { flex: 1, gap: 2 },
  title: { flexDirection: 'row', alignItems: 'center' },
  empty: { padding: 24, alignItems: 'center' },
  footer: { padding: 16, textAlign: 'center' },
});
