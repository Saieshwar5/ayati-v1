/** An agent's profile, full screen on a phone. */
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { AgentProfileView } from '../../agents/agent-profile-view';
import { useAgents } from '../../agents/use-agents';
import { touchTarget } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { ErrorText } from '../../ui/controls';

export default function AgentScreen() {
  const t = useTokens();
  const router = useRouter();
  const { id } = useLocalSearchParams<{ id: string }>();
  const { data, error } = useAgents();
  return (
    <SafeAreaView edges={['top', 'left', 'right']} style={{ flex: 1, backgroundColor: t.surface }}>
      <Stack.Screen options={{ headerShown: false }} />
      <View style={[styles.header, { borderBottomColor: t.border }]}>
        <Pressable accessibilityRole="button" accessibilityLabel="Back to Agents" onPress={() => router.back()} style={styles.back}>
          <Text style={{ color: t.text, fontSize: 24 }}>‹</Text>
        </Pressable>
      </View>
      <ErrorText>{error}</ErrorText>
      {data ? (
        <AgentProfileView
          agentId={id}
          directory={data.directory}
          chats={data.chats}
          opened={chat => router.push({ pathname: '/chat/[id]', params: { id: chat } })}
        />
      ) : null}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  header: { borderBottomWidth: StyleSheet.hairlineWidth, minHeight: 52, justifyContent: 'center' },
  back: { width: touchTarget, height: touchTarget, alignItems: 'center', justifyContent: 'center' },
});
