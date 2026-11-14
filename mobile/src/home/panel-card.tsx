/** A saved panel on Home: title, its first section in short form, who keeps it and when it changed. */
import type { ShownPanel } from '@cuckoo/shared';
import { memo } from 'react';
import { Pressable, StyleSheet, Text } from 'react-native';
import { shortTime } from '../chats/time';
import { radius, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { SectionView } from './section-view';

export function keeper(panel: ShownPanel): string {
  if (panel.shared && panel.owner_name) return `Shared by ${panel.owner_name}`;
  return panel.agent_name ?? 'You';
}

export const PanelCard = memo(function PanelCard({ panel, onOpen }: { panel: ShownPanel; onOpen: (id: string) => void }) {
  const t = useTokens();
  const first = panel.definition.sections[0];
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${panel.definition.title}. Open panel`}
      onPress={() => onOpen(panel.id)}
      style={({ pressed }) => [styles.card, { borderColor: t.borderPanel, backgroundColor: pressed ? t.soft : t.surface }]}
    >
      <Text style={[typography.header, { color: t.text }]}>{panel.definition.title}</Text>
      {first ? <SectionView section={first} compact /> : null}
      <Text style={[typography.small, { color: t.muted }]}>
        {keeper(panel)}
        {panel.saved_ms ? ` · ${shortTime(panel.saved_ms / 1000)}` : ''}
      </Text>
    </Pressable>
  );
});

const styles = StyleSheet.create({
  card: { borderWidth: 1, borderRadius: radius.card, padding: 16, gap: 10 },
});
