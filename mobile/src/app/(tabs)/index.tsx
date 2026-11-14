/**
 * Screen 5: Home. A read-and-react page: greeting and business clock, then
 * what needs this person. Saved panels come in the next increment and are
 * arranged on the web.
 */
import type { AttentionItem } from '@cuckoo/shared';
import * as Linking from 'expo-linking';
import { useRouter } from 'expo-router';
import { useCallback } from 'react';
import { Pressable, RefreshControl, ScrollView, StyleSheet, Text, View } from 'react-native';
import { origin } from '../../config';
import { BusinessClock } from '../../home/business-clock';
import { AttentionCard } from '../../needs-me/attention-card';
import { useAttention } from '../../needs-me/use-attention';
import { useWorkspace } from '../../session/session-context';
import { radius, type as typography } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { ErrorText } from '../../ui/controls';
import { Screen } from '../../ui/screen';

function greeting(hour: number): string {
  if (hour < 12) return 'Good morning';
  if (hour < 18) return 'Good afternoon';
  return 'Good evening';
}

export default function Home() {
  const t = useTokens();
  const router = useRouter();
  const { user, workspace } = useWorkspace();
  const { data, error, loading, refresh } = useAttention('needs_you');
  const open = useCallback((item: AttentionItem) => router.push({ pathname: '/chat/[id]', params: { id: item.chat } }), [router]);
  const first = (user.display_name ?? '').split(' ')[0];
  const waiting = data?.items ?? [];
  return (
    <Screen>
      <ScrollView
        contentContainerStyle={styles.page}
        refreshControl={<RefreshControl refreshing={loading && !!data} onRefresh={() => void refresh()} />}
      >
        <Text style={[typography.meta, { color: t.muted }]}>
          {new Date().toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' })} · {workspace.name}
        </Text>
        <Text accessibilityRole="header" style={[typography.largeTitle, { color: t.text }]}>
          {greeting(new Date().getHours())}
          {first ? `, ${first}` : ''}
        </Text>
        <BusinessClock timezone={workspace.timezone} />

        {waiting.length ? (
          <View style={styles.section}>
            <View style={styles.sectionHead}>
              <Text accessibilityRole="header" style={[typography.header, { color: t.text }]}>
                Needs you · {waiting.length}
              </Text>
              <Pressable accessibilityRole="link" onPress={() => router.navigate('/needs-me')}>
                <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>See all</Text>
              </Pressable>
            </View>
            {waiting.slice(0, 2).map(item => (
              <AttentionCard key={`${item.kind}/${item.id}/${item.version}`} item={item} onOpen={open} />
            ))}
          </View>
        ) : null}
        <ErrorText>{error}</ErrorText>

        <View style={[styles.note, { borderColor: t.borderPanel }]}>
          <Text style={[typography.header, { color: t.text }]}>Your panels</Text>
          <Text style={[typography.meta, { color: t.muted }]}>
            Saved Home panels arrive in the app soon. Until then, see and arrange them on the web.
          </Text>
          <Pressable accessibilityRole="link" onPress={() => void Linking.openURL(`${origin}/w/${workspace.id}/`)}>
            <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>Open Cuckoo on the web ›</Text>
          </Pressable>
        </View>
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  page: { padding: 16, gap: 12 },
  section: { gap: 10, marginTop: 12 },
  sectionHead: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  note: { borderWidth: 1, borderRadius: radius.card, padding: 16, gap: 6, marginTop: 12 },
});
