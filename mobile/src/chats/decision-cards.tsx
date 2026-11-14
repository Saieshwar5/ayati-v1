/**
 * An approval or a question inside the reply that asked it. The card opens
 * the sheet; nothing here decides or answers in one tap.
 */
import { questionStates, waitingLine, type ApprovalReference, type QuestionReference } from '@cuckoo/shared';
import { useRouter } from 'expo-router';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';

function Card({ kind, tone, title, line, action, onPress }: {
  kind: string; tone: string; title: string; line: string; action?: string; onPress?: () => void;
}) {
  const t = useTokens();
  return (
    <View style={[styles.card, { borderColor: t.borderPanel, backgroundColor: t.surface }]}>
      <Text style={[typography.small, { color: tone, fontWeight: '700' }]}>{kind.toUpperCase()}</Text>
      {title ? <Text style={[typography.body, { color: t.text }]}>{title}</Text> : null}
      <Text style={[typography.meta, { color: t.muted }]}>{line}</Text>
      {action && onPress ? (
        <Pressable accessibilityRole="button" accessibilityLabel={`${action}: ${title || kind}`} onPress={onPress} style={styles.action}>
          <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>{action} ›</Text>
        </Pressable>
      ) : null}
    </View>
  );
}

export function ApprovalCard({ reference }: { reference: ApprovalReference }) {
  const t = useTokens();
  const router = useRouter();
  const id = reference.id;
  const decides = reference.state === 'pending' && reference.may_decide;
  return (
    <Card
      kind="Approval"
      tone={t.danger}
      title=""
      line={waitingLine(reference)}
      action={id && !reference.restricted ? (decides ? 'Review and decide' : 'Details') : undefined}
      onPress={id ? () => router.push({ pathname: '/approval/[id]', params: { id } }) : undefined}
    />
  );
}

export function QuestionCards({ questions, viewer }: { questions: QuestionReference[]; viewer: string }) {
  const t = useTokens();
  const router = useRouter();
  const latest = questions[questions.length - 1];
  if (!latest) return null;
  const answering = latest.state === 'pending' && latest.respondent === viewer;
  return (
    <Card
      kind="Question"
      tone={t.working}
      title={latest.title}
      line={`${questionStates[latest.state] ?? 'Unavailable'} · For ${latest.respondent_name}${questions.length > 1 ? ` · ${questions.length - 1} earlier` : ''}`}
      action={answering ? 'Answer' : 'View'}
      onPress={() => router.push({ pathname: '/question/[id]', params: { id: latest.id } })}
    />
  );
}

const styles = StyleSheet.create({
  card: { borderWidth: 1, borderRadius: radius.button, padding: 12, gap: 4 },
  action: { minHeight: touchTarget - 8, justifyContent: 'center' },
});
