import type { ChatFilter } from '@cuckoo/shared';
import { Pressable, ScrollView, StyleSheet, Text } from 'react-native';
import { touchTarget } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';

const filters: { value: ChatFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'unread', label: 'Unread' },
  { value: 'agents', label: 'Agents' },
  { value: 'people', label: 'People' },
  { value: 'channels', label: 'Channels' },
];

export function FilterChips({ value, onChange }: { value: ChatFilter; onChange: (value: ChatFilter) => void }) {
  const t = useTokens();
  return (
    <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.row}>
      {filters.map(filter => {
        const selected = filter.value === value;
        return (
          <Pressable
            key={filter.value}
            accessibilityRole="tab"
            accessibilityState={{ selected }}
            onPress={() => onChange(filter.value)}
            style={[styles.chip, { backgroundColor: selected ? t.selection : t.surface, borderColor: selected ? t.borderControl : t.border }]}
          >
            <Text style={{ color: t.text, fontWeight: selected ? '600' : '400' }}>{filter.label}</Text>
          </Pressable>
        );
      })}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  row: { paddingHorizontal: 16, gap: 8, paddingBottom: 8 },
  chip: { minHeight: touchTarget - 8, borderRadius: 18, borderWidth: 1, paddingHorizontal: 14, justifyContent: 'center' },
});
