/**
 * Screen 9: an exact approval. The sheet shows the action and its exact
 * fields; Approve allows that one action exactly as shown, and the first
 * decision counts. It is only ever reached by opening it, never from a
 * notification or a single tap in a chat message.
 */
import { approvalStates, either, fieldEntries, type Approval, type Decision } from '@cuckoo/shared';
import * as Crypto from 'expo-crypto';
import * as Linking from 'expo-linking';
import { useCallback, useRef } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { origin } from '../config';
import { useWorkspace } from '../session/session-context';
import { radius, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { Button, ErrorText } from '../ui/controls';
import { useMutation } from '../ui/requests';
import { SheetFrame } from './sheet-frame';
import { useLiveRecord } from './use-live-record';

function Who({ approval }: { approval: Approval }) {
  const t = useTokens();
  if (approval.decided_by_name) {
    const what = approval.decision === 'approve' ? 'Approved' : approval.decision === 'request_changes' ? 'Changes requested' : 'Rejected';
    return <Text style={[typography.meta, { color: t.text }]}>{what} by {approval.decided_by_name}</Text>;
  }
  if (approval.state !== 'pending') return null;
  const names = (approval.deciders ?? []).map(person => person.name);
  const backup = approval.backup_at
    ? new Date(approval.backup_at * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
    : '';
  return (
    <View style={styles.gap}>
      <Text style={[typography.meta, { color: t.text }]}>
        {approval.can_decide ? 'Waiting for your decision' : names.length ? `Waiting for ${either(names)}` : 'Waiting for an admin who can see this request'}
      </Text>
      {approval.needs_other ? <Text style={[typography.small, { color: t.muted }]}>Someone other than the person who asked must approve.</Text> : null}
      {names.length && backup ? <Text style={[typography.small, { color: t.muted }]}>If nobody decides by {backup}, an admin can.</Text> : null}
    </View>
  );
}

export function ApprovalReview({ id }: { id: string }) {
  const t = useTokens();
  const { api, workspace } = useWorkspace();
  const { data, error, refresh } = useLiveRecord(useCallback((signal: AbortSignal) => api.approval(id, signal), [api, id]), 3000);
  const mutation = useMutation();
  // One request id per decision: a retry after an unclear failure can't decide twice.
  const requests = useRef(new Map<Decision, string>());
  const decide = (decision: Decision) =>
    mutation.run(async () => {
      const request = requests.current.get(decision) ?? Crypto.randomUUID();
      requests.current.set(decision, request);
      await api.decide(id, decision, request);
      await refresh();
    });
  const pending = data?.state === 'pending' && data.can_decide;
  return (
    <SheetFrame
      title="Approve an action"
      footer={
        pending ? (
          <>
            <ErrorText>{mutation.error}</ErrorText>
            <View style={styles.buttons}>
              <Button label="Decline" kind="outline" disabled={mutation.busy} onPress={() => void decide('reject')} style={styles.button} />
              <Button label="Approve" busy={mutation.busy} onPress={() => void decide('approve')} style={styles.button} />
            </View>
            <Pressable accessibilityRole="button" disabled={mutation.busy} onPress={() => void decide('request_changes')}>
              <Text style={[typography.meta, { color: t.text, textAlign: 'center', fontWeight: '600' }]}>Request changes instead</Text>
            </Pressable>
          </>
        ) : undefined
      }
    >
      {!data ? (
        error ? <ErrorText>{error}</ErrorText> : <Text style={[typography.body, { color: t.muted }]}>Loading the exact request…</Text>
      ) : (
        <>
          <View style={styles.gap}>
            <Text style={[typography.header, { color: t.text }]}>{data.title}</Text>
            <Text style={[typography.meta, { color: t.muted }]}>{data.agent_name} · {data.account_name}</Text>
            {data.conversation_name ? (
              <Text style={[typography.meta, { color: t.muted }]}>
                {data.conversation_name}
                {data.requester_name ? ` · Requested by ${data.requester_name}` : ''}
              </Text>
            ) : null}
          </View>
          {data.state !== 'pending' ? (
            <Text accessibilityRole="alert" style={[typography.body, { color: t.text, fontWeight: '600' }]}>
              {data.decision === 'request_changes' ? 'Changes requested' : (approvalStates[data.state] ?? data.state)}
            </Text>
          ) : null}
          <Who approval={data} />
          {data.guard?.summary ? (
            <View style={[styles.note, { borderColor: data.guard.effect === 'block' ? t.danger : t.warning }]}>
              <Text style={[typography.meta, { color: t.text, fontWeight: '600' }]}>
                {data.guard.effect === 'block' ? 'Blocked by a company rule' : 'Held by a company rule'}
              </Text>
              <Text style={[typography.meta, { color: t.text }]}>{data.guard.summary}</Text>
            </View>
          ) : null}
          <View accessibilityLabel="Exact details" style={[styles.table, { borderColor: t.borderPanel }]}>
            {fieldEntries(data.arguments).map(([field, value]) => (
              <View key={field} style={[styles.row, { borderBottomColor: t.border }]}>
                <Text style={[typography.meta, styles.label, { color: t.muted }]}>{field}</Text>
                <Text selectable style={[typography.meta, styles.value, { color: t.text }]}>{value}</Text>
              </View>
            ))}
          </View>
          {pending ? (
            <Text style={[typography.meta, { color: t.muted }]}>Approving allows this one action exactly as shown. The first decision counts.</Text>
          ) : null}
          {data.decision === 'request_changes' ? (
            <Text style={[typography.meta, { color: t.muted }]}>Ask for a revised proposal in the conversation. These fields were not approved.</Text>
          ) : null}
          {data.state === 'invalidated' ? (
            <Text style={[typography.meta, { color: t.muted }]}>Permissions or routing changed. The agent needs to ask again.</Text>
          ) : null}
          {data.state === 'uncertain' ? (
            <Button label="Check what happened on the web" kind="outline" onPress={() => void Linking.openURL(`${origin}/w/${workspace.id}/`)} />
          ) : null}
        </>
      )}
    </SheetFrame>
  );
}

const styles = StyleSheet.create({
  gap: { gap: 4 },
  buttons: { flexDirection: 'row', gap: 10 },
  button: { flex: 1 },
  note: { borderWidth: 1, borderRadius: radius.button, padding: 12, gap: 4 },
  table: { borderWidth: 1, borderRadius: radius.card - 2, paddingHorizontal: 12 },
  row: { flexDirection: 'row', gap: 12, paddingVertical: 9, borderBottomWidth: StyleSheet.hairlineWidth },
  label: { width: '38%' },
  value: { flex: 1, textAlign: 'right' },
});
