/** A tab's page: large title on the white surface, inside the safe area. */
import type { ReactNode } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';

export function Screen({ title, above, children }: { title?: string; above?: string; children: ReactNode }) {
  const t = useTokens();
  return (
    <SafeAreaView edges={['top', 'left', 'right']} style={[styles.page, { backgroundColor: t.surface }]}>
      {title ? (
        <View style={styles.header}>
          {above ? <Text style={[typography.meta, { color: t.muted }]}>{above}</Text> : null}
          <Text accessibilityRole="header" style={[typography.largeTitle, { color: t.text }]}>
            {title}
          </Text>
        </View>
      ) : null}
      {children}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1 },
  header: { paddingHorizontal: 16, paddingTop: 8, paddingBottom: 12 },
});
