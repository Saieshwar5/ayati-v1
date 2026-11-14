/**
 * Screen 14: every agent this person may use; a row opens its profile. On a
 * tablet the profile sits beside the list (D253). Creating and changing
 * agents stays on the web.
 */
import type { Participant } from '@cuckoo/shared';
import { useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { FlatList, Pressable, RefreshControl, StyleSheet, Text, TextInput, View, useWindowDimensions } from 'react-native';
import { AgentProfileView } from '../../agents/agent-profile-view';
import { useAgents } from '../../agents/use-agents';
import { Face } from '../../faces/face';
import { useWorkspace } from '../../session/session-context';
import { radius, touchTarget, type as typography } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { AiTag, ErrorText } from '../../ui/controls';
import { Screen } from '../../ui/screen';

export default function Agents() {
  const t = useTokens();
  const router = useRouter();
  const { workspace } = useWorkspace();
  const wide = useWindowDimensions().width >= 768;
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<string>();
  const { data, error, loading, refresh } = useAgents();
  const openChat = useCallback((chat: string) => router.push({ pathname: '/chat/[id]', params: { id: chat } }), [router]);
  const openAgent = (agent: Participant) =>
    wide ? setSelected(agent.id) : router.push({ pathname: '/agent/[id]', params: { id: agent.id } });

  const query = search.trim().toLowerCase();
  const agents = (data?.directory.agents ?? []).filter(
    agent => !query || `${agent.name} ${agent.title ?? ''} ${agent.about ?? ''}`.toLowerCase().includes(query),
  );
  const row = ({ item }: { item: Participant }) => (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${item.name}, AI agent${item.title ? `, ${item.title}` : ''}. Open profile`}
      accessibilityState={{ selected: wide && selected === item.id }}
      onPress={() => openAgent(item)}
      style={({ pressed }) => [
        styles.row,
        { borderBottomColor: t.border, backgroundColor: pressed || (wide && selected === item.id) ? t.soft : t.surface },
      ]}
    >
      <Face name={item.name} agent={item} workspace={workspace.id} />
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
  const list = (
    <>
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
      <FlatList
        data={agents}
        keyExtractor={agent => agent.id}
        renderItem={row}
        refreshControl={<RefreshControl refreshing={loading && !!data} onRefresh={() => void refresh()} />}
        ListEmptyComponent={
          <View style={styles.empty}>
            {error ? (
              <ErrorText>{error}</ErrorText>
            ) : (
              <Text style={[typography.body, { color: t.muted }]}>{loading ? 'Loading agents…' : 'No agents match.'}</Text>
            )}
          </View>
        }
        ListFooterComponent={
          <Text style={[typography.meta, styles.footer, { color: t.muted }]}>Create or change agents in Cuckoo on the web.</Text>
        }
      />
    </>
  );

  if (!wide) {
    return (
      <Screen title="Agents" above={workspace.name}>
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
            <Text accessibilityRole="header" style={[typography.largeTitle, { color: t.text }]}>Agents</Text>
          </View>
          {list}
        </View>
        <View style={styles.detailPane}>
          {selected && data ? (
            <AgentProfileView key={selected} agentId={selected} directory={data.directory} chats={data.chats} opened={openChat} />
          ) : (
            <View style={styles.empty}>
              <Text style={[typography.body, { color: t.muted }]}>Choose an agent.</Text>
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
  row: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: 16, paddingVertical: 12, minHeight: 68, borderBottomWidth: StyleSheet.hairlineWidth },
  text: { flex: 1, gap: 2 },
  title: { flexDirection: 'row', alignItems: 'center' },
  empty: { padding: 24, alignItems: 'center' },
  footer: { padding: 16, textAlign: 'center' },
  panes: { flex: 1, flexDirection: 'row' },
  listPane: { width: 360, borderRightWidth: StyleSheet.hairlineWidth },
  paneTitle: { paddingHorizontal: 16, paddingTop: 8, paddingBottom: 12 },
  detailPane: { flex: 1 },
});
