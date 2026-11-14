/**
 * What the app keeps between launches, in the phone's Keychain or Keystore:
 * the device-session token, and the workspace each person last chose.
 */
import * as SecureStore from 'expo-secure-store';

const TOKEN = 'cuckoo.device-token';
const workspaceKey = (user: string) => `cuckoo.workspace.${user.replace(/[^A-Za-z0-9._-]/g, '_')}`;

export const readToken = () => SecureStore.getItemAsync(TOKEN);
export const saveToken = (token: string) => SecureStore.setItemAsync(TOKEN, token);
export const forgetToken = () => SecureStore.deleteItemAsync(TOKEN);

export const readWorkspace = (user: string) => SecureStore.getItemAsync(workspaceKey(user));
export const saveWorkspace = (user: string, workspace: string) => SecureStore.setItemAsync(workspaceKey(user), workspace);
export const forgetWorkspace = (user: string) => SecureStore.deleteItemAsync(workspaceKey(user));
