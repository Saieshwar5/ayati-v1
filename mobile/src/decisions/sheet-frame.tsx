/** The top of a sheet: a grabber, the title and Close. */
import { useRouter } from 'expo-router';
import type { ReactNode } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';

export function SheetFrame({ title, children, footer }: { title: string; children: ReactNode; footer?: ReactNode }) {
  const t = useTokens();
  const router = useRouter();
  return (
    <SafeAreaView edges={['bottom', 'left', 'right']} style={[styles.page, { backgroundColor: t.raised }]}>
      <View style={[styles.grabber, { backgroundColor: t.borderPanel }]} />
      <View style={styles.header}>
        <Text accessibilityRole="header" style={[typography.header, { color: t.text, flex: 1 }]}>{title}</Text>
        <Pressable accessibilityRole="button" accessibilityLabel="Close" onPress={() => router.back()} style={styles.close}>
          <Text style={[typography.body, { color: t.muted }]}>Close</Text>
        </Pressable>
      </View>
      <ScrollView contentContainerStyle={styles.body} keyboardShouldPersistTaps="handled">
        {children}
      </ScrollView>
      {footer ? <View style={[styles.footer, { borderTopColor: t.border }]}>{footer}</View> : null}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, borderTopLeftRadius: radius.sheet, borderTopRightRadius: radius.sheet },
  grabber: { alignSelf: 'center', width: 36, height: 5, borderRadius: 3, marginTop: 8 },
  header: { flexDirection: 'row', alignItems: 'center', paddingHorizontal: 16, paddingTop: 8 },
  close: { minHeight: touchTarget, minWidth: touchTarget, alignItems: 'flex-end', justifyContent: 'center' },
  body: { padding: 16, gap: 14 },
  footer: { padding: 16, gap: 10, borderTopWidth: StyleSheet.hairlineWidth },
});
