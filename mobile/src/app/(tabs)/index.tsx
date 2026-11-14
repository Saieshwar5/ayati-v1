/**
 * Screen 5: Home. A read-and-react page: greeting and business clock, what
 * needs this person, then their saved panels in web order (arranged on the web).
 */
import type { AttentionItem } from '@cuckoo/shared';
import * as Linking from 'expo-linking';
import { useRouter } from 'expo-router';
import { useCallback } from 'react';
import { Pressable, RefreshControl, ScrollView, StyleSheet, Text, View } from 'react-native';
import { origin } from '../../config';
import { BusinessClock } from '../../home/business-clock';
import { PanelCard } from '../../home/panel-card';
import { usePanels } from '../../home/use-panels';
import { AttentionCard } from '../../needs-me/attention-card';
import { attentionTarget } from '../../needs-me/attention-target';
import { useNeedsYou } from '../../needs-me/needs-you-context';
import { useWorkspace } from '../../session/session-context';
import { type as typography } from '../../theme/tokens';
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
  const { data, error, loading, refresh } = useNeedsYou();
  const panels = usePanels();
  const openPanel = useCallback((id: string) => router.push({ pathname: '/panel/[id]', params: { id } }), [router]);
  const open = useCallback((item: AttentionItem) => router.push(attentionTarget(item)), [router]);
  const first = (user.display_name ?? '').split(' ')[0];
  const waiting = data?.items ?? [];
  return (
    <Screen>
      <ScrollView
        contentContainerStyle={styles.page}
        refreshControl={
          <RefreshControl
            refreshing={(loading && !!data) || (panels.loading && !!panels.data)}
            onRefresh={() => void Promise.all([refresh(), panels.refresh()])}
          />
        }
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

        {panels.data?.panels.length ? (
          <View style={styles.section}>
            <Text accessibilityRole="header" style={[typography.header, { color: t.text }]}>Your panels</Text>
            {panels.data.panels.map(panel => (
              <PanelCard key={panel.id} panel={panel} onOpen={openPanel} />
            ))}
          </View>
        ) : null}
        <ErrorText>{panels.error}</ErrorText>
        <Pressable accessibilityRole="link" style={styles.web} onPress={() => void Linking.openURL(`${origin}/w/${workspace.id}/`)}>
          <Text style={[typography.meta, { color: t.muted }]}>
            {panels.data?.panels.length ? 'Arrange your panels on the web ›' : 'Add panels to your Home on the web ›'}
          </Text>
        </Pressable>
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  page: { padding: 16, gap: 12 },
  section: { gap: 10, marginTop: 12 },
  sectionHead: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  web: { paddingVertical: 12 },
});
