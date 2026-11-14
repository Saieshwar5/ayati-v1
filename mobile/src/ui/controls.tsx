/** Buttons, tags and messages shared by every screen, drawn from the tokens. */
import type { ReactNode } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Text, View, type ViewStyle } from 'react-native';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';

type ButtonProps = {
  label: string;
  onPress: () => void;
  kind?: 'primary' | 'outline' | 'danger';
  busy?: boolean;
  disabled?: boolean;
  style?: ViewStyle;
  accessibilityHint?: string;
};

export function Button({ label, onPress, kind = 'primary', busy, disabled, style, accessibilityHint }: ButtonProps) {
  const t = useTokens();
  const off = disabled || busy;
  const filled = kind === 'primary';
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={accessibilityHint}
      accessibilityState={{ disabled: !!off, busy: !!busy }}
      disabled={off}
      onPress={onPress}
      style={({ pressed }) => [
        styles.button,
        {
          backgroundColor: filled ? t.primary : t.surface,
          borderColor: kind === 'danger' ? t.danger : filled ? t.primary : t.borderControl,
          opacity: off ? 0.55 : pressed ? 0.8 : 1,
        },
        style,
      ]}
    >
      {busy ? (
        <ActivityIndicator color={filled ? t.onPrimary : t.text} />
      ) : (
        <Text style={[styles.label, { color: filled ? t.onPrimary : kind === 'danger' ? t.danger : t.text }]}>{label}</Text>
      )}
    </Pressable>
  );
}

/** "AI" after an agent's name: the only thing that tells an agent from a person. */
export function AiTag() {
  const t = useTokens();
  return (
    <Text accessibilityLabel="AI agent" style={[styles.tag, { color: t.muted, borderColor: t.borderPanel }]}>
      AI
    </Text>
  );
}

export function ErrorText({ children }: { children: ReactNode }) {
  const t = useTokens();
  if (!children) return null;
  return (
    <Text accessibilityRole="alert" style={[typography.meta, { color: t.danger }]}>
      {children}
    </Text>
  );
}

/** A round face with initials; agents' bundled robots and photos come next (D253). */
export function Face({ name, size = 40, agent }: { name: string; size?: number; agent?: boolean }) {
  const t = useTokens();
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map(part => part[0]?.toUpperCase())
    .join('');
  return (
    <View
      importantForAccessibility="no-hide-descendants"
      accessibilityElementsHidden
      style={{
        width: size,
        height: size,
        borderRadius: size / 2,
        backgroundColor: agent ? t.selection : t.soft,
        borderColor: t.border,
        borderWidth: StyleSheet.hairlineWidth,
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      <Text style={{ color: t.text, fontWeight: '600', fontSize: size * 0.38 }}>{initials || '?'}</Text>
    </View>
  );
}

/** A count that means something is waiting for this person (red), or muted (grey). */
export function Count({ value, quiet }: { value: number; quiet?: boolean }) {
  const t = useTokens();
  if (!value) return null;
  return (
    <View style={[styles.count, { backgroundColor: quiet ? t.selection : t.danger }]}>
      <Text style={{ color: quiet ? t.text : t.onPrimary, fontSize: 12, fontWeight: '700' }}>
        {value > 99 ? '99+' : value}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  button: {
    minHeight: touchTarget,
    borderRadius: radius.button,
    borderWidth: 1,
    paddingHorizontal: 18,
    alignItems: 'center',
    justifyContent: 'center',
  },
  label: { fontSize: 16, fontWeight: '600' },
  tag: {
    fontSize: 11,
    fontWeight: '600',
    borderWidth: 1,
    borderRadius: 5,
    paddingHorizontal: 4,
    marginLeft: 6,
    overflow: 'hidden',
  },
  count: { minWidth: 22, height: 22, borderRadius: 11, paddingHorizontal: 6, alignItems: 'center', justifyContent: 'center' },
});
