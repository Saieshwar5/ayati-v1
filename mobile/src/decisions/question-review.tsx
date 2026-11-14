/**
 * Screen 13: answer an agent's question. Answering supplies facts; it approves
 * nothing. The same answer keeps one request id, so a retry can't answer twice.
 */
import {
  answerLabels, answerValues, draftProblem, questionStates, questionSupported, type Question, type QuestionDraft,
} from '@cuckoo/shared';
import * as Crypto from 'expo-crypto';
import { useCallback, useRef, useState } from 'react';
import { Text, TextInput, View, StyleSheet } from 'react-native';
import { useWorkspace } from '../session/session-context';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { Button, ErrorText } from '../ui/controls';
import { useMutation } from '../ui/requests';
import { ChoiceRow, FormField, Options } from './question-fields';
import { SheetFrame } from './sheet-frame';
import { useLiveRecord } from './use-live-record';

function initialDraft(question: Question): QuestionDraft {
  const fields = Object.fromEntries(
    question.specification.fields.filter(field => field.suggested !== undefined).map(field => [field.id, String(field.suggested)]),
  );
  return { choices: [], fields };
}

export function QuestionReview({ id }: { id: string }) {
  const t = useTokens();
  const { api } = useWorkspace();
  const { data, error, setData } = useLiveRecord(useCallback((signal: AbortSignal) => api.question(id, signal), [api, id]), 5000);
  const [draft, setDraft] = useState<QuestionDraft>();
  const mutation = useMutation();
  const attempt = useRef<{ payload: string; id: string } | undefined>(undefined);
  const current = data && (draft ?? initialDraft(data));
  const spec = data?.specification;
  const open = !!data && data.state === 'pending' && data.can_respond && !!spec && questionSupported(spec);
  const multiple = spec?.kind === 'multiple_choice';

  const respond = (decision: 'answer' | 'decline') =>
    mutation.run(async () => {
      if (!data || !current || !spec) return;
      const problem = decision === 'answer' ? draftProblem(spec, current) : undefined;
      if (problem) throw new Error(problem);
      const values = decision === 'answer' ? answerValues(spec, current) : {};
      const payload = JSON.stringify({ decision, values, revision: data.revision });
      if (attempt.current?.payload !== payload) attempt.current = { payload, id: Crypto.randomUUID() };
      const saved = await api.answer(id, { id: attempt.current.id, revision: data.revision, decision, values });
      setData({ ...data, ...saved, can_respond: false });
    });

  const toggle = (option: string) =>
    setDraft(previous => {
      const base = previous ?? initialDraft(data!);
      if (!multiple) return { ...base, choices: [option], other: undefined };
      const has = base.choices.includes(option);
      return { ...base, choices: has ? base.choices.filter(c => c !== option) : [...base.choices, option] };
    });

  return (
    <SheetFrame
      title={spec?.title ?? 'Agent question'}
      footer={
        open ? (
          <>
            <ErrorText>{mutation.error}</ErrorText>
            <Button label="Send answer" busy={mutation.busy} onPress={() => void respond('answer')} />
            <Button label="Decline question" kind="outline" disabled={mutation.busy} onPress={() => void respond('decline')} />
            <Text style={[typography.small, { color: t.muted, textAlign: 'center' }]}>Answering is not an approval.</Text>
          </>
        ) : undefined
      }
    >
      {!data || !spec || !current ? (
        error ? <ErrorText>{error}</ErrorText> : <Text style={[typography.body, { color: t.muted }]}>Loading question…</Text>
      ) : (
        <>
          <Text style={[typography.meta, { color: t.muted }]}>{questionStates[data.state] ?? 'Unavailable'} · For {data.respondent_name}</Text>
          {spec.description ? <Text style={[typography.body, { color: t.text }]}>{spec.description}</Text> : null}
          {!questionSupported(spec) ? (
            <Text style={[typography.body, { color: t.text }]}>{spec.fallback || 'Answer this question on the web.'}</Text>
          ) : open ? (
            spec.kind === 'form' ? (
              spec.fields.map(field => (
                <FormField key={field.id} field={field} value={current.fields[field.id] ?? ''}
                  change={value => setDraft({ ...current, fields: { ...current.fields, [field.id]: value } })} />
              ))
            ) : (
              <View style={styles.gap}>
                <Options options={spec.options} selected={current.choices} multiple={multiple} toggle={toggle} />
                {spec.allow_other ? (
                  <ChoiceRow label="Another answer" multiple={multiple} selected={current.other !== undefined}
                    onPress={() => setDraft({ ...current, other: current.other === undefined ? '' : undefined, choices: multiple ? current.choices : [] })} />
                ) : null}
                {current.other !== undefined ? (
                  <TextInput accessibilityLabel="Your other answer" value={current.other} maxLength={1000}
                    onChangeText={other => setDraft({ ...current, other })} placeholder="Your answer" placeholderTextColor={t.muted}
                    style={[typography.body, styles.input, { color: t.text, borderColor: t.borderControl }]} />
                ) : null}
              </View>
            )
          ) : data.state === 'answered' ? (
            answerLabels(data).map(answer => (
              <View key={answer.id} style={styles.gap}>
                <Text style={[typography.meta, { color: t.muted }]}>{answer.label}</Text>
                <Text style={[typography.body, { color: t.text }]}>{answer.value}</Text>
              </View>
            ))
          ) : (
            <Text style={[typography.body, { color: t.muted }]}>
              {data.state === 'pending' ? 'Only the person it asks can answer.' : data.state === 'declined' ? 'The question was declined.' : 'This question is no longer active.'}
            </Text>
          )}
        </>
      )}
    </SheetFrame>
  );
}

const styles = StyleSheet.create({
  gap: { gap: 8 },
  input: { borderWidth: 1, borderRadius: radius.button, paddingHorizontal: 12, minHeight: touchTarget },
});
