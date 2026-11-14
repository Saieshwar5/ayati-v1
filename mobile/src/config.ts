import Constants from 'expo-constants';

/**
 * Which Cuckoo the app talks to. EXPO_PUBLIC_ values are built into the app,
 * so they hold addresses only, never secrets.
 * - EXPO_PUBLIC_CUCKOO_ORIGIN: the controller's origin, exactly as the server's
 *   own (AYATI_PUBLIC_ORIGIN), since the server checks the Host header.
 * - EXPO_PUBLIC_CUCKOO_UNIVERSAL_LINKS=1: sign-in returns by the verified
 *   universal link; otherwise by the app's scheme, which the server must name
 *   in AYATI_MOBILE_APP_SCHEME (development).
 */
export const origin = (process.env.EXPO_PUBLIC_CUCKOO_ORIGIN ?? 'http://127.0.0.1:8765').replace(/\/+$/, '');
export const universalLinks = process.env.EXPO_PUBLIC_CUCKOO_UNIVERSAL_LINKS === '1';
export const appScheme = typeof Constants.expoConfig?.scheme === 'string' ? Constants.expoConfig.scheme : 'cuckoo';
