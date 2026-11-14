/** The inputs of an agent's question: choice rows, and a form's fields. */
import type { QuestionField, QuestionOption } from '@cuckoo/shared';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';

export function ChoiceRow({ label, selected, multiple, suggested, onPress }: {
  label: string; selected: boolean; multiple: boolean; suggested?: boolean; onPress: () => void;
}) {
  const t = useTokens();
  return (
    <Pressable
      accessibilityRole={multiple ? 'checkbox' : 'radio'}
      accessibilityState={{ checked: selected }}
      accessibilityLabel={suggested ? `${label}, suggested` : label}
      onPress={onPress}
      style={[styles.choice, { borderColor: selected ? t.text : t.borderControl, backgroundColor: selected ? t.selection : t.surface }]}
    >
      <View style={[styles.mark, { borderColor: t.borderControl, borderRadius: multiple ? 5 : 11, backgroundColor: selected ? t.text : 'transparent' }]} />
      <Text style={[typography.body, { color: t.text, flex: 1 }]}>{label}</Text>
      {suggested ? <Text style={[typography.small, { color: t.working, fontWeight: '600' }]}>Suggested</Text> : null}
    </Pressable>
  );
}

export function Options({ options, selected, multiple, suggested, toggle }: {
  options: QuestionOption[]; selected: string[]; multiple: boolean; suggested?: string; toggle: (id: string) => void;
}) {
  return (
    <View accessibilityRole={multiple ? undefined : 'radiogroup'} style={styles.list}>
      {options.map(option => (
        <ChoiceRow key={option.id} label={option.label} selected={selected.includes(option.id)} multiple={multiple}
          suggested={suggested === option.id} onPress={() => toggle(option.id)} />
      ))}
    </View>
  );
}

export function FormField({ field, value, change }: { field: QuestionField; value: string; change: (value: string) => void }) {
  const t = useTokens();
  const label = `${field.label}${field.required ? ' (required)' : ''}`;
  const picks: QuestionOption[] | undefined =
    field.kind === 'boolean' ? [{ id: 'true', label: 'Yes' }, { id: 'false', label: 'No' }] : field.kind === 'choice' ? field.options : undefined;
  return (
    <View style={styles.field}>
      <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>{label}</Text>
      {picks ? (
        <Options options={picks} selected={value ? [value] : []} multiple={false} toggle={id => change(id === value ? '' : id)} />
      ) : (
        <TextInput
          accessibilityLabel={label}
          value={value}
          onChangeText={change}
          multiline={field.kind === 'multiline'}
          keyboardType={field.kind === 'number' ? 'decimal-pad' : 'default'}
          placeholder={field.kind === 'date' ? 'YYYY-MM-DD' : undefined}
          placeholderTextColor={t.muted}
          maxLength={field.max_length ?? (field.kind === 'multiline' ? 4000 : 1000)}
          style={[typography.body, styles.input, { color: t.text, borderColor: t.borderControl, minHeight: field.kind === 'multiline' ? 88 : touchTarget }]}
        />
      )}
      {field.description || field.suggested !== undefined ? (
        <Text style={[typography.small, { color: t.muted }]}>
          {field.description}
          {field.suggested !== undefined ? `${field.description ? ' ' : ''}The agent's suggestion is filled in; you can change it.` : ''}
        </Text>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  list: { gap: 8 },
  choice: { minHeight: touchTarget + 8, flexDirection: 'row', alignItems: 'center', gap: 12, borderWidth: 1, borderRadius: radius.button, paddingHorizontal: 14, paddingVertical: 10 },
  mark: { width: 22, height: 22, borderWidth: 1.5 },
  field: { gap: 6 },
  input: { borderWidth: 1, borderRadius: radius.button, paddingHorizontal: 12, paddingVertical: 10, textAlignVertical: 'top' },
});
