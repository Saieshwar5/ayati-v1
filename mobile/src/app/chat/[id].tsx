/** Screens 8 and 10: a conversation with an agent, a person or a channel. */
import { conversationKind, conversationName, directAgent, type Message } from '@cuckoo/shared';
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useCallback, useMemo } from 'react';
import { ActivityIndicator, FlatList, KeyboardAvoidingView, Platform, Pressable, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { Composer } from '../../chats/composer';
import { MessageBubble } from '../../chats/message-bubble';
import { useConversation } from '../../chats/use-conversation';
import { useWorkspace } from '../../session/session-context';
import { touchTarget, type as typography } from '../../theme/tokens';
import { useTokens } from '../../theme/use-tokens';
import { ConnectionBanner } from '../../ui/connection-banner';
import { AiTag, Button, ErrorText } from '../../ui/controls';

export default function ConversationScreen() {
  const t = useTokens();
  const router = useRouter();
  const { id } = useLocalSearchParams<{ id: string }>();
  const { api, user } = useWorkspace();
  const { loaded, error, reload, loadOlder, add } = useConversation(id);
  // Newest first, for the inverted list that starts at the bottom.
  const newestFirst = useMemo(() => (loaded ? [...loaded.messages].reverse() : []), [loaded]);
  const agents = useMemo(() => new Set(loaded?.directory.agents.map(a => a.id)), [loaded?.directory]);
  const send = useCallback((submission: Parameters<typeof api.send>[1]) => api.send(id, submission), [api, id]);
  const renderItem = useCallback(
    ({ item }: { item: Message }) => <MessageBubble message={item} mine={item.author === user.id} agent={agents.has(item.author)} />,
    [agents, user.id],
  );

  const name = loaded ? conversationName(loaded.chat, loaded.directory, user.id) : '';
  const kind = loaded ? conversationKind(loaded.chat, loaded.directory, user.id) : undefined;
  const agent = loaded ? directAgent(loaded.chat, loaded.directory, user.id) : undefined;
  const title = kind === 'Channel' ? `# ${name}` : name;
  const subtitle =
    kind === 'Channel'
      ? `${loaded!.chat.users.length} people · ${loaded!.chat.agents.length} agents`
      : kind === 'Agent'
        ? (loaded!.directory.agents.find(a => a.id === agent)?.title ?? 'Agent')
        : undefined;

  return (
    <SafeAreaView edges={['top', 'left', 'right', 'bottom']} style={[styles.page, { backgroundColor: t.surface }]}>
      <Stack.Screen options={{ headerShown: false }} />
      <View style={[styles.header, { borderBottomColor: t.border }]}>
        <Pressable accessibilityRole="button" accessibilityLabel="Back to Chats" onPress={() => router.back()} style={styles.back}>
          <Text style={{ color: t.text, fontSize: 24 }}>‹</Text>
        </Pressable>
        <View style={styles.headerText}>
          <View style={styles.titleRow}>
            <Text numberOfLines={1} accessibilityRole="header" style={[typography.header, { color: t.text, flexShrink: 1 }]}>
              {title || 'Conversation'}
            </Text>
            {kind === 'Agent' ? <AiTag /> : null}
          </View>
          {subtitle ? <Text numberOfLines={1} style={[typography.small, { color: t.muted }]}>{subtitle}</Text> : null}
        </View>
      </View>
      <ConnectionBanner />
      <KeyboardAvoidingView style={styles.page} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
        {!loaded ? (
          <View style={styles.center}>
            {error ? (
              <>
                <ErrorText>{error}</ErrorText>
                <Button label="Try again" kind="outline" onPress={() => void reload()} />
              </>
            ) : (
              <ActivityIndicator accessibilityLabel="Loading messages" color={t.muted} />
            )}
          </View>
        ) : !newestFirst.length ? (
          <View style={styles.center}>
            <Text style={[typography.body, { color: t.muted }]}>
              {kind === 'Agent' ? `Ask ${name} anything.` : 'No messages yet.'}
            </Text>
          </View>
        ) : (
          <FlatList
            inverted
            data={newestFirst}
            keyExtractor={message => message.id}
            renderItem={renderItem}
            onEndReached={() => void loadOlder()}
            onEndReachedThreshold={0.4}
            keyboardDismissMode="interactive"
            contentContainerStyle={styles.list}
          />
        )}
        <Composer
          placeholder={kind === 'Agent' ? `Message ${name}` : kind === 'Channel' ? `Message ${title}` : 'Message'}
          disabled={!loaded || loaded.chat.archived}
          agents={agent ? [agent] : []}
          send={send}
          sent={add}
        />
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1 },
  header: { flexDirection: 'row', alignItems: 'center', paddingRight: 16, borderBottomWidth: StyleSheet.hairlineWidth, minHeight: 52 },
  back: { width: touchTarget, height: touchTarget, alignItems: 'center', justifyContent: 'center' },
  headerText: { flex: 1 },
  titleRow: { flexDirection: 'row', alignItems: 'center' },
  center: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 12, padding: 24 },
  list: { paddingVertical: 8 },
});
