/**
 * The floating composer. Sending keeps one id per payload, so a retry after an
 * unclear failure can't post twice; a failed send keeps the draft.
 */
import type { Message, Submission } from '@cuckoo/shared';
import * as Crypto from 'expo-crypto';
import { useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { errorMessage } from '../ui/requests';

type Props = {
  placeholder: string;
  disabled?: boolean;
  /** The agents this message asks: the direct conversation's agent, if any. */
  agents: string[];
  send: (submission: Submission) => Promise<{ message?: Message }>;
  sent: (message: Message) => void;
};

export function Composer({ placeholder, disabled, agents, send, sent }: Props) {
  const t = useTokens();
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const pending = useRef(false);
  const attempt = useRef<Submission | undefined>(undefined);

  const submit = async () => {
    const content = draft.trim();
    if (pending.current || !content || disabled) return;
    pending.current = true;
    setBusy(true);
    setError('');
    const asked = agents.length ? agents : undefined;
    // The same text to the same agents keeps its id across retries.
    if (!attempt.current || attempt.current.content !== content || attempt.current.agents?.join() !== asked?.join()) {
      attempt.current = { id: Crypto.randomUUID(), content, agents: asked, attachments: [] };
    }
    try {
      const result = await send(attempt.current);
      setDraft('');
      attempt.current = undefined;
      if (result.message) sent(result.message);
    } catch (failure) {
      setError(`Not sent · ${errorMessage(failure)}`);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  return (
    <View style={styles.wrap}>
      {error ? (
        <Pressable accessibilityRole="button" accessibilityLabel="Retry sending" onPress={() => void submit()}>
          <Text accessibilityRole="alert" style={[typography.meta, styles.error, { color: t.danger }]}>
            {error} · Retry
          </Text>
        </Pressable>
      ) : null}
      <View style={[styles.composer, { backgroundColor: t.raised, borderColor: t.borderControl, shadowColor: t.text }]}>
        <TextInput
          accessibilityLabel="Message"
          placeholder={placeholder}
          placeholderTextColor={t.muted}
          value={draft}
          onChangeText={setDraft}
          editable={!disabled}
          multiline
          maxLength={20000}
          style={[typography.body, styles.input, { color: t.text }]}
        />
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Send message"
          accessibilityState={{ disabled: busy || !draft.trim() || !!disabled, busy }}
          disabled={busy || !draft.trim() || !!disabled}
          onPress={() => void submit()}
          style={[styles.send, { backgroundColor: draft.trim() && !busy ? t.primary : t.selection }]}
        >
          <Text style={{ color: draft.trim() && !busy ? t.onPrimary : t.muted, fontWeight: '700' }}>↑</Text>
        </Pressable>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { paddingHorizontal: 12, paddingTop: 6, paddingBottom: 8, gap: 4 },
  error: { paddingHorizontal: 6 },
  composer: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    borderWidth: 1,
    borderRadius: radius.bubble,
    paddingLeft: 14,
    paddingRight: 6,
    paddingVertical: 6,
    shadowOpacity: 0.08,
    shadowRadius: 12,
    shadowOffset: { width: 0, height: 4 },
    elevation: 2,
  },
  input: { flex: 1, maxHeight: 140, paddingVertical: 8 },
  send: { width: touchTarget - 4, height: touchTarget - 4, borderRadius: (touchTarget - 4) / 2, alignItems: 'center', justifyContent: 'center' },
});
