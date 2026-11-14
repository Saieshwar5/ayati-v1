/**
 * Sign-in in the system browser (runtime/mobile-sign-in.md): PKCE pair, the
 * auth session, then the code exchanged for a device-session token.
 */
import { makePkce, readReturn, returnUrl, startUrl, type Client, type DeviceSession } from '@cuckoo/shared';
import * as Crypto from 'expo-crypto';
import * as Device from 'expo-device';
import * as WebBrowser from 'expo-web-browser';
import { Platform } from 'react-native';
import { appScheme, origin, universalLinks } from '../config';

export class SignInCancelled extends Error {
  constructor() {
    super('Sign-in was cancelled.');
    this.name = 'SignInCancelled';
  }
}

async function sha256(text: string): Promise<Uint8Array> {
  return new Uint8Array(await Crypto.digest(Crypto.CryptoDigestAlgorithm.SHA256, new TextEncoder().encode(text)));
}

/** A readable name for Profile › Devices, within the server's 80 characters. */
export function deviceName(): string {
  const name = (Device.deviceName ?? Device.modelName ?? '').trim();
  return (name || (Platform.OS === 'ios' ? 'iPhone' : 'Android phone')).slice(0, 80);
}

export async function signInWithBrowser(client: Client, options: { google?: boolean } = {}): Promise<DeviceSession> {
  const pkce = await makePkce(count => Crypto.getRandomBytes(count), sha256);
  const redirect = returnUrl(origin, universalLinks ? undefined : appScheme);
  const result = await WebBrowser.openAuthSessionAsync(startUrl(origin, pkce.challenge, options), redirect, {
    preferUniversalLinks: universalLinks,
  });
  if (result.type !== 'success') throw new SignInCancelled();
  const back = readReturn(result.url);
  if ('error' in back) throw new Error('Sign-in did not finish. Try again.');
  return client.exchange({
    code: back.code,
    verifier: pkce.verifier,
    device_name: deviceName(),
    platform: Platform.OS === 'ios' ? 'ios' : 'android',
  });
}
