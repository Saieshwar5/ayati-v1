/**
 * Turning this phone's notifications on and off (mobile plan M4). Both happen
 * only when the person asks; the server keeps the registration with the
 * device session, so signing out removes it.
 */
import type { Client } from '@cuckoo/shared';
import Constants from 'expo-constants';
import * as Device from 'expo-device';
import * as Notifications from 'expo-notifications';
import * as SecureStore from 'expo-secure-store';
import { Platform } from 'react-native';

const ASKED = 'cuckoo.notifications-asked';

/** Why notifications can't be turned on here, or undefined when they can. */
export function unavailable(): string | undefined {
  if (!Device.isDevice) return 'Notifications need a real phone, not a simulator.';
  if (!projectId()) return 'Notifications arrive once the app has its store build.';
  return undefined;
}

function projectId(): string | undefined {
  const extra = Constants.expoConfig?.extra as { eas?: { projectId?: string } } | undefined;
  return Constants.easConfig?.projectId ?? extra?.eas?.projectId;
}

/** Both tokens: Expo's, and the native APNs or FCM token so sending can move later. */
async function tokens(): Promise<{ expo_token: string; native_token?: string }> {
  const expo = await Notifications.getExpoPushTokenAsync({ projectId: projectId() });
  const native = await Notifications.getDevicePushTokenAsync().catch(() => undefined);
  return { expo_token: expo.data, native_token: typeof native?.data === 'string' ? native.data : undefined };
}

export async function turnOn(client: Client): Promise<void> {
  const problem = unavailable();
  if (problem) throw new Error(problem);
  if (Platform.OS === 'android') {
    // Android 13 shows the permission prompt only once a channel exists.
    await Notifications.setNotificationChannelAsync('default', {
      name: 'Cuckoo',
      importance: Notifications.AndroidImportance.HIGH,
    });
  }
  const permission = await Notifications.requestPermissionsAsync();
  if (!permission.granted) throw new Error('Allow notifications for Cuckoo in Settings.');
  await client.registerPush(await tokens());
}

export async function turnOff(client: Client): Promise<void> {
  await client.removePush();
}

/** When the phone gives Cuckoo a new token, the registration follows it. */
export function followTokenChanges(client: Client, enabled: () => boolean) {
  const subscription = Notifications.addPushTokenListener(() => {
    if (enabled()) void tokens().then(next => client.registerPush(next)).catch(() => {});
  });
  return () => subscription.remove();
}

/** The one-time prompt after sign-in (screen 3), never at first launch. */
export async function shouldAsk(): Promise<boolean> {
  if (unavailable()) return false;
  if ((await SecureStore.getItemAsync(ASKED).catch(() => null)) === 'yes') return false;
  const permission = await Notifications.getPermissionsAsync();
  return !permission.granted;
}

export const markAsked = () => SecureStore.setItemAsync(ASKED, 'yes').catch(() => {});
