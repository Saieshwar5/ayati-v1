/** Screen 2: choose a workspace. Invitations are accepted on the web or by email. */
import { FlatList, Pressable, StyleSheet, Text, View } from 'react-native';
import { useSession } from '../session/session-context';
import { touchTarget, type as typography, radius } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { Button, ErrorText } from '../ui/controls';
import { useMutation } from '../ui/requests';
import { Screen } from '../ui/screen';

export default function ChooseWorkspace() {
  const t = useTokens();
  const { state, chooseWorkspace, signOut } = useSession();
  const mutation = useMutation();
  if (state.status !== 'signed-in') return null;
  const workspaces = (state.session.workspaces ?? []).filter(w => !w.suspended);
  const invitations = state.session.invitations ?? [];
  return (
    <Screen title="Choose a workspace" above={state.session.user.email}>
      <FlatList
        data={workspaces}
        keyExtractor={w => w.id}
        contentContainerStyle={styles.list}
        ListEmptyComponent={
          <Text style={[typography.body, { color: t.muted }]}>
            You are not in a workspace yet. Ask your admin for an invitation.
          </Text>
        }
        renderItem={({ item }) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`Open ${item.name}`}
            accessibilityState={{ selected: state.workspace?.id === item.id }}
            disabled={mutation.busy}
            onPress={() => void mutation.run(() => chooseWorkspace(item.id))}
            style={[styles.card, { borderColor: t.borderPanel, backgroundColor: t.surface }]}
          >
            <Text style={[typography.header, { color: t.text }]}>{item.name}</Text>
            <Text style={[typography.meta, { color: t.muted }]}>
              {item.role}
              {item.people ? ` · ${item.people} people` : ''}
              {item.agents ? ` · ${item.agents} agents` : ''}
            </Text>
          </Pressable>
        )}
        ListFooterComponent={
          <View style={styles.footer}>
            {invitations.length ? (
              <Text style={[typography.meta, { color: t.muted }]}>
                {invitations.length === 1 ? 'An invitation is' : `${invitations.length} invitations are`} waiting. Accept on
                the web or from the email.
              </Text>
            ) : null}
            <ErrorText>{mutation.error}</ErrorText>
            <Button label="Sign out" kind="danger" disabled={mutation.busy} onPress={() => void mutation.run(signOut)} />
          </View>
        }
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  list: { paddingHorizontal: 16, gap: 12, paddingBottom: 24 },
  card: { borderWidth: 1, borderRadius: radius.card, padding: 16, minHeight: touchTarget, gap: 4 },
  footer: { gap: 12, marginTop: 12 },
});
