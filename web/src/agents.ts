// Shared agent profile contract. The contact list is owned by React.
export type Profile = { browser_available: boolean; revision: number; editable: boolean; id: string; name: string; role: string; model: string; memory: boolean; browser: boolean; state: string; detail: string };

/** Old daemons may still advertise a previous built-in guide name. */
export function displayAgentName({ id, name }: { id: string; name: string }): string {
  return id === 'local-agent' && ['Ayati', 'Relay'].includes(name) ? 'Cuckoo' : name;
}
