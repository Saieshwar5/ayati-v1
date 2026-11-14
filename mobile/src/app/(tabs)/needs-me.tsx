/** Screen 12: Needs me. Approving and answering open the conversation until M3 adds the sheets. */
import type { AttentionItem, AttentionView } from '@cuckoo/shared';
import { useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { FlatList, Pressable, RefreshControl, StyleSheet, Text, View } from 'react-native';
import { AttentionCard } from '../../needs-me/attention-card';
import { useNeedsYou } from '../../needs-me/needs-you-context';
import { useAttention } from '../../needs-me/use-attention';
import { useWorkspace } from '../../session/session-context';
import { radius, type as typography } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { ErrorText } from '../../ui/controls';
import { Screen } from '../../ui/screen';

export default function NeedsMe() {
  const t = useTokens();
  const router = useRouter();
  const { workspace } = useWorkspace();
  const [view, setView] = useState<AttentionView>('needs_you');
  const waiting = useNeedsYou();
  const recent = useAttention('recent', view === 'recent');
  const { data, error, loading, refresh } = view === 'needs_you' ? waiting : recent;
  const open = useCallback((item: AttentionItem) => router.push({ pathname: '/chat/[id]', params: { id: item.chat } }), [router]);
  const segments: { value: AttentionView; label: string }[] = [
    { value: 'needs_you', label: 'Waiting' },
    { value: 'recent', label: 'Done recently' },
  ];
  return (
    <Screen title="Needs me" above={workspace.name}>
      <View accessibilityRole="tablist" style={[styles.segments, { backgroundColor: t.soft }]}>
        {segments.map(segment => (
          <Pressable
            key={segment.value}
            accessibilityRole="tab"
            accessibilityState={{ selected: view === segment.value }}
            onPress={() => setView(segment.value)}
            style={[styles.segment, view === segment.value && { backgroundColor: t.surface, borderColor: t.border }]}
          >
            <Text style={{ color: t.text, fontWeight: view === segment.value ? '600' : '400' }}>
              {segment.label}
              {segment.value === 'needs_you' && view === 'needs_you' && data?.items.length ? ` · ${data.items.length}` : ''}
            </Text>
          </Pressable>
        ))}
      </View>
      <FlatList
        data={data?.items ?? []}
        keyExtractor={item => `${item.kind}/${item.id}/${item.version}`}
        contentContainerStyle={styles.list}
        refreshControl={<RefreshControl refreshing={loading && !!data} onRefresh={() => void refresh()} />}
        renderItem={({ item }) => <AttentionCard item={item} onOpen={open} />}
        ListEmptyComponent={
          error ? (
            <ErrorText>{error}</ErrorText>
          ) : (
            <Text style={[typography.body, { color: t.muted, textAlign: 'center', padding: 24 }]}>
              {loading ? 'Loading…' : view === 'needs_you' ? 'Nothing is waiting for you.' : 'Nothing finished recently.'}
            </Text>
          )
        }
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  segments: { flexDirection: 'row', marginHorizontal: 16, marginBottom: 12, borderRadius: radius.button, padding: 3 },
  segment: { flex: 1, minHeight: 38, alignItems: 'center', justifyContent: 'center', borderRadius: radius.button - 3, borderWidth: 1, borderColor: 'transparent' },
  list: { paddingHorizontal: 16, gap: 12, paddingBottom: 24 },
});
