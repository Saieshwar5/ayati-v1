/**
 * Opening the exact place a notification is about. While the app is open, no
 * banner is shown: the live stream already shows the message. A tap names a
 * workspace and an item; the app switches workspace if needed (only to one
 * the person is still in) and opens it, where access is checked again.
 */
import { readTarget, type NotificationTarget } from '@cuckoo/shared';
import * as Notifications from 'expo-notifications';
import { useRouter } from 'expo-router';
import { useEffect, useState } from 'react';
import { useSession } from '../session/session-context';

Notifications.setNotificationHandler({
  handleNotification: async () => ({
    shouldShowBanner: false,
    shouldShowList: true,
    shouldPlaySound: false,
    shouldSetBadge: false,
  }),
});

export function pathFor(target: NotificationTarget) {
  if (target.kind === 'approval') return { pathname: '/approval/[id]' as const, params: { id: target.approval } };
  if (target.kind === 'question') return { pathname: '/question/[id]' as const, params: { id: target.question } };
  return { pathname: '/chat/[id]' as const, params: { id: target.chat } };
}

export function NotificationRouting() {
  const { state, chooseWorkspace } = useSession();
  const router = useRouter();
  const response = Notifications.useLastNotificationResponse();
  const [pending, setPending] = useState<NotificationTarget>();

  useEffect(() => {
    if (!response || response.actionIdentifier !== Notifications.DEFAULT_ACTION_IDENTIFIER) return;
    setPending(readTarget(response.notification.request.content.data));
    void Notifications.clearLastNotificationResponseAsync().catch(() => {});
  }, [response]);

  useEffect(() => {
    if (!pending || state.status !== 'signed-in') return;
    if (state.workspace?.id !== pending.workspace) {
      if (state.session.workspaces?.some(w => w.id === pending.workspace)) void chooseWorkspace(pending.workspace);
      else setPending(undefined);
      return;
    }
    router.push(pathFor(pending));
    setPending(undefined);
  }, [pending, state, chooseWorkspace, router]);

  return null;
}
