/**
 * Screen 15: an agent's profile, as people see it in Chat on the web: face,
 * name, job, who manages it, what it is for and the channels it is in.
 * Its tools and rules are changed on the web.
 */
import type { Conversation, Directory } from '@cuckoo/shared';
import * as Linking from 'expo-linking';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { origin } from '../config';
import { Face } from '../faces/face';
import { useWorkspace } from '../session/session-context';
import { type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { AiTag, Button, ErrorText } from '../ui/controls';
import { useOpenAgent } from './use-open-agent';

type Props = { agentId: string; directory: Directory; chats: Conversation[]; opened: (chat: string) => void };

export function AgentProfileView({ agentId, directory, chats, opened }: Props) {
  const t = useTokens();
  const { workspace } = useWorkspace();
  const opener = useOpenAgent(chats, opened);
  const agent = directory.agents.find(a => a.id === agentId);
  if (!agent) {
    return (
      <View style={styles.page}>
        <Text style={[typography.body, { color: t.muted }]}>This agent is no longer available to you.</Text>
      </View>
    );
  }
  const manager = agent.manager ? directory.users.find(u => u.id === agent.manager)?.name : undefined;
  const channels = chats.filter(chat => chat.kind !== 'direct' && !chat.archived && chat.agents.includes(agent.id));
  return (
    <ScrollView contentContainerStyle={styles.page}>
      <View style={styles.identity}>
        <Face name={agent.name} agent={agent} size={88} workspace={workspace.id} />
        <View style={styles.name}>
          <Text accessibilityRole="header" style={[typography.largeTitle, { color: t.text }]}>{agent.name}</Text>
          <AiTag />
        </View>
        {agent.title ? <Text style={[typography.body, { color: t.muted }]}>{agent.title}</Text> : null}
        {manager ? <Text style={[typography.meta, { color: t.muted }]}>Managed by {manager}</Text> : null}
      </View>
      <Button label={`Message ${agent.name}`} busy={opener.busy} onPress={() => void opener.open(agent)} />
      <ErrorText>{opener.error}</ErrorText>
      {agent.about ? (
        <View style={styles.block}>
          <Text accessibilityRole="header" style={[typography.header, { color: t.text }]}>What it does</Text>
          <Text style={[typography.body, { color: t.text }]}>{agent.about}</Text>
        </View>
      ) : null}
      <View style={styles.block}>
        <Text accessibilityRole="header" style={[typography.header, { color: t.text }]}>Channels</Text>
        <Text style={[typography.meta, { color: t.muted }]}>
          {channels.length ? channels.map(chat => `# ${chat.name}`).join(' · ') : 'Not in any of your channels.'}
        </Text>
      </View>
      <Pressable accessibilityRole="link" onPress={() => void Linking.openURL(`${origin}/w/${workspace.id}/`)} style={styles.web}>
        <Text style={[typography.meta, { color: t.muted }]}>What it can use and what it asks first are set on the web ›</Text>
      </Pressable>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  page: { padding: 16, gap: 16 },
  identity: { alignItems: 'center', gap: 6, paddingVertical: 8 },
  name: { flexDirection: 'row', alignItems: 'center' },
  block: { gap: 6 },
  web: { paddingVertical: 8 },
});
