/** Signed-in devices: this phone, and the others this person can sign out. */
import type { Device } from '@cuckoo/shared';
import { useCallback, useEffect } from 'react';
import { StyleSheet, Switch, Text, View } from 'react-native';
import { followTokenChanges, turnOff, turnOn, unavailable } from '../notifications/push-setup';
import { shortTime } from '../chats/time';
import { useSession } from '../session/session-context';
import { type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { Button, ErrorText } from '../ui/controls';
import { useMutation, useResource } from '../ui/requests';

export function Devices() {
  const t = useTokens();
  const { client } = useSession();
  const { data, error, refresh } = useResource(useCallback((signal: AbortSignal) => client.devices(signal), [client]));
  const revoke = useMutation();
  const toggle = useMutation();
  const current = data?.devices.find(device => device.current);
  const on = !!current?.notifications;
  // While on, a new token from the phone moves the registration with it.
  useEffect(() => (on ? followTokenChanges(client, () => true) : undefined), [on, client]);
  const setNotifications = (next: boolean) =>
    toggle.run(async () => {
      await (next ? turnOn(client) : turnOff(client));
      await refresh();
    });
  const reason = unavailable();
  const signOutDevice = (device: Device) =>
    revoke.run(async () => {
      await client.revokeDevice(device.id);
      await refresh();
    });
  return (
    <View style={styles.list}>
      {current ? (
        <View style={[styles.row, { borderBottomColor: t.border }]}>
          <View style={styles.text}>
            <Text style={[typography.body, { color: t.text }]}>Notifications on this phone</Text>
            <Text style={[typography.meta, { color: t.muted }]}>
              {reason ?? 'Who and what kind only, never the message itself.'}
            </Text>
          </View>
          <Switch
            accessibilityLabel="Notifications on this phone"
            value={on}
            disabled={toggle.busy || (!on && !!reason)}
            onValueChange={next => void setNotifications(next)}
          />
        </View>
      ) : null}
      <ErrorText>{toggle.error}</ErrorText>
      <Text accessibilityRole="header" style={[typography.header, { color: t.text, marginTop: 12 }]}>Signed-in devices</Text>
      {(data?.devices ?? []).map(device => (
        <View key={device.id} style={[styles.row, { borderBottomColor: t.border }]}>
          <View style={styles.text}>
            <Text style={[typography.body, { color: t.text }]}>
              {device.name}
              {device.current ? ' · this phone' : ''}
            </Text>
            <Text style={[typography.meta, { color: t.muted }]}>
              {device.platform === 'ios' ? 'iPhone or iPad' : 'Android'} · last used {shortTime(device.last_used)}
            </Text>
          </View>
          {device.current ? null : (
            <Button label="Sign out" kind="outline" disabled={revoke.busy} onPress={() => void signOutDevice(device)} />
          )}
        </View>
      ))}
      <ErrorText>{error || revoke.error}</ErrorText>
    </View>
  );
}

const styles = StyleSheet.create({
  list: { gap: 4 },
  row: { flexDirection: 'row', alignItems: 'center', gap: 12, paddingVertical: 10, borderBottomWidth: StyleSheet.hairlineWidth },
  text: { flex: 1 },
});
