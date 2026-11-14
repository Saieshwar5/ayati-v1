/** Screen 6: one panel, full screen and read-only, with its numbers and where it comes from. */
import { chartTable, type ChartSection } from '@cuckoo/shared';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { shortTime } from '../../chats/time';
import { keeper } from '../../home/panel-card';
import { SectionView } from '../../home/section-view';
import { usePanels } from '../../home/use-panels';
import { radius, touchTarget, type as typography } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { Button, ErrorText } from '../../ui/controls';

function every(seconds: number | null | undefined): string {
  if (!seconds) return 'when asked';
  if (seconds < 3600) return `every ${Math.round(seconds / 60)} minutes`;
  if (seconds < 86_400) return `every ${Math.round(seconds / 3600)} hours`;
  return `every ${Math.round(seconds / 86_400)} days`;
}

/** The accessible values table under a chart. */
function Numbers({ section }: { section: ChartSection }) {
  const t = useTokens();
  const [open, setOpen] = useState(false);
  const table = chartTable(section);
  return (
    <View style={styles.gap}>
      <Pressable accessibilityRole="button" accessibilityState={{ expanded: open }} onPress={() => setOpen(!open)} style={styles.toggle}>
        <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>{open ? 'Hide the numbers' : 'Show the numbers'}</Text>
      </Pressable>
      {open
        ? [table.columns, ...table.rows].map((row, r) => (
            <View key={r} style={[styles.row, { borderBottomColor: t.border }]}>
              {row.map((cell, c) => (
                <Text key={c} style={[typography.meta, styles.cell, { color: r ? t.text : t.muted, textAlign: c ? 'right' : 'left' }]}>
                  {cell}
                </Text>
              ))}
            </View>
          ))
        : null}
    </View>
  );
}

export default function PanelScreen() {
  const t = useTokens();
  const router = useRouter();
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, error, refresh } = usePanels();
  const panel = data?.panels.find(p => p.id === id);
  const chat = panel?.active?.chat;
  return (
    <SafeAreaView edges={['top', 'left', 'right']} style={[styles.page, { backgroundColor: t.surface }]}>
      <View style={[styles.header, { borderBottomColor: t.border }]}>
        <Pressable accessibilityRole="button" accessibilityLabel="Back to Home" onPress={() => router.back()} style={styles.back}>
          <Text style={{ color: t.text, fontSize: 24 }}>‹</Text>
        </Pressable>
        <Text numberOfLines={1} accessibilityRole="header" style={[typography.header, { color: t.text, flex: 1 }]}>
          {panel?.definition.title ?? 'Panel'}
        </Text>
      </View>
      <ScrollView contentContainerStyle={styles.body}>
        {!panel ? (
          error ? (
            <>
              <ErrorText>{error}</ErrorText>
              <Button label="Try again" kind="outline" onPress={() => void refresh()} />
            </>
          ) : (
            <Text style={[typography.body, { color: t.muted }]}>{data ? 'This panel is no longer on your Home.' : 'Loading…'}</Text>
          )
        ) : (
          <>
            {panel.definition.purpose ? <Text style={[typography.meta, { color: t.muted }]}>{panel.definition.purpose}</Text> : null}
            {panel.definition.sections.map((section, index) => (
              <View key={index} style={styles.gap}>
                <SectionView section={section} />
                {section.kind === 'chart' ? <Numbers section={section} /> : null}
              </View>
            ))}
            <View style={[styles.about, { borderColor: t.borderPanel }]}>
              <Text accessibilityRole="header" style={[typography.header, { color: t.text }]}>
                About this panel
              </Text>
              <Text style={[typography.meta, { color: t.text }]}>
                Kept by {keeper(panel)}, {every(panel.maintenance?.enabled ? panel.maintenance.interval_seconds : null)}
                {panel.maintenance?.from_source ? ', straight from its source' : ''}.
              </Text>
              {panel.saved_ms ? (
                <Text style={[typography.meta, { color: t.muted }]}>
                  Updated {shortTime(panel.saved_ms / 1000)}
                  {panel.maintenance?.status ? ` · ${panel.maintenance.status.replace(/_/g, ' ')}` : ''}
                </Text>
              ) : null}
              {panel.active?.what_changed ? (
                <Text style={[typography.meta, { color: t.muted }]}>{panel.active.what_changed}</Text>
              ) : null}
              {chat ? (
                <Button label="Ask about this" kind="outline" onPress={() => router.push({ pathname: '/chat/[id]', params: { id: chat } })} />
              ) : null}
            </View>
          </>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1 },
  header: { flexDirection: 'row', alignItems: 'center', paddingRight: 16, borderBottomWidth: StyleSheet.hairlineWidth, minHeight: 52 },
  back: { width: touchTarget, height: touchTarget, alignItems: 'center', justifyContent: 'center' },
  body: { padding: 16, gap: 20 },
  gap: { gap: 8 },
  toggle: { minHeight: touchTarget, justifyContent: 'center' },
  row: { flexDirection: 'row', borderBottomWidth: StyleSheet.hairlineWidth, paddingVertical: 6 },
  cell: { flex: 1 },
  about: { borderWidth: 1, borderRadius: radius.card, padding: 16, gap: 8 },
});
