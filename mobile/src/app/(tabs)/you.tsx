/** Screen 16: You. Settings, people and usage stay on the web. */
import * as Linking from 'expo-linking';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { origin } from '../../config';
import { useSession, useWorkspace } from '../../session/session-context';
import { touchTarget, type as typography } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { Button, ErrorText, Face } from '../../ui/controls';
import { useMutation } from '../../ui/requests';
import { Screen } from '../../ui/screen';
import { Devices } from '../../you/devices';

export default function You() {
  const t = useTokens();
  const { signOut, chooseWorkspace } = useSession();
  const { user, workspace } = useWorkspace();
  const mutation = useMutation();
  const name = user.display_name || user.email;
  return (
    <Screen title="You">
      <ScrollView contentContainerStyle={styles.page}>
        <View style={styles.profile}>
          <Face name={name} size={64} />
          <View style={styles.text}>
            <Text style={[typography.header, { color: t.text }]}>{name}</Text>
            <Text style={[typography.meta, { color: t.muted }]}>{user.email}</Text>
            <Text style={[typography.meta, { color: t.muted }]}>{workspace.role} · {workspace.name}</Text>
          </View>
        </View>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={`Workspace ${workspace.name}. Switch workspace`}
          onPress={() => void mutation.run(() => chooseWorkspace(undefined))}
          style={[styles.item, { borderColor: t.border }]}
        >
          <Text style={[typography.body, { color: t.text }]}>Workspace</Text>
          <Text style={[typography.meta, { color: t.muted }]}>{workspace.name} · Switch ›</Text>
        </Pressable>
        <Devices />
        <Pressable
          accessibilityRole="link"
          onPress={() => void Linking.openURL(`${origin}/w/${workspace.id}/`)}
          style={[styles.item, { borderColor: t.border }]}
        >
          <Text style={[typography.body, { color: t.text }]}>Open Cuckoo on the web</Text>
          <Text style={[typography.meta, { color: t.muted }]}>›</Text>
        </Pressable>
        <ErrorText>{mutation.error}</ErrorText>
        <Button label="Sign out" kind="danger" busy={mutation.busy} onPress={() => void mutation.run(signOut)} />
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  page: { padding: 16, gap: 16 },
  profile: { flexDirection: 'row', alignItems: 'center', gap: 14 },
  text: { flex: 1, gap: 2 },
  item: { minHeight: touchTarget, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', borderBottomWidth: StyleSheet.hairlineWidth, paddingVertical: 8 },
});
